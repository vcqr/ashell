//! SFTP 业务服务：基于 sid 复用 SshSession，提供详细元数据列出、增删改名等

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use russh_sftp::client::fs::File;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, FileType, OpenFlags};
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::errors::{AppError, AppResult};
use crate::service::ssh as ssh_svc;

/// russh-sftp 的 Display 会把状态码描述和服务器消息都输出，
/// 当两者相同时产生 "Permission denied: Permission denied"，
/// 这里去掉重复的前半部分。
pub fn sftp_err(op: &str, e: impl std::fmt::Display) -> AppError {
    let raw = e.to_string();
    let cleaned = if let Some((a, b)) = raw.split_once(": ") {
        if a.eq_ignore_ascii_case(b) {
            b.to_string()
        } else {
            raw
        }
    } else {
        raw
    };
    AppError::Sftp(format!("{op}: {cleaned}"))
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SftpFileAttr {
    pub file_name: String,
    pub file_type: String,
    pub full_path: String,
    pub link_path: Option<String>,
    pub size: String,
    pub size_bytes: u64,
    pub user: String,
    pub group: String,
    pub permissions: String,
    pub atime: Option<u32>,
    pub mtime: Option<u32>,
    /// 隐藏/系统条目（Windows FILE_ATTRIBUTE_HIDDEN/SYSTEM，Unix 点开头）。
    /// 前端"显示隐藏文件"开关据此过滤；false 时省略序列化
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SftpListResp {
    pub sid: String,
    pub path: String,
    pub files: Vec<SftpFileAttr>,
}

fn convert_file_type(t: FileType) -> &'static str {
    match t {
        FileType::Dir => "dir",
        FileType::File => "file",
        FileType::Symlink => "symlink",
        FileType::Other => "other",
    }
}

pub(crate) fn human_size(bytes: u64) -> String {
    let units = ["B", "K", "M", "G", "T", "P", "E"];
    if bytes == 0 {
        return "0B".to_string();
    }
    let mut num = bytes as f64;
    let mut i = 0;
    while num >= 1024.0 && i < units.len() - 1 {
        num /= 1024.0;
        i += 1;
    }
    format!("{:.2} {}", num, units[i])
}

fn join_path(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
    .replace("//", "/")
}

/// 从 /etc/passwd 或 /etc/group 中读取 id -> name 映射
async fn read_id_map(sftp: &SftpSession, path: &str) -> HashMap<u32, String> {
    let mut map = HashMap::new();
    let file = match sftp.open_with_flags(path, OpenFlags::READ).await {
        Ok(f) => f,
        Err(_) => return map,
    };
    let mut lines = BufReader::new(file).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let v: Vec<&str> = line.split(':').collect();
        if v.len() > 2 {
            if let Ok(id) = v[2].parse::<u32>() {
                map.insert(id, v[0].to_string());
            }
        }
    }
    map
}

/// 列出目录详细信息
pub async fn list(sid: &str, path: Option<String>) -> AppResult<SftpListResp> {
    let sftp = ssh_svc::get_sftp(sid).await?;
    let mut dir_path = sftp
        .canonicalize(".")
        .await
        .map_err(|e| sftp_err("canonicalize", e))?;
    if let Some(p) = path {
        if !p.is_empty() {
            dir_path = p;
        }
    }

    let user_map = read_id_map(&sftp, "/etc/passwd").await;
    let group_map = read_id_map(&sftp, "/etc/group").await;

    let mut files = Vec::new();
    let entries = sftp
        .read_dir(&dir_path)
        .await
        .map_err(|e| sftp_err("read_dir", e))?;

    for entry in entries {
        let attrs: FileAttributes = entry.metadata();
        let ft = entry.file_type();
        let mut link_path = String::new();
        if ft.is_symlink() {
            let p = join_path(&dir_path, &entry.file_name());
            if let Ok(target) = sftp.read_link(&p).await {
                link_path = target;
            }
        }

        let uid = attrs.uid.unwrap_or(0);
        let gid = attrs.gid.unwrap_or(0);
        let user = user_map
            .get(&uid)
            .cloned()
            .unwrap_or_else(|| uid.to_string());
        let group = group_map
            .get(&gid)
            .cloned()
            .unwrap_or_else(|| gid.to_string());
        let size = attrs.size.unwrap_or(0);

        files.push(SftpFileAttr {
            file_name: entry.file_name(),
            file_type: convert_file_type(ft).to_string(),
            full_path: join_path(&dir_path, &entry.file_name()),
            link_path: if link_path.is_empty() {
                None
            } else {
                Some(link_path)
            },
            size: human_size(size),
            size_bytes: size,
            user,
            group,
            permissions: attrs.permissions().to_string(),
            atime: attrs.atime,
            mtime: attrs.mtime,
            hidden: entry.file_name().starts_with('.'),
        });
    }

    Ok(SftpListResp {
        sid: sid.to_string(),
        path: dir_path,
        files,
    })
}

/// 递归创建目录（先尝试 sftp，失败则尝试 mkdir -p 命令兜底）
pub async fn mkdir(sid: &str, path: &str) -> AppResult<()> {
    let sftp = ssh_svc::get_sftp(sid).await?;
    let mut current = PathBuf::new();
    for comp in Path::new(path).components() {
        current.push(comp);
        let dir = current.to_string_lossy().replace('\\', "/");
        match sftp.try_exists(&dir).await {
            Ok(true) => continue,
            Ok(false) => {
                sftp.create_dir(&dir)
                    .await
                    .map_err(|e| sftp_err(&format!("create_dir {dir}"), e))?;
            }
            Err(e) => return Err(sftp_err(&format!("exists {dir}"), e)),
        }
    }
    Ok(())
}

/// 创建空文件 (touch)
pub async fn touch(sid: &str, path: &str) -> AppResult<()> {
    let sftp = ssh_svc::get_sftp(sid).await?;
    let parent = Path::new(path)
        .parent()
        .ok_or_else(|| AppError::BadRequest("invalid path".into()))?;
    let parent_str = parent.to_string_lossy().replace('\\', "/");
    sftp.read_dir(&parent_str)
        .await
        .map_err(|e| sftp_err("parent not exist", e))?;
    sftp.create(path)
        .await
        .map_err(|e| sftp_err("create", e))?;
    Ok(())
}

/// 删除文件
pub async fn remove_file(sid: &str, path: &str) -> AppResult<()> {
    let sftp = ssh_svc::get_sftp(sid).await?;
    sftp.remove_file(path)
        .await
        .map_err(|e| sftp_err("remove_file", e))?;
    Ok(())
}

/// 删除目录（命令式 rm -rf 兜底，避免目录非空问题）
pub async fn remove_dir(sid: &str, path: &str) -> AppResult<()> {
    if path == "/" || path == "/*" || path == "./" || path == "./*" {
        return Err(AppError::BadRequest("不允许删除根路径".into()));
    }
    let client = ssh_svc::get_client(sid).await?;
    let cmd = format!("rm -rf '{}'", path.replace('\'', "'\\''"));
    client.execute(&cmd).await?;
    Ok(())
}

/// 重命名（要求同目录）
pub async fn rename(sid: &str, old_path: &str, new_path: &str) -> AppResult<()> {
    let old_parent = Path::new(old_path)
        .parent()
        .ok_or_else(|| AppError::BadRequest("invalid old_path".into()))?;
    let new_parent = Path::new(new_path)
        .parent()
        .ok_or_else(|| AppError::BadRequest("invalid new_path".into()))?;
    if old_parent != new_parent {
        return Err(AppError::BadRequest("rename 必须在同一目录下".into()));
    }
    let sftp = ssh_svc::get_sftp(sid).await?;
    sftp.rename(old_path, new_path)
        .await
        .map_err(|e| sftp_err("rename", e))?;
    Ok(())
}

/// 移动（跨目录 rename）。SFTP 协议原生支持同文件系统内瞬时移动；
/// 跨文件系统由服务器报错，前端引导用户改用复制+删除。
pub async fn move_path(sid: &str, old_path: &str, new_path: &str) -> AppResult<()> {
    if old_path == "/" || old_path.is_empty() {
        return Err(AppError::BadRequest("不允许移动根路径".into()));
    }
    let sftp = ssh_svc::get_sftp(sid).await?;
    sftp.rename(old_path, new_path)
        .await
        .map_err(|e| sftp_err("rename(move)", e))?;
    Ok(())
}

/// 远程内部复制：src 复制到 dst_dir 下（保持原名，文件同名覆盖、
/// 目录同名合并，整体递归）。SFTP 协议无服务器端 copy，走进程内
/// 流式中转（chunk 级读写，不落盘、不进 webview 内存）。
pub async fn duplicate(sid: &str, src_path: &str, dst_dir: &str) -> AppResult<()> {
    let sftp = ssh_svc::get_sftp(sid).await?;
    let name = Path::new(src_path)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| AppError::BadRequest("invalid src_path".into()))?
        .to_string();
    let dst_path = join_path(dst_dir, &name);
    duplicate_inner(&sftp, src_path, &dst_path).await
}

async fn duplicate_inner(sftp: &SftpSession, src: &str, dst: &str) -> AppResult<()> {
    let attrs = sftp
        .metadata(src)
        .await
        .map_err(|e| sftp_err("metadata", e))?;
    if attrs.is_dir() {
        // 目标目录已存在时按合并语义继续（Windows 资源管理器对齐）
        if !sftp
            .try_exists(dst)
            .await
            .map_err(|e| sftp_err("try_exists", e))?
        {
            sftp.create_dir(dst)
                .await
                .map_err(|e| sftp_err("create_dir", e))?;
        }
        let entries = sftp
            .read_dir(src)
            .await
            .map_err(|e| sftp_err("read_dir", e))?;
        for entry in entries {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let child_src = join_path(src, &name);
            let child_dst = join_path(dst, &name);
            Box::pin(duplicate_inner(sftp, &child_src, &child_dst)).await?;
        }
        return Ok(());
    }
    // 文件：读 chunk -> 写 chunk 流式中转
    let mut reader = sftp
        .open_with_flags(src, OpenFlags::READ)
        .await
        .map_err(|e| sftp_err("open", e))?;
    let mut writer = sftp
        .create(dst)
        .await
        .map_err(|e| sftp_err("create", e))?;
    tokio::io::copy(&mut reader, &mut writer)
        .await
        .map_err(|e| sftp_err("io::copy", e))?;
    writer
        .shutdown()
        .await
        .map_err(|e| sftp_err("shutdown", e))?;
    Ok(())
}

/// 打开远程文件用于流式下载
pub async fn open_for_read(
    sid: &str,
    filename: &str,
) -> AppResult<(FileAttributes, FramedRead<File, BytesCodec>)> {
    let sftp = ssh_svc::get_sftp(sid).await?;
    let file = sftp
        .open_with_flags(filename, OpenFlags::READ)
        .await
        .map_err(|e| sftp_err("open", e))?;
    let metadata = file
        .metadata()
        .await
        .map_err(|e| sftp_err("metadata", e))?;
    let stream = FramedRead::new(file, BytesCodec::new());
    Ok((metadata, stream))
}

/// 打开远程文件用于流式上传
pub async fn open_for_write(sid: &str, filename: &str) -> AppResult<File> {
    let sftp = ssh_svc::get_sftp(sid).await?;
    match sftp.create(filename).await {
        Ok(f) => Ok(f),
        Err(e) => {
            // 附带目标文件/父目录属性，便于区分 immutable、属主、目录不可写等根因
            let mut detail = String::new();
            if let Ok(m) = sftp.metadata(filename).await {
                detail.push_str(&format!(
                    " [target exists, perms={:o}, uid={}, gid={}]",
                    m.permissions.unwrap_or(0),
                    m.uid.unwrap_or(0),
                    m.gid.unwrap_or(0)
                ));
            } else {
                detail.push_str(" [target missing: parent dir may not allow create]");
            }
            let base = sftp_err("create", e).to_string();
            Err(AppError::Sftp(format!("{base} {detail}")))
        }
    }
}

/// 一次性读取整个文件到内存（小文件用）
#[allow(dead_code)]
pub async fn read_all(sid: &str, filename: &str) -> AppResult<Vec<u8>> {
    let sftp = ssh_svc::get_sftp(sid).await?;
    let mut file = sftp
        .open_with_flags(filename, OpenFlags::READ)
        .await
        .map_err(|e| sftp_err("open", e))?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .await
        .map_err(|e| sftp_err("read", e))?;
    Ok(buf)
}

/// 名称或纯数字字符串 -> 数字 ID；名称在 map（/etc/passwd、/etc/group 反查）中找不到时报错
fn resolve_id(name: &str, map: &HashMap<u32, String>, kind: &str) -> AppResult<u32> {
    if let Ok(id) = name.parse::<u32>() {
        return Ok(id);
    }
    map.iter()
        .find(|(_, n)| n.as_str() == name)
        .map(|(id, _)| *id)
        .ok_or_else(|| AppError::BadRequest(format!("无法解析{kind} \"{name}\" 的数字 ID")))
}

/// 修改远程文件属性（chmod / chown）。走 SFTP setstat：只携带调用方
/// 提供的字段（permissions / uid / gid），未提供的字段服务器不会改动。
/// mode 为 3-4 位八进制串；user/group 可传名称（经 /etc/passwd、/etc/group
/// 反查）或纯数字 ID。
pub async fn set_attrs(
    sid: &str,
    path: &str,
    mode: Option<String>,
    user: Option<String>,
    group: Option<String>,
) -> AppResult<()> {
    let sftp = ssh_svc::get_sftp(sid).await?;
    let mut attrs = FileAttributes::default();
    if let Some(m) = mode {
        let trimmed = m.trim();
        let parsed = u32::from_str_radix(trimmed, 8)
            .map_err(|_| AppError::BadRequest(format!("无效的八进制权限: {m}")))?;
        attrs.permissions = Some(parsed);
    }
    if user.is_some() || group.is_some() {
        if let Some(u) = user {
            let map = read_id_map(&sftp, "/etc/passwd").await;
            attrs.uid = Some(resolve_id(&u, &map, "用户")?);
        }
        if let Some(g) = group {
            let map = read_id_map(&sftp, "/etc/group").await;
            attrs.gid = Some(resolve_id(&g, &map, "组")?);
        }
    }
    if attrs.permissions.is_none() && attrs.uid.is_none() && attrs.gid.is_none() {
        return Err(AppError::BadRequest("没有需要修改的属性".into()));
    }
    sftp.set_metadata(path, attrs)
        .await
        .map_err(|e| sftp_err("set_attrs", e))?;
    Ok(())
}

/// 计算目录/文件占用大小（du -sk，POSIX/BusyBox 兼容），返回字节数。
/// 子目录部分不可读时 du 仍输出已统计部分（exit != 0），按 stdout 解析。
/// 提权会话下以 root 执行（execute_for_sftp 统一处理，下同）。
pub async fn du_size(sid: &str, path: &str, password: Option<&str>) -> AppResult<u64> {
    let cmd = format!("du -sk {} 2>/dev/null", sh_quote(path));
    let res = ssh_svc::execute_for_sftp(sid, &cmd, password).await?;
    let first = res.stdout.lines().next().unwrap_or("");
    let kb: u64 = first
        .split_whitespace()
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| AppError::Sftp(format!("du 解析失败: {}", res.stderr.trim())))?;
    Ok(kb * 1024)
}

/// 远端 shell 单引号转义（防路径中的特殊字符逃逸命令），ssh.rs 的提权
/// 包装也会用到
pub(crate) fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 压缩包识别：返回匹配到的（后缀, tar 解压压缩字母）；zip 用 "zip" 特殊标记。
/// tar 字母拼进 -x{字母}f；长后缀在前，避免 .tar 抢先匹配 .tar.gz。
fn detect_archive(name: &str) -> Option<(&'static str, &'static str)> {
    const RULES: &[(&str, &str)] = &[
        (".tar.gz", "z"),
        (".tgz", "z"),
        (".tar.bz2", "j"),
        (".tbz2", "j"),
        (".tbz", "j"),
        (".tar.xz", "J"),
        (".txz", "J"),
        (".tar", ""),
        (".zip", "zip"),
    ];
    let lower = name.to_lowercase();
    RULES.iter().find(|(s, _)| lower.ends_with(s)).copied()
}

/// 远端压缩：把 dir 下的 names 打包为 dir/archive_name（.tar.gz）。
/// 以 dir 为工作目录打包，压缩包内成员为相对路径；archive_name 由前端
/// 保证不重名（tar 会静默覆盖已有文件）。提权会话下以 root 执行。
pub async fn compress(
    sid: &str,
    dir: &str,
    names: &[String],
    archive_name: &str,
    password: Option<&str>,
) -> AppResult<String> {
    if names.is_empty() {
        return Err(AppError::BadRequest("没有可压缩的条目".into()));
    }
    let items = names.iter().map(|n| sh_quote(n)).collect::<Vec<_>>().join(" ");
    let cmd = format!(
        "cd {} && tar -czf {} -- {}",
        sh_quote(dir),
        sh_quote(archive_name),
        items
    );
    let res = ssh_svc::execute_for_sftp(sid, &cmd, password).await?;
    if res.exit_status != 0 {
        return Err(AppError::Sftp(format!("压缩失败: {}", res.stderr.trim())));
    }
    Ok(join_path(dir, archive_name))
}

/// 远端解压：支持 tar 家族（gz/bz2/xz/裸 tar）与 zip（unzip，缺则回退
/// python3 -m zipfile）。解到压缩包同目录下去扩展名命名的子目录；目录
/// 已存在时自动追加 -2、-3…，返回实际解压目录名。提权会话下以 root 执行。
pub async fn extract(sid: &str, path: &str, password: Option<&str>) -> AppResult<String> {
    let trimmed = path.trim_end_matches('/');
    let slash = trimmed
        .rfind('/')
        .ok_or_else(|| AppError::BadRequest("无效路径".into()))?;
    let (parent, name) = trimmed.split_at(slash);
    let parent = if parent.is_empty() { "/" } else { parent };
    let name = &name[1..];
    if name.is_empty() {
        return Err(AppError::BadRequest("无效路径".into()));
    }
    let Some((suffix, kind)) = detect_archive(name) else {
        return Err(AppError::BadRequest(
            "不支持的压缩格式（支持 tar/tar.gz/tgz/tar.bz2/tbz2/tar.xz/txz/zip）".into(),
        ));
    };
    let inner = &name[..name.len() - suffix.len()];

    // 目标目录去重循环放在远端执行，避免目录名竞争与来回确认
    let head = format!(
        "cd {p} && base={b} && dest=\"$base\" && n=2 && \
         while [ -e \"$dest\" ]; do dest=\"$base-$n\"; n=$((n+1)); done && mkdir \"$dest\"",
        p = sh_quote(parent),
        b = sh_quote(inner),
    );
    let body = if kind == "zip" {
        format!(
            "if command -v unzip >/dev/null 2>&1; then \
               unzip -q {a} -d \"$dest\"; \
             elif command -v python3 >/dev/null 2>&1; then \
               python3 -m zipfile -e {a} \"$dest\"; \
             else \
               echo '远端缺少 unzip 且无 python3，无法解压 zip' >&2; exit 23; \
             fi",
            a = sh_quote(name),
        )
    } else {
        format!("tar -x{kind}f {a} -C \"$dest\"", a = sh_quote(name),)
    };
    let script = format!("{head} && ({body}) || exit 22; printf '%s' \"$dest\"");

    let res = ssh_svc::execute_for_sftp(sid, &script, password).await?;
    if res.exit_status != 0 {
        let detail = res.stderr.trim();
        return Err(AppError::Sftp(if detail.is_empty() {
            format!("解压失败（exit {}）", res.exit_status)
        } else {
            format!("解压失败: {detail}")
        }));
    }
    let dest = res.stdout.trim();
    if dest.is_empty() {
        return Err(AppError::Sftp("解压结果异常：未返回目录名".into()));
    }
    Ok(dest.to_string())
}
