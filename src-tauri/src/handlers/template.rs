use axum::extract::{Path, State};
use axum::Json;

use crate::errors::AppResult;
use crate::handlers::ApiResponse;
use crate::models::{CommandTemplate, CommandTemplateCreate, CommandTemplateUpdate};
use crate::service::AppState;

pub async fn list(
    State(s): State<AppState>,
) -> AppResult<Json<ApiResponse<Vec<CommandTemplate>>>> {
    let templates = crate::service::template::list(&s.db).await?;
    Ok(ApiResponse::ok(templates))
}

pub async fn create(
    State(s): State<AppState>,
    Json(input): Json<CommandTemplateCreate>,
) -> AppResult<Json<ApiResponse<CommandTemplate>>> {
    let template = crate::service::template::create(&s.db, input).await?;
    Ok(ApiResponse::ok(template))
}

pub async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<CommandTemplateUpdate>,
) -> AppResult<Json<ApiResponse<CommandTemplate>>> {
    let template = crate::service::template::update(&s.db, id, input).await?;
    Ok(ApiResponse::ok(template))
}

pub async fn delete(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    crate::service::template::delete(&s.db, id).await?;
    Ok(Json(ApiResponse {
        code: 0,
        message: "deleted".into(),
        data: None,
    }))
}
