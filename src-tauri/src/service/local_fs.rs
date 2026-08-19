//! 本地文件系统访问（SFTP 双栏的本地侧）。
//!
//! 提供"列目录"与两个直传能力："远端 -> 本地落盘"、"本地文件 -> 远端"，
//! 传输在 Rust 进程内流式完成，大文件不经过 webview 内存。
//! 另提供"OS 拖放文件落盘"：双栏下把文件拖进本地栏时，webview 只能拿到
//! File 对象，字节流经本地 HTTP（multipart）回传写入本地目录--这是唯一
//! 必须过 webview 的传输路径。
//! 文件管理操作（回收站/永久删除、新建、重命名、复制、移动、在文件
//! 管理器中显示、默认程序打开）：回收站走系统语义可恢复，永久删除
//! 由前端强警示确认后才调用。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::errors::{AppError, AppResult};
use crate::service::sftp::{self, SftpFileAttr, SftpListResp};

fn io_err(context: &str, e: std::io::Error) -> AppError {
    AppError::BadRequest(format!("{context}: {e}"))
}

/* ---------- 直传任务进度 ----------
 * Rust 进程内直传（本地<->远端）不经过 webview，前端拿不到字节流，
 * 进度改由这里按 task_id 记账，前端轮询 /api/local/fs/progress 取回。
 * 计数器随 256KiB 块更新，Mutex 只是极短的整数读写，无争用压力。 */

fn progress_map() -> &'static Mutex<HashMap<String, u64>> {
    static MAP: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn progress_set(id: &str, bytes: u64) {
    progress_map().lock().unwrap().insert(id.to_string(), bytes);
}

pub fn progress_get(id: &str) -> Option<u64> {
    progress_map().lock().unwrap().get(id).copied()
}

pub fn progress_remove(id: &str) {
    progress_map().lock().unwrap().remove(id);
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
/// `task_id` 提供时在进度表中记账，供前端轮询。
pub async fn download_to_local(
    sid: &str,
    remote_path: &str,
    local_dir: &str,
    task_id: Option<&str>,
) -> AppResult<u64> {
    let dir = validate_absolute(local_dir)?;
    let basename = Path::new(remote_path)
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| AppError::BadRequest("invalid remote filename".into()))?
        .to_string();

    if let Some(id) = task_id {
        progress_set(id, 0);
    }
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
        if let Some(id) = task_id {
            progress_set(id, written);
        }
    }
    writer.flush().await.map_err(|e| io_err("flush", e))?;

    Ok(written)
}

/// 把本地文件流式上传到远端（"上传 ->"按钮；webview 无法从路径构造 File
/// 对象，浏览器中转会把大文件读进内存，所以在 Rust 进程内直传）。
/// 远端已存在的同名文件会被覆盖（与 sftp upload 的行为一致）。
/// `task_id` 提供时在进度表中记账，供前端轮询。
pub async fn upload_to_remote(
    sid: &str,
    local_path: &str,
    remote_path: &str,
    task_id: Option<&str>,
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

    if let Some(id) = task_id {
        progress_set(id, 0);
    }

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
        if let Some(id) = task_id {
            progress_set(id, written);
        }
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

/// 解析"OS 拖放文件落盘"的目标路径，并按需创建父目录。
///
/// `rel_name` 为前端给的相对路径（可含子目录，如 `sub/a.txt`）；
/// 统一把 `\` 归一为 `/` 后逐段校验，拒绝空段、`.`、`..` 与绝对路径，
/// 防止拖放数据逃逸出目标目录。同名文件由调用方直接覆盖（File::create 语义）。
pub async fn prepare_save_path(local_dir: &str, rel_name: &str) -> AppResult<PathBuf> {
    let mut target = validate_absolute(local_dir)?;
    let rel = rel_name.replace('\\', "/");
    let rel = rel.trim_matches('/');
    if rel.is_empty() {
        return Err(AppError::BadRequest("empty save name".into()));
    }
    for seg in rel.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return Err(AppError::BadRequest(format!(
                "invalid segment in save name: {rel_name}"
            )));
        }
        target.push(seg);
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| io_err("create_dir_all", e))?;
    }
    Ok(target)
}

/// 移入系统回收站（macOS Finder 废纸篓 / Windows 回收站 / Linux XDG trash），
/// 可从系统回收站恢复。trash::delete 是同步系统调用，包在 spawn_blocking
/// 里避免阻塞 tokio worker。
pub async fn trash(path: &str) -> AppResult<()> {
    let p = validate_absolute(path)?;
    let display = p.to_string_lossy().into_owned();
    tokio::task::spawn_blocking(move || {
        trash::delete(&p).map_err(|e| AppError::BadRequest(format!("trash {display}: {e}")))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join trash task: {e}")))?
}

/// 删除本地文件 / 目录（目录递归）。不经回收站，调用方负责确认。
/// symlink_metadata 不跟随符号链接：链接本身按文件删除，不会误删目标内容。
pub async fn remove(path: &str) -> AppResult<()> {
    let p = validate_absolute(path)?;
    let display = p.to_string_lossy().into_owned();
    let md = tokio::fs::symlink_metadata(&p)
        .await
        .map_err(|e| io_err(&format!("stat {}", display), e))?;
    if md.is_dir() {
        tokio::fs::remove_dir_all(&p)
            .await
            .map_err(|e| io_err(&format!("remove_dir_all {}", display), e))?;
    } else {
        tokio::fs::remove_file(&p)
            .await
            .map_err(|e| io_err(&format!("remove_file {}", display), e))?;
    }
    Ok(())
}

/// 新建本地目录（已存在时报错）
pub async fn mkdir(path: &str) -> AppResult<()> {
    let p = validate_absolute(path)?;
    tokio::fs::create_dir(&p)
        .await
        .map_err(|e| io_err(&format!("create_dir {}", p.to_string_lossy()), e))
}

/// 新建本地空文件（已存在时报错，避免截断已有内容）
pub async fn create_file(path: &str) -> AppResult<()> {
    let p = validate_absolute(path)?;
    if tokio::fs::try_exists(&p).await.unwrap_or(false) {
        return Err(AppError::BadRequest(format!(
            "文件已存在：{}",
            p.to_string_lossy()
        )));
    }
    tokio::fs::File::create(&p)
        .await
        .map_err(|e| io_err(&format!("create {}", p.to_string_lossy()), e))?;
    Ok(())
}

/// 重命名 / 同盘移动（tokio rename；跨盘移动请用 move_to）
pub async fn rename(from: &str, to: &str) -> AppResult<()> {
    let src = validate_absolute(from)?;
    let dst = validate_absolute(to)?;
    tokio::fs::rename(&src, &dst).await.map_err(|e| {
        io_err(
            &format!(
                "rename {} -> {}",
                src.to_string_lossy(),
                dst.to_string_lossy()
            ),
            e,
        )
    })
}

/// 复制文件/目录到 dst_dir 下（保持原名；文件同名覆盖、目录同名合并，递归）。
/// 类型判定用 metadata（跟随符号链接），与远程 duplicate 的 stat 语义一致。
pub async fn copy(src_path: &str, dst_dir: &str) -> AppResult<()> {
    let src = validate_absolute(src_path)?;
    let dir = validate_absolute(dst_dir)?;
    let name = src
        .file_name()
        .ok_or_else(|| AppError::BadRequest("invalid src_path".into()))?;
    let dst = dir.join(name);
    copy_inner(&src, &dst).await
}

async fn copy_inner(src: &Path, dst: &Path) -> AppResult<()> {
    let display = src.to_string_lossy().into_owned();
    let md = tokio::fs::metadata(src)
        .await
        .map_err(|e| io_err(&format!("stat {}", display), e))?;
    if md.is_dir() {
        tokio::fs::create_dir_all(dst)
            .await
            .map_err(|e| io_err(&format!("create_dir_all {}", dst.to_string_lossy()), e))?;
        let mut rd = tokio::fs::read_dir(src)
            .await
            .map_err(|e| io_err(&format!("read_dir {}", display), e))?;
        while let Some(entry) = rd
            .next_entry()
            .await
            .map_err(|e| io_err("next_entry", e))?
        {
            Box::pin(copy_inner(&entry.path(), &dst.join(entry.file_name()))).await?;
        }
        return Ok(());
    }
    tokio::fs::copy(src, dst)
        .await
        .map_err(|e| io_err(&format!("copy {}", display), e))?;
    Ok(())
}

/// 移动到 dst_dir 下（保持原名）。同盘 = rename 瞬时完成；
/// 跨盘（EXDEV）自动回退为递归复制 + 删除源。
pub async fn move_to(src_path: &str, dst_dir: &str) -> AppResult<()> {
    let src = validate_absolute(src_path)?;
    let dir = validate_absolute(dst_dir)?;
    let name = src
        .file_name()
        .ok_or_else(|| AppError::BadRequest("invalid src_path".into()))?;
    let dst = dir.join(name);
    let display = src.to_string_lossy().into_owned();
    match tokio::fs::rename(&src, &dst).await {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            copy_inner(&src, &dst).await?;
            remove(src.to_str().unwrap_or_default()).await
        }
        Err(e) => Err(io_err(&format!("rename {}", display), e)),
    }
}

/// 在系统文件管理器中定位显示（Finder / 资源管理器）。
/// opener 会等待子进程启动，包在 spawn_blocking 里避免阻塞 worker。
pub async fn reveal(path: &str) -> AppResult<()> {
    let p = validate_absolute(path)?;
    tokio::task::spawn_blocking(move || {
        tauri_plugin_opener::reveal_item_in_dir(&p)
            .map_err(|e| AppError::Internal(format!("reveal: {e}")))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join reveal task: {e}")))?
}

/// 用系统默认程序打开本地文件/目录
pub async fn open(path: &str) -> AppResult<()> {
    let p = validate_absolute(path)?;
    tokio::task::spawn_blocking(move || {
        tauri_plugin_opener::open_path(&p, None::<&str>)
            .map_err(|e| AppError::Internal(format!("open: {e}")))
    })
    .await
    .map_err(|e| AppError::Internal(format!("join open task: {e}")))?
}
