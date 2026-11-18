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
    if let Some(pp) = host.key_passphrase.as_deref() {
        if !pp.is_empty() {
            host.key_passphrase = Some(crypto::decrypt(key, pp)?);
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
    // 堡垒机标记仅对 SSH 协议有意义
    let bastion = protocol == "ssh" && input.bastion;
    // tmux 注入行会落在堡垒机菜单 TUI 上，bastion 会话强制关闭
    let tmux_enabled = protocol == "ssh" && input.tmux_enabled && !bastion;

    let password_enc = enc_opt(key, input.password.as_deref())?;
    let pk_enc = enc_opt(key, input.private_key.as_deref())?;
    let pp_enc = enc_opt(key, input.key_passphrase.as_deref())?;

    let res = sqlx::query(
        r#"INSERT INTO hosts (gid, name, icon, color, addr, port, username, password, desc, private_key, private_key_path, key_passphrase, protocol, baud_rate, data_bits, stop_bits, parity, flow_control, keepalive_interval, inactivity_timeout, idle_send_interval, jump_host_id, connect_command, tmux_enabled, tmux_session, bastion)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
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
    .bind(&pp_enc)
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
    .bind(input.connect_command.as_deref().filter(|s| !s.is_empty()))
    .bind(tmux_enabled)
    .bind(input.tmux_session.as_deref().filter(|s| !s.is_empty()))
    .bind(bastion)
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

/// 仅更新私钥 passphrase 字段（AES-256-GCM 加密落盘）。
/// 用于终端连接时解码带口令私钥、用户选择"记住 passphrase"的场景。
pub async fn update_key_passphrase_only(
    pool: &DbPool,
    key: &[u8; 32],
    id: i64,
    plain: &str,
) -> AppResult<()> {
    let enc = crypto::encrypt(key, plain)?;
    let res = sqlx::query(
        "UPDATE hosts SET key_passphrase = ?, updated_at = datetime('now') WHERE id = ? AND is_del = 0",
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
               h.password, h.desc, h.is_del, h.private_key, h.private_key_path, h.key_passphrase,
               h.protocol, h.baud_rate, h.data_bits, h.stop_bits, h.parity, h.flow_control,
               h.keepalive_interval, h.inactivity_timeout, h.idle_send_interval,
               h.jump_host_id, h.connect_command, h.tmux_enabled, h.tmux_session, h.bastion,
               h.created_at, h.updated_at,
               g.name AS group_name, g.parent_id AS parent_gid
        FROM hosts h
        LEFT JOIN groups g ON g.id = h.gid AND g.is_del = 0
        WHERE h.is_del = 0
    "#;
    // base 为字面量 SQL，拼接的只有过滤条件占位符，值全部走 bind 参数
    let rows = match gid {
        Some(g) => {
            sqlx::query_as::<_, crate::models::HostWithGroup>(sqlx::AssertSqlSafe(format!(
                "{base} AND h.gid = ? ORDER BY h.id ASC"
            )))
            .bind(g)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, crate::models::HostWithGroup>(sqlx::AssertSqlSafe(format!(
                "{base} ORDER BY h.id ASC"
            )))
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
    // 可空展示字段：前端编辑表单总是发送当前值（null = 清空），
    // 直接覆盖而非 or(cur) 保留，否则清空后保存会残留旧值
    let new_icon = input.icon;
    let new_color = input.color;
    let new_addr = input.addr.unwrap_or(cur.addr.clone());
    let new_port = input.port.unwrap_or(cur.port.clone());
    let new_username = input.username.unwrap_or(cur.username.clone());
    let new_desc = input.desc;

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
    let new_private_key_path = input.private_key_path;
    let new_key_passphrase = match input.key_passphrase {
        Some(p) if p.is_empty() => None,
        Some(p) => Some(crypto::encrypt(key, &p)?),
        None => cur.key_passphrase.clone(),
    };
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
    // 堡垒机标记仅对 SSH 协议有意义；前端编辑表单总是发送当前值
    let new_bastion = new_protocol == "ssh" && input.bastion.unwrap_or(cur.bastion);
    // tmux 集成开关；bastion 会话注入行落在菜单 TUI 上，强制关闭
    let new_tmux_enabled =
        new_protocol == "ssh" && input.tmux_enabled.unwrap_or(cur.tmux_enabled) && !new_bastion;
    // 双层 Option：None = 不修改；Some(None) = 清除；Some(Some(s)) = 设置
    let new_tmux_session = match input.tmux_session {
        Some(v) => v.filter(|s| !s.is_empty()),
        None => cur.tmux_session.clone(),
    };

    if let Some(path) = new_private_key_path.as_deref() {
        if !path.is_empty() {
            ensure_private_key_path(path)?;
        }
    }

    sqlx::query(
        r#"UPDATE hosts
           SET gid = ?, name = ?, icon = ?, color = ?, addr = ?, port = ?, username = ?,
               password = ?, desc = ?, private_key = ?, private_key_path = ?, key_passphrase = ?,
               protocol = ?, baud_rate = ?, data_bits = ?, stop_bits = ?, parity = ?, flow_control = ?,
               keepalive_interval = ?, inactivity_timeout = ?, idle_send_interval = ?,
               jump_host_id = ?, connect_command = ?, tmux_enabled = ?, tmux_session = ?, bastion = ?,
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
    .bind(&new_key_passphrase)
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
    // 双层 Option：None = 不修改；Some(None) = 清除；Some(Some(cmd)) = 设置
    .bind(match input.connect_command {
        Some(cmd) => cmd.filter(|s| !s.is_empty()),
        None => cur.connect_command.clone(),
    })
    .bind(new_tmux_enabled)
    .bind(new_tmux_session)
    .bind(new_bastion)
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

/// 批量删除结果：deleted = 实际删除数；skipped = 因被批量外主机作为跳板机引用而跳过的成员
#[derive(Debug, serde::Serialize)]
pub struct BatchDeleteSkipped {
    pub id: i64,
    pub name: String,
    /// 引用方主机名（前端据此组 i18n 文案）
    pub referrer: String,
}

#[derive(Debug, serde::Serialize)]
pub struct BatchDeleteResult {
    pub deleted: u64,
    pub skipped: Vec<BatchDeleteSkipped>,
}

/// 批量软删除。与单删的差异：引用方也在批量内时一并删除、互相不阻止；
/// 被批量外的主机引用的成员跳过并在结果中报告。
pub async fn delete_many(pool: &DbPool, ids: &[i64]) -> AppResult<BatchDeleteResult> {
    if ids.is_empty() {
        return Ok(BatchDeleteResult {
            deleted: 0,
            skipped: Vec::new(),
        });
    }

    // 目标集合（仅存在的未删除主机）
    let mut qb = sqlx::QueryBuilder::new("SELECT * FROM hosts WHERE is_del = 0 AND id IN (");
    let mut sep = qb.separated(", ");
    for id in ids {
        sep.push_bind(id);
    }
    qb.push(")");
    let targets: Vec<Host> = qb.build_query_as::<Host>().fetch_all(pool).await?;
    let batch: std::collections::HashSet<i64> = targets.iter().map(|h| h.id).collect();

    // 引用检查：jump_host_id 指向批量内成员的记录；引用方在批量内则一并删除
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT id, name, jump_host_id FROM hosts WHERE is_del = 0 AND jump_host_id IN (",
    );
    let mut sep = qb.separated(", ");
    for id in ids {
        sep.push_bind(id);
    }
    qb.push(")");
    let referrers = qb.build().fetch_all(pool).await?;
    use sqlx::Row;
    let mut skipped: std::collections::HashMap<i64, BatchDeleteSkipped> = std::collections::HashMap::new();
    for row in referrers {
        let referrer_name: String = row.try_get("name")?;
        let target_id: i64 = row.try_get("jump_host_id")?;
        if batch.contains(&target_id) {
            continue;
        }
        skipped
            .entry(target_id)
            .or_insert_with(|| BatchDeleteSkipped {
                id: target_id,
                name: targets
                    .iter()
                    .find(|h| h.id == target_id)
                    .map(|h| h.name.clone())
                    .unwrap_or_else(|| format!("#{}", target_id)),
                referrer: referrer_name,
            });
    }

    let mut deleted = 0u64;
    for h in &targets {
        if skipped.contains_key(&h.id) {
            continue;
        }
        let res = sqlx::query(
            "UPDATE hosts SET is_del = 1, updated_at = datetime('now') WHERE id = ? AND is_del = 0",
        )
        .bind(h.id)
        .execute(pool)
        .await?;
        deleted += res.rows_affected();
    }

    Ok(BatchDeleteResult {
        deleted,
        skipped: skipped.into_values().collect(),
    })
}

/// 批量移动到目标目录（gid=0 为根）。返回实际移动的台数。
pub async fn move_many(pool: &DbPool, ids: &[i64], gid: i64) -> AppResult<u64> {
    if ids.is_empty() {
        return Ok(0);
    }
    if gid != 0 {
        ensure_group_exists(pool, gid).await?;
    }
    let mut qb = sqlx::QueryBuilder::new("UPDATE hosts SET gid = ");
    qb.push_bind(gid);
    qb.push(", updated_at = datetime('now') WHERE is_del = 0 AND id IN (");
    let mut sep = qb.separated(", ");
    for id in ids {
        sep.push_bind(id);
    }
    qb.push(")");
    let res = qb.build().execute(pool).await?;
    Ok(res.rows_affected())
}

/// 恢复被跳过的成员及原因（前端据此组 i18n 文案）：
/// - "orphan"：所属目录已删除，恢复无处挂载
/// - "jump_target_missing"：跳板机目标仍处于删除态（与删除时的引用校验对称，
///   避免恢复出连不上的主机）
#[derive(Debug, serde::Serialize)]
pub struct RestoreSkipped {
    pub id: i64,
    pub name: String,
    pub reason: String,
}

#[derive(Debug, serde::Serialize)]
pub struct BatchRestoreResult {
    pub restored: u64,
    pub skipped: Vec<RestoreSkipped>,
}

/// 批量恢复软删主机（删除撤销窗口用）。软删只翻转 is_del、凭证列原样未动，
/// 恢复即原样带回（密钥同一安装内稳定，密文始终可解）。
/// 跳过两类：①孤儿——所属目录（gid≠0）已删除；②跳板机目标仍为删除态。
/// 不在软删集合里的 id（不存在/本来就没删）静默忽略。
pub async fn restore_many(pool: &DbPool, ids: &[i64]) -> AppResult<BatchRestoreResult> {
    // 去重保序：同一 id 传两遍只恢复一次
    let mut seen = std::collections::HashSet::new();
    let unique: Vec<i64> = ids
        .iter()
        .copied()
        .filter(|id| seen.insert(*id))
        .collect();
    if unique.is_empty() {
        return Ok(BatchRestoreResult {
            restored: 0,
            skipped: Vec::new(),
        });
    }

    // 候选：仅软删态主机（现有读函数全过滤 is_del=0，这里必须自写查询）
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT id, name, gid, jump_host_id FROM hosts WHERE is_del = 1 AND id IN (",
    );
    let mut sep = qb.separated(", ");
    for id in &unique {
        sep.push_bind(id);
    }
    qb.push(")");
    let rows = qb.build().fetch_all(pool).await?;
    use sqlx::Row;

    let mut skipped = Vec::new();
    let mut restore_ids = Vec::new();
    for row in rows {
        let id: i64 = row.try_get("id")?;
        let name: String = row.try_get("name")?;
        let gid: i64 = row.try_get("gid")?;
        let jump_host_id: Option<i64> = row.try_get("jump_host_id")?;

        // ①孤儿：父目录已删（gid=0 是根目录，合法且必存在）
        if gid != 0 {
            let alive: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM groups WHERE id = ? AND is_del = 0",
            )
            .bind(gid)
            .fetch_one(pool)
            .await?;
            if alive == 0 {
                skipped.push(RestoreSkipped {
                    id,
                    name,
                    reason: "orphan".to_string(),
                });
                continue;
            }
        }

        // ②跳板机目标仍处于删除态
        if let Some(jid) = jump_host_id {
            let alive: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM hosts WHERE id = ? AND is_del = 0",
            )
            .bind(jid)
            .fetch_one(pool)
            .await?;
            if alive == 0 {
                skipped.push(RestoreSkipped {
                    id,
                    name,
                    reason: "jump_target_missing".to_string(),
                });
                continue;
            }
        }

        restore_ids.push(id);
    }

    let mut restored = 0u64;
    if !restore_ids.is_empty() {
        let mut qb = sqlx::QueryBuilder::new(
            "UPDATE hosts SET is_del = 0, updated_at = datetime('now') WHERE is_del = 1 AND id IN (",
        );
        let mut sep = qb.separated(", ");
        for id in &restore_ids {
            sep.push_bind(id);
        }
        qb.push(")");
        restored = qb.build().execute(pool).await?.rows_affected();
    }

    Ok(BatchRestoreResult {
        restored,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;

    async fn test_pool() -> (tempfile::TempDir, std::path::PathBuf, DbPool) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = crate::models::db::init_pool(&db_path).await.unwrap();
        (dir, db_path, pool)
    }

    /// 最小化插入：仅必填列，其余走表默认值
    async fn insert_host(pool: &DbPool, gid: i64, name: &str, jump: Option<i64>) -> i64 {
        let res = sqlx::query(
            "INSERT INTO hosts (gid, name, addr, port, username, jump_host_id) \
             VALUES (?, ?, '10.0.0.1', '22', 'root', ?)",
        )
        .bind(gid)
        .bind(name)
        .bind(jump)
        .execute(pool)
        .await
        .unwrap();
        res.last_insert_rowid()
    }

    async fn insert_group(pool: &DbPool, name: &str) -> i64 {
        let res = sqlx::query("INSERT INTO groups (name) VALUES (?)")
            .bind(name)
            .execute(pool)
            .await
            .unwrap();
        res.last_insert_rowid()
    }

    async fn host_is_del(pool: &DbPool, id: i64) -> bool {
        let v: i64 = sqlx::query_scalar("SELECT is_del FROM hosts WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap();
        v == 1
    }

    #[tokio::test]
    async fn restore_flips_is_del_and_keeps_credentials() {
        let (_dir, _path, pool) = test_pool().await;
        let gid = insert_group(&pool, "prod").await;
        let id = insert_host(&pool, gid, "web-1", None).await;
        sqlx::query("UPDATE hosts SET password = 'ciphertext' WHERE id = ?")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();

        service_delete(&pool, id).await;
        assert!(host_is_del(&pool, id).await);

        let r = restore_many(&pool, &[id]).await.unwrap();
        assert_eq!(r.restored, 1);
        assert!(r.skipped.is_empty());
        assert!(!host_is_del(&pool, id).await);
        // 凭证列原样保留
        let pwd: Option<String> =
            sqlx::query_scalar("SELECT password FROM hosts WHERE id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(pwd.as_deref(), Some("ciphertext"));
    }

    #[tokio::test]
    async fn orphan_host_is_skipped() {
        let (_dir, _path, pool) = test_pool().await;
        let gid = insert_group(&pool, "gone").await;
        let id = insert_host(&pool, gid, "orphan-1", None).await;
        service_delete(&pool, id).await;
        // 父目录随后也被删
        sqlx::query("UPDATE groups SET is_del = 1 WHERE id = ?")
            .bind(gid)
            .execute(&pool)
            .await
            .unwrap();

        let r = restore_many(&pool, &[id]).await.unwrap();
        assert_eq!(r.restored, 0);
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.skipped[0].reason, "orphan");
        assert!(host_is_del(&pool, id).await);
    }

    #[tokio::test]
    async fn root_host_without_group_restores() {
        let (_dir, _path, pool) = test_pool().await;
        let id = insert_host(&pool, 0, "root-host", None).await;
        service_delete(&pool, id).await;
        let r = restore_many(&pool, &[id]).await.unwrap();
        assert_eq!(r.restored, 1);
        assert!(!host_is_del(&pool, id).await);
    }

    #[tokio::test]
    async fn jump_target_still_deleted_is_skipped() {
        let (_dir, _path, pool) = test_pool().await;
        let target = insert_host(&pool, 0, "jump-a", None).await;
        let user = insert_host(&pool, 0, "via-a", Some(target)).await;
        // 先删 A（引用方 C 在场会拦，先删引用方再删目标）
        service_delete(&pool, user).await;
        service_delete(&pool, target).await;

        // 只恢复引用方：跳板目标仍为删除态 → 跳过
        let r = restore_many(&pool, &[user]).await.unwrap();
        assert_eq!(r.restored, 0);
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.skipped[0].reason, "jump_target_missing");

        // 两者一起恢复：引用方跳过、目标正常恢复
        let r = restore_many(&pool, &[user, target]).await.unwrap();
        assert_eq!(r.restored, 1);
        assert_eq!(r.skipped.len(), 1);
        assert_eq!(r.skipped[0].id, user);
        assert!(!host_is_del(&pool, target).await);
    }

    #[tokio::test]
    async fn unknown_and_duplicate_ids_are_ignored() {
        let (_dir, _path, pool) = test_pool().await;
        let id = insert_host(&pool, 0, "dup-1", None).await;
        service_delete(&pool, id).await;

        let r = restore_many(&pool, &[9999, id, id]).await.unwrap();
        assert_eq!(r.restored, 1);
        assert!(r.skipped.is_empty());
    }

    /// 包装一层避免与模块内 delete 同名混淆（service::host::delete）
    async fn service_delete(pool: &DbPool, id: i64) {
        super::delete(pool, id).await.unwrap();
    }
}
