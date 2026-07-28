use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::errors::AppResult;
use crate::handlers::{ok_msg, ApiResponse};
use crate::models::{Group, GroupCreate, GroupUpdate};
use crate::service::{self, AppState};

#[derive(Deserialize)]
pub struct ListQuery {
    pub parent_id: Option<i64>,
}

pub async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<ApiResponse<Vec<Group>>>> {
    let rows = match q.parent_id {
        Some(p) => service::group::list_children(&s.db, p).await?,
        None => service::group::list_all(&s.db).await?,
    };
    Ok(ApiResponse::ok(rows))
}

pub async fn detail(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<ApiResponse<Group>>> {
    let g = service::group::get_by_id(&s.db, id).await?;
    Ok(ApiResponse::ok(g))
}

pub async fn create(
    State(s): State<AppState>,
    Json(input): Json<GroupCreate>,
) -> AppResult<Json<ApiResponse<Group>>> {
    let g = service::group::create(&s.db, input).await?;
    Ok(ApiResponse::ok(g))
}

pub async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<GroupUpdate>,
) -> AppResult<Json<ApiResponse<Group>>> {
    let g = service::group::update(&s.db, id, input).await?;
    Ok(ApiResponse::ok(g))
}

pub async fn delete(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    service::group::delete(&s.db, id).await?;
    Ok(ok_msg("deleted"))
}
