//! 本地文件系统访问（SFTP 双栏的本地侧）。
//!
//! 提供"列目录"与两个直传能力："远端 -> 本地落盘"、"本地文件 -> 远端"，
//! 传输在 Rust 进程内流式完成，大文件不经过 webview 内存。
//! 不提供本地文件的删除/改名：本地文件管理交给操作系统，AShell 的
//! 本地侧只作为传输的源与目的地，避免出现两套语义不同的删除/回收站行为。

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::errors::{AppError, AppResult};
use crate::service::sftp::{self, SftpFileAttr, SftpListResp};

fn io_err(context: &str, e: std::io::Error) -> AppError {
    AppError::BadRequest(format!("{context}: {e}"))
}

/// 只接受绝对路径。本地浏览端点等于给 webview 开了列本机目录的能力，
/// 虽然有本机 token 鉴权，仍拒绝相对路径以避免 cwd 相关的歧义。
fn validate_absolute(p: &str) -> AppResult<PathBuf> {
    let path = PathBuf::from(p);
    if !path.is_absolute() {
        return Err(AppError::BadRequest(format!("path must be absolute: {p}")));
    }
    Ok(path)
}

fn mtime_secs(md: &std::fs::Metadata) -> Option<u32> {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as u32)
}

#[cfg(unix)]
fn permission_string(md: &std::fs::Metadata) -> String {
    use std::os::unix::fs::PermissionsExt;
    let mode = md.permissions().mode();
    format!("{:o}", mode & 0o7777)
}

#[cfg(not(unix))]
fn permission_string(_md: &std::fs::Metadata) -> String {
    String::new()
}

/// 列出本地目录。path 为空时返回家目录。
/// 返回结构复用 SftpListResp，前端本地栏可直接复用远程列表的渲染逻辑。
pub async fn list(path: Option<String>) -> AppResult<SftpListResp> {
    let dir: PathBuf = match path {
        Some(p) if !p.trim().is_empty() => validate_absolute(&p)?,
        _ => dirs::home_dir()
            .ok_or_else(|| AppError::Internal("无法获取用户家目录".into()))?,
    };

    let display_path = dir.to_string_lossy().into_owned();

    let mut rd = tokio::fs::read_dir(&dir)
        .await
        .map_err(|e| io_err(&format!("read_dir {}", display_path), e))?;

    let mut files: Vec<SftpFileAttr> = Vec::new();
    while let Some(entry) = rd
        .next_entry()
        .await
        .map_err(|e| io_err("next_entry", e))?
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        let full_path = dir.join(&name).to_string_lossy().into_owned();

        let ft = entry.file_type().await.map_err(|e| io_err("file_type", e))?;

        let (file_type, link_path, md) = if ft.is_symlink() {
            let target = tokio::fs::read_link(&entry.path())
                .await
                .ok()
                .map(|p| p.to_string_lossy().into_owned());
            // 跟随链接取真实大小/时间；断链时退化为链接自身的 dirent 元数据，
            // 目录列表不应因单个断链而整体失败
            let md = match tokio::fs::metadata(entry.path()).await {
                Ok(m) => m,
                Err(_) => entry.metadata().await.map_err(|e| io_err("metadata", e))?,
            };
            ("symlink", target, md)
        } else if ft.is_dir() {
            let md = entry.metadata().await.map_err(|e| io_err("metadata", e))?;
            ("dir", None, md)
        } else {
            let md = entry.metadata().await.map_err(|e| io_err("metadata", e))?;
            ("file", None, md)
        };

        let size_bytes = md.len();
        files.push(SftpFileAttr {
            file_name: name,
            file_type: file_type.to_string(),
            full_path,
            link_path,
            size: sftp::human_size(size_bytes),
            size_bytes,
            // 本地栏不展示属主（Unix 下取 uid->name 需要 passwd 解析，
            // Windows 下无此概念），前端本地栏隐藏这两列
            user: String::new(),
            group: String::new(),
            permissions: permission_string(&md),
            atime: mtime_secs(&md),
            mtime: mtime_secs(&md),
        });
    }

    // 目录优先、名称不区分大小写排序（与远程列表行为对齐）
    files.sort_by(|a, b| {
        let dir_first =
            (a.file_type == "dir").cmp(&(b.file_type == "dir")).reverse();
        dir_first.then_with(|| a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase()))
    });

    Ok(SftpListResp {
        sid: String::new(),
        path: display_path,
        files,
    })
}

/// 列出可浏览的"根"：Windows 为所有存在的盘符，Unix 为 "/"。
/// 供本地栏"此电脑"页展示，解决家目录起步看不到其他盘的问题。
/// 返回的 path 为空串，前端据此显示"此电脑"而非具体路径。
pub async fn roots() -> AppResult<SftpListResp> {
    let mut entries: Vec<SftpFileAttr> = Vec::new();

    #[cfg(windows)]
    {
        // A..Z 逐个探测：26 次 stat 是微秒级，避免为 GetLogicalDrivesStrings
        // 引入新的 windows-sys feature
        for letter in b'A'..=b'Z' {
            let root = format!("{}:\\", letter as char);
            if Path::new(&root).exists() {
                entries.push(SftpFileAttr {
                    file_name: format!("{}:", letter as char),
                    file_type: "dir".to_string(),
                    full_path: root,
                    link_path: None,
                    size: "-".to_string(),
                    size_bytes: 0,
                    user: String::new(),
                    group: String::new(),
                    permissions: String::new(),
                    atime: None,
                    mtime: None,
                });
            }
        }
    }

    #[cfg(not(windows))]
    {
        entries.push(SftpFileAttr {
            file_name: "/".to_string(),
            file_type: "dir".to_string(),
            full_path: "/".to_string(),
            link_path: None,
            size: "-".to_string(),
            size_bytes: 0,
            user: String::new(),
            group: String::new(),
            permissions: String::new(),
            atime: None,
            mtime: None,
        });
    }

    Ok(SftpListResp {
        sid: String::new(),
        path: String::new(),
        files: entries,
    })
}

/// 把远端文件流式写到本地目录（跳过前端 blob，GB 级文件不占内存）。
/// 目标文件名为远端 basename；local_dir 不存在时自动创建。
/// 已存在的同名本地文件会被覆盖（与"另存为"直接确认覆盖的行为一致）。
pub async fn download_to_local(
    sid: &str,
    remote_path: &str,
    local_dir: &str,
) -> AppResult<u64> {
    let dir = validate_absolute(local_dir)?;
    let basename = Path::new(remote_path)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| AppError::BadRequest("invalid remote filename".into()))?
        .to_string();

    let (_meta, mut stream) = sftp::open_for_read(sid, remote_path).await?;

    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| io_err("create_dir_all", e))?;
    let target = dir.join(&basename);
    let file = tokio::fs::File::create(&target)
        .await
        .map_err(|e| io_err(&format!("create {}", target.to_string_lossy()), e))?;
    let mut writer = tokio::io::BufWriter::new(file);

    let mut written: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| sftp::sftp_err("read", e))?;
        writer
            .write_all(&chunk)
            .await
            .map_err(|e| io_err("write", e))?;
        written += chunk.len() as u64;
    }
    writer.flush().await.map_err(|e| io_err("flush", e))?;

    Ok(written)
}

/// 把本地文件流式上传到远端（"上传 ->"按钮；webview 无法从路径构造 File
/// 对象，浏览器中转会把大文件读进内存，所以在 Rust 进程内直传）。
/// 远端已存在的同名文件会被覆盖（与 sftp upload 的行为一致）。
pub async fn upload_to_remote(
    sid: &str,
    local_path: &str,
    remote_path: &str,
) -> AppResult<u64> {
    let src = validate_absolute(local_path)?;
    let md = tokio::fs::metadata(&src)
        .await
        .map_err(|e| io_err(&format!("stat {}", src.to_string_lossy()), e))?;
    if !md.is_file() {
        return Err(AppError::BadRequest(format!(
            "not a regular file: {}",
            src.to_string_lossy()
        )));
    }

    let mut reader = tokio::fs::File::open(&src)
        .await
        .map_err(|e| io_err(&format!("open {}", src.to_string_lossy()), e))?;
    let mut writer = sftp::open_for_write(sid, remote_path).await?;

    // 256KiB 块直传：不经过 Vec 增长，也不把整个文件读入内存
    let mut buf = vec![0u8; 256 * 1024];
    let mut written: u64 = 0;
    loop {
        let n = reader
            .read(&mut buf)
            .await
            .map_err(|e| io_err("read local", e))?;
        if n == 0 {
            break;
        }
        writer
            .write_all(&buf[..n])
            .await
            .map_err(|e| sftp::sftp_err("write", e))?;
        written += n as u64;
    }
    writer
        .flush()
        .await
        .map_err(|e| sftp::sftp_err("flush", e))?;
    writer
        .shutdown()
        .await
        .map_err(|e| sftp::sftp_err("close", e))?;

    Ok(written)
}
