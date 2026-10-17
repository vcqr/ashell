use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

/// WebDAV 备份配置：服务器地址 + Basic 认证 + 远端存储目录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebDavConfig {
    pub url: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_path_prefix")]
    pub path_prefix: String,
}

impl Default for WebDavConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            username: String::new(),
            password: String::new(),
            path_prefix: default_path_prefix(),
        }
    }
}

fn default_path_prefix() -> String {
    "ashell/".to_string()
}

/// 基于 reqwest 的极简 WebDAV 客户端：只需 PUT/GET/DELETE/MKCOL。
/// 备份列表以远端 manifest.json 为准（与 S3 一致），无需 PROPFIND/XML 解析。
pub struct WebDavClient {
    http: Client,
    /// 规范化后的服务器根地址（去尾部斜杠），如 https://dav.jianguoyun.com/dav
    base_url: String,
    username: String,
    password: String,
    /// 逐级 MKCOL 建目录在一个客户端实例内只需成功执行一次
    dirs_ensured: AtomicBool,
}

impl WebDavClient {
    pub fn new(cfg: &WebDavConfig) -> AppResult<Self> {
        let base = cfg.url.trim();
        if base.is_empty() {
            return Err(AppError::Internal("webdav url is required".into()));
        }
        let mut builder = Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(120));
        // 代理决策与 S3（create_bucket）同一套：
        // - Proxy：注入应用代理（reqwest 默认只读环境变量，不读应用内配置）
        // - Direct：显式禁用一切代理（含环境变量回退）
        // - Auto：保持 reqwest 默认行为
        match crate::proxy::effective() {
            crate::proxy::EffectiveProxy::Proxy { url, no_proxy } => {
                let mut proxy = reqwest::Proxy::all(&url)
                    .map_err(|e| AppError::Internal(format!("proxy url: {e}")))?;
                proxy = proxy.no_proxy(reqwest::NoProxy::from_string(&no_proxy));
                builder = builder.proxy(proxy);
            }
            crate::proxy::EffectiveProxy::Direct => {
                builder = builder.no_proxy();
            }
            crate::proxy::EffectiveProxy::Auto => {}
        }
        let http = builder
            .build()
            .map_err(|e| AppError::Internal(format!("webdav client: {e}")))?;
        Ok(Self {
            http,
            base_url: base.trim_end_matches('/').to_string(),
            username: cfg.username.trim().to_string(),
            password: cfg.password.clone(),
            dirs_ensured: AtomicBool::new(false),
        })
    }

    fn file_url(&self, key: &str) -> String {
        format!("{}/{}", self.base_url, key.trim_start_matches('/'))
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if self.username.is_empty() {
            req
        } else {
            req.basic_auth(&self.username, Some(&self.password))
        }
    }

    /// 目标文件 URL 的所有父级集合逐级 MKCOL（含 base 的路径段），已存在(405)等
    /// 错误一律忽略——真正失败会在后续 PUT 上暴露。
    async fn ensure_parent_dirs(&self, key: &str) {
        if self.dirs_ensured.load(Ordering::Relaxed) {
            return;
        }
        self.dirs_ensured.store(true, Ordering::Relaxed);
        let Ok(url) = reqwest::Url::parse(&self.file_url(key)) else {
            return;
        };
        let segments: Vec<&str> = url.path().split('/').filter(|s| !s.is_empty()).collect();
        if segments.len() <= 1 {
            return;
        }
        let origin = url.origin().ascii_serialization();
        let mkcol = Method::from_bytes(b"MKCOL").expect("MKCOL is a valid method");
        let mut acc = String::new();
        for seg in &segments[..segments.len() - 1] {
            acc.push('/');
            acc.push_str(seg);
            let collection = format!("{origin}{acc}");
            let _ = self
                .auth(self.http.request(mkcol.clone(), &collection))
                .send()
                .await;
        }
    }

    pub async fn put(&self, key: &str, data: &[u8]) -> AppResult<()> {
        self.ensure_parent_dirs(key).await;
        let status = self
            .auth(self.http.put(self.file_url(key)).body(data.to_vec()))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("webdav put: {e}")))?
            .status();
        if !status.is_success() {
            return Err(AppError::Internal(format!(
                "webdav put status: {}",
                status.as_u16()
            )));
        }
        Ok(())
    }

    /// 404 → None（文件不存在），2xx → 文件内容，其余（含 401/403）→ 错误
    pub async fn get(&self, key: &str) -> AppResult<Option<Vec<u8>>> {
        let resp = self
            .auth(self.http.get(self.file_url(key)))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("webdav get: {e}")))?;
        let status = resp.status();
        if status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(AppError::Internal(format!(
                "webdav get status: {}",
                status.as_u16()
            )));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::Internal(format!("webdav get body: {e}")))?;
        Ok(Some(bytes.to_vec()))
    }

    pub async fn delete(&self, key: &str) -> AppResult<()> {
        let status = self
            .auth(self.http.delete(self.file_url(key)))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("webdav delete: {e}")))?
            .status();
        if status == StatusCode::NOT_FOUND || status.is_success() {
            return Ok(());
        }
        Err(AppError::Internal(format!(
            "webdav delete status: {}",
            status.as_u16()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_url_joins_base_and_key() {
        let client = WebDavClient::new(&WebDavConfig {
            url: "https://dav.example.com/dav/".into(),
            username: "u".into(),
            password: "p".into(),
            path_prefix: "ashell/".into(),
        })
        .unwrap();
        assert_eq!(
            client.file_url("ashell/manifest.json"),
            "https://dav.example.com/dav/ashell/manifest.json"
        );
    }

    #[test]
    fn empty_url_rejected() {
        assert!(WebDavClient::new(&WebDavConfig::default()).is_err());
    }

    /// 真实 HTTP 链路往返（MKCOL 建目录 / PUT / GET / DELETE）。
    /// 需要本地 WebDAV 服务器：
    ///   wsgidav --host 127.0.0.1 --port 9980 --root <某空目录> --auth anonymous
    /// 运行：cargo test --lib webdav -- --ignored
    #[tokio::test]
    #[ignore = "requires a local WebDAV server (wsgidav) on 127.0.0.1:9980"]
    async fn roundtrip_against_live_server() {
        let client = WebDavClient::new(&WebDavConfig {
            url: "http://127.0.0.1:9980/".into(),
            username: String::new(),
            password: String::new(),
            path_prefix: "ashell/".into(),
        })
        .unwrap();

        // put 前逐级 MKCOL（ashell/、ashell/sub/ 均不存在）
        client
            .put("ashell/sub/test-backup.json", b"hello-backup")
            .await
            .unwrap();

        let got = client.get("ashell/sub/test-backup.json").await.unwrap();
        assert_eq!(got.as_deref(), Some(&b"hello-backup"[..]));

        let missing = client.get("ashell/missing.json").await.unwrap();
        assert!(missing.is_none());

        client.delete("ashell/sub/test-backup.json").await.unwrap();
        let deleted = client.get("ashell/sub/test-backup.json").await.unwrap();
        assert!(deleted.is_none());
    }
}
