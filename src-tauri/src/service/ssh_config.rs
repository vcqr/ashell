use serde::Serialize;

/// 从 ~/.ssh/config 解析出的单个主机条目
#[derive(Debug, Clone, Serialize)]
pub struct SshConfigHost {
    /// Host 别名（ssh config 中的 Host 名称）
    pub name: String,
    /// 实际地址（HostName；未指定时回退为 name）
    pub addr: String,
    pub port: String,
    pub username: Option<String>,
    /// 私钥文件路径（已展开 ~）
    pub identity_file: Option<String>,
}

/// 读取并解析 ~/.ssh/config，返回可导入的主机列表。
///
/// - 跳过含通配符（`*` / `?`）的 Host 条目
/// - 展开 IdentityFile 中的 `~`
/// - 不处理 Include 指令
pub fn parse_ssh_config() -> std::io::Result<Vec<SshConfigHost>> {
    let home = dirs::home_dir().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "无法获取用户家目录")
    })?;
    let path = home.join(".ssh").join("config");

    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&path)?;
    Ok(parse_content(&content, &home))
}

fn parse_content(content: &str, home: &std::path::Path) -> Vec<SshConfigHost> {
    let mut hosts = Vec::new();
    // 当前块的累积配置
    let mut cur_names: Vec<String> = Vec::new();
    let mut cur_host_name: Option<String> = None;
    let mut cur_port: Option<String> = None;
    let mut cur_user: Option<String> = None;
    let mut cur_identity: Option<String> = None;
    let mut has_host = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // 拆分 key value（支持 "Key Value" 和 "Key=Value"）
        let (key, value) = match line.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => {
                let mut parts = line.splitn(2, char::is_whitespace);
                let k = parts.next().unwrap_or("").trim();
                let v = parts.next().unwrap_or("").trim();
                (k, v)
            }
        };
        let key_lower = key.to_ascii_lowercase();

        if key_lower == "host" {
            // 刷新上一个块
            if has_host {
                flush_block(
                    &mut hosts,
                    &cur_names,
                    &cur_host_name,
                    &cur_port,
                    &cur_user,
                    &cur_identity,
                    home,
                );
            }
            // 开始新块
            cur_names = value
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
            cur_host_name = None;
            cur_port = None;
            cur_user = None;
            cur_identity = None;
            has_host = true;
            continue;
        }

        if !has_host {
            continue;
        }

        match key_lower.as_str() {
            "hostname" => cur_host_name = Some(value.to_string()),
            "port" => cur_port = Some(value.to_string()),
            "user" => cur_user = Some(value.to_string()),
            "identityfile" => {
                if cur_identity.is_none() {
                    cur_identity = Some(expand_tilde(value, home));
                }
            }
            _ => {}
        }
    }

    // 最后一个块
    if has_host {
        flush_block(
            &mut hosts,
            &cur_names,
            &cur_host_name,
            &cur_port,
            &cur_user,
            &cur_identity,
            home,
        );
    }

    hosts
}

/// 将一个 Host 块的累积配置写入结果列表，跳过含通配符的名称
fn flush_block(
    hosts: &mut Vec<SshConfigHost>,
    names: &[String],
    host_name: &Option<String>,
    port: &Option<String>,
    user: &Option<String>,
    identity: &Option<String>,
    home: &std::path::Path,
) {
    for name in names {
        if name.contains('*') || name.contains('?') {
            continue;
        }
        hosts.push(SshConfigHost {
            name: name.clone(),
            addr: host_name.clone().unwrap_or_else(|| name.clone()),
            port: port.clone().unwrap_or_else(|| "22".to_string()),
            username: user.clone(),
            identity_file: identity
                .as_ref()
                .map(|p| expand_tilde(p, home)),
        });
    }
}

/// 展开 `~` 为 home 目录
fn expand_tilde(path: &str, home: &std::path::Path) -> String {
    if path == "~" {
        return home.to_string_lossy().into_owned();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home.join(rest).to_string_lossy().into_owned();
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_basic_parse() {
        let content = r#"
Host myserver
  HostName 10.0.0.1
  Port 2222
  User root
  IdentityFile ~/.ssh/id_rsa

Host github.com
  User git

Host *
  User default
"#;
        let home = PathBuf::from("/home/user");
        let hosts = parse_content(content, &home);

        assert_eq!(hosts.len(), 2);

        assert_eq!(hosts[0].name, "myserver");
        assert_eq!(hosts[0].addr, "10.0.0.1");
        assert_eq!(hosts[0].port, "2222");
        assert_eq!(hosts[0].username.as_deref(), Some("root"));
        assert_eq!(hosts[0].identity_file.as_deref(), Some("/home/user/.ssh/id_rsa"));

        assert_eq!(hosts[1].name, "github.com");
        assert_eq!(hosts[1].addr, "github.com");
        assert_eq!(hosts[1].port, "22");
        assert_eq!(hosts[1].username.as_deref(), Some("git"));
        assert!(hosts[1].identity_file.is_none());
    }

    #[test]
    fn test_equals_syntax() {
        let content = "Host srv\n  HostName=192.168.1.1\n  Port=22\n";
        let home = PathBuf::from("/home/user");
        let hosts = parse_content(content, &home);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].addr, "192.168.1.1");
        assert_eq!(hosts[0].port, "22");
    }

    #[test]
    fn test_skip_wildcard() {
        let content = "Host *.example.com\n  User deploy\n";
        let home = PathBuf::from("/home/user");
        let hosts = parse_content(content, &home);
        assert!(hosts.is_empty());
    }

    #[test]
    fn test_mixed_wildcard() {
        let content = "Host foo *.internal\n  HostName 10.0.0.5\n";
        let home = PathBuf::from("/home/user");
        let hosts = parse_content(content, &home);
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].name, "foo");
    }
}
