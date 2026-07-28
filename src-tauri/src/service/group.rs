use sqlx::Row;

use crate::errors::{AppError, AppResult};
use crate::models::{DbPool, Group, GroupCreate, GroupUpdate};

/// 根据 parent 计算新节点的 path 与 level
async fn compute_path_level(pool: &DbPool, parent_id: i64) -> AppResult<(String, i64)> {
    if parent_id == 0 {
        return Ok(("/".to_string(), 0));
    }
    let row = sqlx::query("SELECT path, level FROM groups WHERE id = ? AND is_del = 0")
        .bind(parent_id)
        .fetch_optional(pool)
        .await?;
    match row {
        Some(r) => {
            let parent_path: String = r.try_get("path")?;
            let parent_level: i64 = r.try_get("level")?;
            // 例如父 path "/", parent_id=3 → "/3/"； 父 path "/3/", parent_id=5 → "/3/5/"
            let new_path = format!("{}{}/", parent_path, parent_id);
            Ok((new_path, parent_level + 1))
        }
        None => Err(AppError::NotFound(format!("parent group {parent_id}"))),
    }
}

pub async fn create(pool: &DbPool, input: GroupCreate) -> AppResult<Group> {
    if input.name.trim().is_empty() {
        return Err(AppError::BadRequest("name 不能为空".into()));
    }
    let (path, level) = compute_path_level(pool, input.parent_id).await?;

    let res = sqlx::query(
        "INSERT INTO groups (parent_id, name, path, level) VALUES (?, ?, ?, ?)",
    )
    .bind(input.parent_id)
    .bind(&input.name)
    .bind(&path)
    .bind(level)
    .execute(pool)
    .await?;

    let id = res.last_insert_rowid();
    get_by_id(pool, id).await
}

pub async fn get_by_id(pool: &DbPool, id: i64) -> AppResult<Group> {
    let g = sqlx::query_as::<_, Group>("SELECT * FROM groups WHERE id = ? AND is_del = 0")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("group {id}")))?;
    Ok(g)
}

/// 列出所有未删除的目录组（前端可自行构造树）
pub async fn list_all(pool: &DbPool) -> AppResult<Vec<Group>> {
    let rows = sqlx::query_as::<_, Group>(
        "SELECT * FROM groups WHERE is_del = 0 ORDER BY level ASC, id ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 列出某节点的直接子节点
pub async fn list_children(pool: &DbPool, parent_id: i64) -> AppResult<Vec<Group>> {
    let rows = sqlx::query_as::<_, Group>(
        "SELECT * FROM groups WHERE parent_id = ? AND is_del = 0 ORDER BY id ASC",
    )
    .bind(parent_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn update(pool: &DbPool, id: i64, input: GroupUpdate) -> AppResult<Group> {
    let cur = get_by_id(pool, id).await?;

    // 更新名字
    if let Some(name) = input.name.as_ref() {
        if name.trim().is_empty() {
            return Err(AppError::BadRequest("name 不能为空".into()));
        }
        sqlx::query(
            "UPDATE groups SET name = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(name)
        .bind(id)
        .execute(pool)
        .await?;
    }

    // 移动节点（修改 parent_id 时需重算 path/level，并级联更新所有后代）
    if let Some(new_parent) = input.parent_id {
        if new_parent == id {
            return Err(AppError::BadRequest("不能将节点移动到自身".into()));
        }
        // 防环：新父节点不能是自身的后代
        let descendant_prefix = format!("{}{}/", cur.path, cur.id);
        if new_parent != 0 {
            let parent = get_by_id(pool, new_parent).await?;
            if parent.path.starts_with(&descendant_prefix) || parent.id == cur.id {
                return Err(AppError::BadRequest("不能将节点移动到自身的子树".into()));
            }
        }

        let (new_path, new_level) = compute_path_level(pool, new_parent).await?;

        // 更新自身
        sqlx::query(
            "UPDATE groups SET parent_id = ?, path = ?, level = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(new_parent)
        .bind(&new_path)
        .bind(new_level)
        .bind(id)
        .execute(pool)
        .await?;

        // 级联更新后代：把 path 前缀从 old_descendant_prefix 替换为 new_descendant_prefix
        let old_desc = descendant_prefix; // 形如 "/p1/cur_id/"
        let new_desc = format!("{}{}/", new_path, id);
        // SQLite: 用 substr + replace。后代行：path LIKE old_desc%
        let like_pat = format!("{}%", old_desc);
        let level_delta = new_level - cur.level; // 自身 level 变化量同样作用于后代
        sqlx::query(
            "UPDATE groups SET path = ? || substr(path, ?), level = level + ?, updated_at = datetime('now') WHERE path LIKE ? AND is_del = 0",
        )
        .bind(&new_desc)
        .bind((old_desc.len() + 1) as i64) // substr 1-based
        .bind(level_delta)
        .bind(&like_pat)
        .execute(pool)
        .await?;
    }

    get_by_id(pool, id).await
}

/// 软删除：删除该节点及所有后代节点；同时软删除其下所有 hosts
pub async fn delete(pool: &DbPool, id: i64) -> AppResult<()> {
    let cur = get_by_id(pool, id).await?;
    let subtree_prefix = format!("{}{}/", cur.path, cur.id);
    let like_pat = format!("{}%", subtree_prefix);

    let mut tx = pool.begin().await?;

    // 收集要删除的所有 group ids（自身 + 后代）
    let descendants: Vec<i64> = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM groups WHERE path LIKE ? AND is_del = 0",
    )
    .bind(&like_pat)
    .fetch_all(&mut *tx)
    .await?;

    let mut all_ids = vec![cur.id];
    all_ids.extend(descendants);

    // 软删除 groups
    sqlx::query("UPDATE groups SET is_del = 1, updated_at = datetime('now') WHERE id = ?")
        .bind(cur.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE groups SET is_del = 1, updated_at = datetime('now') WHERE path LIKE ? AND is_del = 0",
    )
    .bind(&like_pat)
    .execute(&mut *tx)
    .await?;

    // 软删除 hosts (gid IN all_ids)
    if !all_ids.is_empty() {
        // 构造 IN 占位符
        let placeholders = all_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "UPDATE hosts SET is_del = 1, updated_at = datetime('now') WHERE is_del = 0 AND gid IN ({})",
            placeholders
        );
        let mut q = sqlx::query(&sql);
        for id in &all_ids {
            q = q.bind(id);
        }
        q.execute(&mut *tx).await?;
    }

    tx.commit().await?;
    Ok(())
}
