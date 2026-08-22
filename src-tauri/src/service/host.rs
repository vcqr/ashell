use crate::config::crypto;
use crate::errors::{AppError, AppResult};
use crate::models::{DbPool, Host, HostCreate, HostUpdate};

/// 加密非空凭证；输入 None / 空串则保持 None
fn enc_opt(key: &[u8; 32], v: Option<&str>) -> AppResult<Option<String>> {
    match v {
        Some(s) if !s.is_empty() => Ok(Some(crypto::encrypt(key, s)?)),
        _ => Ok(None),
    }
}

/// 解密凭证字段（数据库读出后）
pub fn decrypt_credentials(key: &[u8; 32], host: &mut Host) -> AppResult<()> {
    if let Some(p) = host.password.as_deref() {
        if !p.is_empty() {
            host.password = Some(crypto::decrypt(key, p)?);
        }
    }
    if let Some(pk) = host.private_key.as_deref() {
        if !pk.is_empty() {
            host.private_key = Some(crypto::decrypt(key, pk)?);
        }
    }
    Ok(())
}

pub async fn create(pool: &DbPool, key: &[u8; 32], input: HostCreate) -> AppResult<Host> {
    let protocol = input.protocol.as_deref().unwrap_or("ssh");

    if input.name.trim().is_empty() || input.addr.trim().is_empty() {
        return Err(AppError::BadRequest("name/addr 不能为空".into()));
    }
    // SSH/Telnet 需要 username；Serial 不需要
    if protocol != "serial" && input.username.trim().is_empty() {
        return Err(AppError::BadRequest("username 不能为空".into()));
    }

    if input.gid != 0 {
        ensure_group_exists(pool, input.gid).await?;
    }

    if let Some(path) = input.private_key_path.as_deref() {
        if !path.is_empty() {
            ensure_private_key_path(path)?;
        }
    }

    // 跳板机仅对 SSH 协议有意义
    let jump_host_id = if protocol == "ssh" {
        input.jump_host_id
    } else {
        None
    };
    if let Some(jump_id) = jump_host_id {
        validate_jump_host(pool, jump_id, None).await?;
    }

    let password_enc = enc_opt(key, input.password.as_deref())?;
    let pk_enc = enc_opt(key, input.private_key.as_deref())?;

    let res = sqlx::query(
        r#"INSERT INTO hosts (gid, name, icon, color, addr, port, username, password, desc, private_key, private_key_path, protocol, baud_rate, data_bits, stop_bits, parity, flow_control, keepalive_interval, inactivity_timeout, idle_send_interval, jump_host_id)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(input.gid)
    .bind(&input.name)
    .bind(&input.icon)
    .bind(&input.color)
    .bind(&input.addr)
    .bind(&input.port)
    .bind(&input.username)
    .bind(&password_enc)
    .bind(&input.desc)
    .bind(&pk_enc)
    .bind(&input.private_key_path)
    .bind(protocol)
    .bind(input.baud_rate)
    .bind(input.data_bits)
    .bind(input.stop_bits)
    .bind(&input.parity)
    .bind(&input.flow_control)
    .bind(input.keepalive_interval)
    .bind(input.inactivity_timeout)
    .bind(input.idle_send_interval)
    .bind(jump_host_id)
    .execute(pool)
    .await?;

    get_by_id(pool, res.last_insert_rowid()).await
}

/// 校验跳板机引用：必须存在、必须是 SSH 主机、不允许多级跳、不能指向自己
async fn validate_jump_host(pool: &DbPool, jump_id: i64, self_id: Option<i64>) -> AppResult<()> {
    if Some(jump_id) == self_id {
        return Err(AppError::BadRequest("跳板机不能是主机自身".into()));
    }
    let jump = get_by_id(pool, jump_id).await.map_err(|_| {
        AppError::BadRequest(format!("跳板机主机不存在: {jump_id}"))
    })?;
    if jump.protocol != "ssh" {
        return Err(AppError::BadRequest("跳板机必须是 SSH 主机".into()));
    }
    if jump.jump_host_id.is_some() {
        return Err(AppError::BadRequest(
            "暂不支持多级跳板：所选跳板机自身也配置了跳板机".into(),
        ));
    }
    Ok(())
}

async fn ensure_group_exists(pool: &DbPool, gid: i64) -> AppResult<()> {
    let exists: Option<i64> =
        sqlx::query_scalar("SELECT id FROM groups WHERE id = ? AND is_del = 0")
            .bind(gid)
            .fetch_optional(pool)
            .await?;
    if exists.is_none() {
        return Err(AppError::NotFound(format!("group {gid}")));
    }
    Ok(())
}

/// 预检私钥文件路径是否存在且可读
fn ensure_private_key_path(path: &str) -> AppResult<()> {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return Err(AppError::BadRequest(format!(
            "私钥文件不存在: {path}"
        )));
    }
    std::fs::read_to_string(p).map_err(|e| {
        AppError::BadRequest(format!("私钥文件无法读取: {path}: {e}"))
    })?;
    Ok(())
}

pub async fn get_by_id(pool: &DbPool, id: i64) -> AppResult<Host> {
    let h = sqlx::query_as::<_, Host>("SELECT * FROM hosts WHERE id = ? AND is_del = 0")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("host {id}")))?;
    Ok(h)
}

/// 获取并解密凭证（仅供 SSH/SFTP 内部使用，不要直接返回前端）
pub async fn get_with_credentials(pool: &DbPool, key: &[u8; 32], id: i64) -> AppResult<Host> {
    let mut h = get_by_id(pool, id).await?;
    decrypt_credentials(key, &mut h)?;
    Ok(h)
}

/// 仅更新密码字段（AES-256-GCM 加密落盘）。
/// 用于终端连接时发现密码已变更、用户选择"记住新密码"的场景。
pub async fn update_password_only(
    pool: &DbPool,
    key: &[u8; 32],
    id: i64,
    plain: &str,
) -> AppResult<()> {
    let enc = crypto::encrypt(key, plain)?;
    let res = sqlx::query(
        "UPDATE hosts SET password = ?, updated_at = datetime('now') WHERE id = ? AND is_del = 0",
    )
    .bind(enc)
    .bind(id)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("host {id}")));
    }
    Ok(())
}

pub async fn list(pool: &DbPool, gid: Option<i64>) -> AppResult<Vec<Host>> {
    let rows = match gid {
        Some(g) => {
            sqlx::query_as::<_, Host>(
                "SELECT * FROM hosts WHERE gid = ? AND is_del = 0 ORDER BY id ASC",
            )
            .bind(g)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, Host>("SELECT * FROM hosts WHERE is_del = 0 ORDER BY id ASC")
                .fetch_all(pool)
                .await?
        }
    };
    Ok(rows)
}

/// 联表查询：返回 host + group_name + parent_gid，前端树形展示直接复用
pub async fn list_with_group(
    pool: &DbPool,
    gid: Option<i64>,
) -> AppResult<Vec<crate::models::HostWithGroup>> {
    let base = r#"
        SELECT h.id, h.gid, h.name, h.icon, h.color, h.addr, h.port, h.username,
               h.password, h.desc, h.is_del, h.private_key, h.private_key_path,
               h.protocol, h.baud_rate, h.data_bits, h.stop_bits, h.parity, h.flow_control,
               h.keepalive_interval, h.inactivity_timeout, h.idle_send_interval,
               h.jump_host_id,
               h.created_at, h.updated_at,
               g.name AS group_name, g.parent_id AS parent_gid
        FROM hosts h
        LEFT JOIN groups g ON g.id = h.gid AND g.is_del = 0
        WHERE h.is_del = 0
    "#;
    let rows = match gid {
        Some(g) => {
            sqlx::query_as::<_, crate::models::HostWithGroup>(&format!(
                "{base} AND h.gid = ? ORDER BY h.id ASC"
            ))
            .bind(g)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, crate::models::HostWithGroup>(&format!(
                "{base} ORDER BY h.id ASC"
            ))
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows)
}

pub async fn update(
    pool: &DbPool,
    key: &[u8; 32],
    id: i64,
    input: HostUpdate,
) -> AppResult<Host> {
    let cur = get_by_id(pool, id).await?;

    // 通过 COALESCE 简化部分更新；凭证字段单独处理（仅当传入了字符串才更新）
    let new_gid = input.gid.unwrap_or(cur.gid);
    if new_gid != cur.gid && new_gid != 0 {
        ensure_group_exists(pool, new_gid).await?;
    }

    let new_name = input.name.unwrap_or(cur.name.clone());
    let new_icon = input.icon.or(cur.icon.clone());
    let new_color = input.color.or(cur.color.clone());
    let new_addr = input.addr.unwrap_or(cur.addr.clone());
    let new_port = input.port.unwrap_or(cur.port.clone());
    let new_username = input.username.unwrap_or(cur.username.clone());
    let new_desc = input.desc.or(cur.desc.clone());

    let new_password = match input.password {
        Some(p) if p.is_empty() => None,
        Some(p) => Some(crypto::encrypt(key, &p)?),
        None => cur.password.clone(),
    };
    let new_private_key = match input.private_key {
        Some(p) if p.is_empty() => None,
        Some(p) => Some(crypto::encrypt(key, &p)?),
        None => cur.private_key.clone(),
    };
    let new_private_key_path = input.private_key_path.or(cur.private_key_path.clone());
    let new_protocol = input.protocol.unwrap_or(cur.protocol.clone());
    let new_baud_rate = input.baud_rate.or(cur.baud_rate);
    let new_data_bits = input.data_bits.or(cur.data_bits);
    let new_stop_bits = input.stop_bits.or(cur.stop_bits);
    let new_parity = input.parity.or(cur.parity.clone());
    let new_flow_control = input.flow_control.or(cur.flow_control.clone());
    // 三个保活/超时字段：前端表单 edit 总是发送当前值（null = 清空），
    // 直接覆盖而非 or(cur) 保留，否则删除字段后保存会残留旧值
    let new_keepalive_interval = input.keepalive_interval;
    let new_inactivity_timeout = input.inactivity_timeout;
    let new_idle_send_interval = input.idle_send_interval;

    // 双层 Option：None = 不修改；Some(None) = 清除；Some(Some(id)) = 设置
    let new_jump_host_id = match input.jump_host_id {
        Some(v) => v,
        None => cur.jump_host_id,
    };
    if new_protocol == "ssh" {
        if let Some(jump_id) = new_jump_host_id {
            validate_jump_host(pool, jump_id, Some(id)).await?;
        }
    }
    // 非 SSH 协议强制清除跳板机配置
    let new_jump_host_id = if new_protocol == "ssh" {
        new_jump_host_id
    } else {
        None
    };

    if let Some(path) = new_private_key_path.as_deref() {
        if !path.is_empty() {
            ensure_private_key_path(path)?;
        }
    }

    sqlx::query(
        r#"UPDATE hosts
           SET gid = ?, name = ?, icon = ?, color = ?, addr = ?, port = ?, username = ?,
               password = ?, desc = ?, private_key = ?, private_key_path = ?,
               protocol = ?, baud_rate = ?, data_bits = ?, stop_bits = ?, parity = ?, flow_control = ?,
               keepalive_interval = ?, inactivity_timeout = ?, idle_send_interval = ?,
               jump_host_id = ?,
               updated_at = datetime('now')
           WHERE id = ?"#,
    )
    .bind(new_gid)
    .bind(&new_name)
    .bind(&new_icon)
    .bind(&new_color)
    .bind(&new_addr)
    .bind(&new_port)
    .bind(&new_username)
    .bind(&new_password)
    .bind(&new_desc)
    .bind(&new_private_key)
    .bind(&new_private_key_path)
    .bind(&new_protocol)
    .bind(new_baud_rate)
    .bind(new_data_bits)
    .bind(new_stop_bits)
    .bind(&new_parity)
    .bind(&new_flow_control)
    .bind(new_keepalive_interval)
    .bind(new_inactivity_timeout)
    .bind(new_idle_send_interval)
    .bind(new_jump_host_id)
    .bind(id)
    .execute(pool)
    .await?;

    get_by_id(pool, id).await
}

pub async fn delete(pool: &DbPool, id: i64) -> AppResult<()> {
    // 被其他主机作为跳板机引用时禁止删除，避免引用方连接时才报错
    let refs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM hosts WHERE jump_host_id = ? AND is_del = 0")
            .bind(id)
            .fetch_one(pool)
            .await?;
    if refs > 0 {
        return Err(AppError::BadRequest(format!(
            "该主机被 {refs} 台主机作为跳板机引用，请先解除引用再删除"
        )));
    }

    let res = sqlx::query(
        "UPDATE hosts SET is_del = 1, updated_at = datetime('now') WHERE id = ? AND is_del = 0",
    )
    .bind(id)
    .execute(pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("host {id}")));
    }
    Ok(())
}
