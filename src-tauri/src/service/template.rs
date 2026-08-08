use crate::errors::{AppError, AppResult};
use crate::models::{CommandTemplate, CommandTemplateCreate, CommandTemplateUpdate, DbPool};

/// 列出全部模板命令，按 sort_order -> created_at 排序
pub async fn list(db: &DbPool) -> AppResult<Vec<CommandTemplate>> {
    let rows = sqlx::query_as::<_, CommandTemplate>(
        r#"SELECT id, title, command, description, sort_order, created_at
           FROM command_templates
           ORDER BY sort_order ASC, created_at ASC"#,
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// 创建模板命令；sort_order 自动取当前最大值 +1
pub async fn create(db: &DbPool, input: CommandTemplateCreate) -> AppResult<CommandTemplate> {
    let max_order: i64 = sqlx::query_scalar(
        r#"SELECT COALESCE(MAX(sort_order), 0) FROM command_templates"#,
    )
    .fetch_one(db)
    .await?;

    let row = sqlx::query_as::<_, CommandTemplate>(
        r#"INSERT INTO command_templates (title, command, description, sort_order)
           VALUES (?, ?, ?, ?)
           RETURNING id, title, command, description, sort_order, created_at"#,
    )
    .bind(&input.title)
    .bind(&input.command)
    .bind(&input.description)
    .bind(max_order + 1)
    .fetch_one(db)
    .await?;

    Ok(row)
}

/// 更新模板命令
pub async fn update(
    db: &DbPool,
    id: i64,
    input: CommandTemplateUpdate,
) -> AppResult<CommandTemplate> {
    let row = sqlx::query_as::<_, CommandTemplate>(
        r#"UPDATE command_templates SET
             title       = COALESCE(?, title),
             command     = COALESCE(?, command),
             description = COALESCE(?, description)
           WHERE id = ?
           RETURNING id, title, command, description, sort_order, created_at"#,
    )
    .bind(&input.title)
    .bind(&input.command)
    .bind(&input.description)
    .bind(id)
    .fetch_optional(db)
    .await?;

    row.ok_or_else(|| AppError::NotFound(format!("command template {id}")))
}

/// 删除模板命令
pub async fn delete(db: &DbPool, id: i64) -> AppResult<()> {
    let result = sqlx::query(r#"DELETE FROM command_templates WHERE id = ?"#)
        .bind(id)
        .execute(db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("command template {id}")));
    }
    Ok(())
}
