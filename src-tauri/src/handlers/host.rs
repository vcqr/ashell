use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::errors::AppResult;
use crate::handlers::{ok_msg, ApiResponse};
use crate::models::{Host, HostCreate, HostUpdate};
use crate::service::{self, ssh_config::SshConfigHost, AppState};

#[derive(Deserialize)]
pub struct ListQuery {
    pub gid: Option<i64>,
    /// 是否联表返回 group_name / parent_gid
    pub with_group: Option<bool>,
}

pub async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> AppResult<axum::response::Response> {
    if q.with_group.unwrap_or(false) {
        let rows = service::host::list_with_group(&s.db, q.gid).await?;
        Ok(ApiResponse::ok(rows).into_response())
    } else {
        let rows = service::host::list(&s.db, q.gid).await?;
        Ok(ApiResponse::ok(rows).into_response())
    }
}

pub async fn detail(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<ApiResponse<Host>>> {
    let h = service::host::get_by_id(&s.db, id).await?;
    Ok(ApiResponse::ok(h))
}

pub async fn create(
    State(s): State<AppState>,
    Json(input): Json<HostCreate>,
) -> AppResult<Json<ApiResponse<Host>>> {
    let h = service::host::create(&s.db, &s.config.crypto_key, input).await?;
    Ok(ApiResponse::ok(h))
}

pub async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<HostUpdate>,
) -> AppResult<Json<ApiResponse<Host>>> {
    let h = service::host::update(&s.db, &s.config.crypto_key, id, input).await?;
    Ok(ApiResponse::ok(h))
}

pub async fn delete(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    service::host::delete(&s.db, id).await?;
    Ok(ok_msg("deleted"))
}

/// 解析 ~/.ssh/config，返回可导入的主机列表
pub async fn ssh_config() -> AppResult<Json<ApiResponse<Vec<SshConfigHost>>>> {
    let hosts = service::ssh_config::parse_ssh_config()
        .map_err(|e| crate::errors::AppError::Internal(format!("读取 ssh config 失败: {e}")))?;
    Ok(ApiResponse::ok(hosts))
}
