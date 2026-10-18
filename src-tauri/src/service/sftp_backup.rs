//! SFTP 备份存储：复用 SSH 会话栈（凭证解密 / known_hosts TOFU 校验 / 跳板机），
//! 备份是低频操作，每次操作新建连接、用完断开。

use std::sync::atomic::{AtomicBool, Ordering};

use russh_sftp::client::error::Error as SftpError;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::StatusCode;
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};
use crate::models::DbPool;
use crate::service::host as host_svc;
use crate::service::sftp as sftp_svc;
use crate::service::ssh::Session;
use tokio::io::AsyncWriteExt;

/// SFTP 备份配置：引用已保存的 SSH 主机（凭证/主机密钥/跳板机均复用主机配置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpConfig {
    pub host_id: i64,
    #[serde(default = "default_path_prefix")]
    pub path_prefix: String,
}

impl Default for SftpConfig {
    fn default() -> Self {
        Self {
            host_id: 0,
            path_prefix: default_path_prefix(),
        }
    }
}

fn default_path_prefix() -> String {
    "ashell/".to_string()
}

pub struct SftpStorage {
    session: Session,
    sftp: SftpSession,
    /// 逐级建目录在一个存储实例内只需执行一次
    dirs_ensured: AtomicBool,
}

impl SftpStorage {
    /// 连接所选主机并打开 SFTP 子系统。
    ///
    /// 凭证由 `host::get_with_credentials` 解密；known_hosts TOFU 校验
    /// （未信任主机返回 HostKeyVerify）与跳板机在 `Session::connect` 内自动生效。
    pub async fn connect(
        pool: &DbPool,
        crypto_key: &[u8; 32],
        cfg: &SftpConfig,
    ) -> AppResult<Self> {
        if cfg.host_id <= 0 {
            return Err(AppError::Internal(
                "sftp backup host is not selected".into(),
            ));
        }
        let host = host_svc::get_with_credentials(pool, crypto_key, cfg.host_id)
            .await
            .map_err(|_| {
                AppError::Internal(format!(
                    "sftp backup host (id={}) not found, please re-select",
                    cfg.host_id
                ))
            })?;
        if host.protocol != "ssh" {
            return Err(AppError::Internal(
                "sftp backup host must use SSH protocol".into(),
            ));
        }
        let session = Session::connect(pool, crypto_key, &host).await?;
        let channel = session.channel_open_session().await?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| sftp_svc::sftp_err("request subsystem", e))?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| sftp_svc::sftp_err("init", e))?;
        Ok(Self {
            session,
            sftp,
            dirs_ensured: AtomicBool::new(false),
        })
    }

    pub async fn put(&self, key: &str, data: &[u8]) -> AppResult<()> {
        self.ensure_parent_dirs(key).await?;
        // 注意不能用 SftpSession::write：它只带 WRITE flags、不创建文件
        // （russh-sftp 3.x 行为与文档名不符），新文件会报 No such file。
        // 这里 create（CREATE|TRUNCATE|WRITE）后手动写入并关闭。
        let mut file = self
            .sftp
            .create(key)
            .await
            .map_err(|e| sftp_svc::sftp_err("create", e))?;
        if let Err(e) = file.write_all(data).await {
            let _ = file.close().await;
            return Err(sftp_svc::sftp_err("write", e));
        }
        file.close()
            .await
            .map_err(|e| sftp_svc::sftp_err("close", e))
    }

    /// NoSuchFile → None（文件不存在），2xx 语义由 SFTP 协议保证
    pub async fn get(&self, key: &str) -> AppResult<Option<Vec<u8>>> {
        match self.sftp.read(key).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => {
                Ok(None)
            }
            Err(e) => Err(sftp_svc::sftp_err("read", e)),
        }
    }

    pub async fn delete(&self, key: &str) -> AppResult<()> {
        match self.sftp.remove_file(key).await {
            Ok(()) => Ok(()),
            Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => {
                Ok(())
            }
            Err(e) => Err(sftp_svc::sftp_err("remove_file", e)),
        }
    }

    pub async fn disconnect(&self) {
        self.session.disconnect().await;
    }

    /// 上传前确保 key 的父目录存在（逐级 try_exists + create_dir，
    /// 与 sftp.rs::mkdir 同一语义）
    async fn ensure_parent_dirs(&self, key: &str) -> AppResult<()> {
        if self.dirs_ensured.load(Ordering::Relaxed) {
            return Ok(());
        }
        self.dirs_ensured.store(true, Ordering::Relaxed);
        for dir in parent_dirs(key) {
            match self.sftp.try_exists(&dir).await {
                Ok(true) => continue,
                Ok(false) => {
                    self.sftp
                        .create_dir(&dir)
                        .await
                        .map_err(|e| sftp_svc::sftp_err(&format!("create_dir {dir}"), e))?;
                }
                Err(e) => return Err(sftp_svc::sftp_err(&format!("exists {dir}"), e)),
            }
        }
        Ok(())
    }
}

/// key（最后一段为文件名）的待创建父目录序列，含中间级、由浅到深。
/// 绝对路径保留根前缀（"/a/b" → ["/a", "/a/b"]），相对路径保持相对。
fn parent_dirs(key: &str) -> Vec<String> {
    let Some(idx) = key.rfind('/') else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    let mut current = String::new();
    for comp in key[..idx].split('/') {
        if comp.is_empty() {
            // 绝对路径开头的空段
            if current.is_empty() {
                current.push('/');
            }
            continue;
        }
        if !current.is_empty() && !current.ends_with('/') {
            current.push('/');
        }
        current.push_str(comp);
        dirs.push(current.clone());
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_dirs_of_relative_key() {
        assert_eq!(
            parent_dirs("ashell/ashell-backup-2026.json"),
            vec!["ashell".to_string()]
        );
        assert_eq!(
            parent_dirs("backup/sub/x.json"),
            vec!["backup".to_string(), "backup/sub".to_string()]
        );
    }

    #[test]
    fn parent_dirs_of_absolute_key() {
        assert_eq!(
            parent_dirs("/home/user/backup/x.json"),
            vec!["/home".to_string(), "/home/user".to_string(), "/home/user/backup".to_string()]
        );
    }

    #[test]
    fn parent_dirs_of_bare_filename() {
        assert!(parent_dirs("manifest.json").is_empty());
    }

    #[test]
    fn sftp_config_defaults() {
        let cfg: SftpConfig = serde_json::from_str(r#"{"host_id": 3}"#).unwrap();
        assert_eq!(cfg.host_id, 3);
        assert_eq!(cfg.path_prefix, "ashell/");
    }

    /// 真实链路往返：连接（含 TOFU 主机密钥信任）、逐级建目录、put/get/delete。
    /// 需要本地 SFTP 服务器（任意密码认证），端口可用环境变量覆盖：
    ///   ASHELL_SFTP_TEST_PORT=9900
    /// 运行：cargo test --lib sftp -- --ignored
    #[tokio::test]
    #[ignore = "requires a local SFTP server (any-password auth) on 127.0.0.1"]
    async fn roundtrip_against_live_server() {
        use crate::config;
        use crate::service::known_host;

        let port: u16 = std::env::var("ASHELL_SFTP_TEST_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(9900);

        let dir = tempfile::tempdir().unwrap();
        let pool = crate::models::init_pool(&dir.path().join("t.db"))
            .await
            .expect("init pool");
        let crypto_key = [5u8; 32];

        // 插入备份主机：密码按本地加密存储，Session::authenticate 走密码认证
        let enc = config::crypto::encrypt(&crypto_key, "test-pass").unwrap();
        sqlx::query(
            "INSERT INTO hosts (name, addr, port, username, password, protocol) \
             VALUES ('sftp-test', '127.0.0.1', ?, 'test', ?, 'ssh')",
        )
        .bind(port.to_string())
        .bind(&enc)
        .execute(&pool)
        .await
        .unwrap();
        let host_id: i64 =
            sqlx::query_scalar("SELECT id FROM hosts").fetch_one(&pool).await.unwrap();

        let cfg = SftpConfig {
            host_id,
            path_prefix: "ashell/".into(),
        };

        // 首连触发 TOFU：捕获服务器指纹并 trust 后重连
        match SftpStorage::connect(&pool, &crypto_key, &cfg).await {
            Ok(storage) => storage.disconnect().await,
            Err(AppError::HostKeyVerify {
                addr,
                port: verify_port,
                key_type,
                fingerprint,
                ..
            }) => {
                assert_eq!(addr, "127.0.0.1");
                assert_eq!(verify_port, port);
                known_host::trust(&pool, &addr, verify_port, &key_type, &fingerprint)
                    .await
                    .unwrap();
            }
            Err(e) => panic!("unexpected connect error: {e}"),
        }

        let storage = SftpStorage::connect(&pool, &crypto_key, &cfg)
            .await
            .expect("second connect after trusting host key");

        // put 前逐级 MKCOL（ashell/、ashell/sub/ 均不存在）
        storage
            .put("ashell/sub/backup.json", b"hello-backup")
            .await
            .expect("put");

        let got = storage.get("ashell/sub/backup.json").await.unwrap();
        assert_eq!(got.as_deref(), Some(&b"hello-backup"[..]));

        let missing = storage.get("ashell/missing.json").await.unwrap();
        assert!(missing.is_none());

        storage.delete("ashell/sub/backup.json").await.unwrap();
        let deleted = storage.get("ashell/sub/backup.json").await.unwrap();
        assert!(deleted.is_none());

        storage.disconnect().await;
        pool.close().await;
    }
}
