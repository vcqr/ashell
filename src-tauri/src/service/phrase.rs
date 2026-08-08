use sqlx::QueryBuilder;

use crate::errors::{AppError, AppResult};
use crate::models::{DbPool, QuickPhrase, QuickPhraseCreate};

/// 列出全部常用语，按 sort_order -> created_at 排序
pub async fn list(db: &DbPool) -> AppResult<Vec<QuickPhrase>> {
    let rows = sqlx::query_as::<_, QuickPhrase>(
        r#"SELECT id, content, sort_order, created_at
           FROM quick_phrases
           ORDER BY sort_order ASC, created_at ASC"#,
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// 创建常用语；sort_order 自动取当前最大值 +1
pub async fn create(db: &DbPool, input: QuickPhraseCreate) -> AppResult<QuickPhrase> {
    let max_order: i64 = sqlx::query_scalar(
        r#"SELECT COALESCE(MAX(sort_order), 0) FROM quick_phrases"#,
    )
    .fetch_one(db)
    .await?;

    let row = sqlx::query_as::<_, QuickPhrase>(
        r#"INSERT INTO quick_phrases (content, sort_order)
           VALUES (?, ?)
           RETURNING id, content, sort_order, created_at"#,
    )
    .bind(&input.content)
    .bind(max_order + 1)
    .fetch_one(db)
    .await?;

    Ok(row)
}

/// 删除常用语
pub async fn delete(db: &DbPool, id: i64) -> AppResult<()> {
    let result = sqlx::query(r#"DELETE FROM quick_phrases WHERE id = ?"#)
        .bind(id)
        .execute(db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("quick phrase {id}")));
    }
    Ok(())
}

/// 清空全部常用语
pub async fn clear_all(db: &DbPool) -> AppResult<()> {
    sqlx::query(r#"DELETE FROM quick_phrases"#)
        .execute(db)
        .await?;
    Ok(())
}

/// 按 content 判断是否已存在（用于前端去重检查）
#[allow(dead_code)]
pub async fn exists_by_content(db: &DbPool, content: &str) -> AppResult<bool> {
    let count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM quick_phrases WHERE content = ?"#,
    )
    .bind(content)
    .fetch_one(db)
    .await?;
    Ok(count > 0)
}

/// 批量更新排序（拖拽排序用）
#[allow(dead_code)]
pub async fn reorder(db: &DbPool, ids: &[i64]) -> AppResult<()> {
    let mut tx = db.begin().await?;
    for (idx, id) in ids.iter().enumerate() {
        let mut builder = QueryBuilder::new("UPDATE quick_phrases SET sort_order = ");
        builder.push_bind(idx as i64);
        builder.push(" WHERE id = ");
        builder.push_bind(id);
        builder.build().execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}
