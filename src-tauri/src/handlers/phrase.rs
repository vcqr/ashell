use axum::extract::{Path, State};
use axum::Json;

use crate::errors::AppResult;
use crate::handlers::ApiResponse;
use crate::models::{QuickPhrase, QuickPhraseCreate};
use crate::service::AppState;

pub async fn list(State(s): State<AppState>) -> AppResult<Json<ApiResponse<Vec<QuickPhrase>>>> {
    let phrases = crate::service::phrase::list(&s.db).await?;
    Ok(ApiResponse::ok(phrases))
}

pub async fn create(
    State(s): State<AppState>,
    Json(input): Json<QuickPhraseCreate>,
) -> AppResult<Json<ApiResponse<QuickPhrase>>> {
    let phrase = crate::service::phrase::create(&s.db, input).await?;
    Ok(ApiResponse::ok(phrase))
}

pub async fn delete(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    crate::service::phrase::delete(&s.db, id).await?;
    Ok(Json(ApiResponse {
        code: 0,
        message: "deleted".into(),
        data: None,
    }))
}

pub async fn clear_all(
    State(s): State<AppState>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    crate::service::phrase::clear_all(&s.db).await?;
    Ok(Json(ApiResponse {
        code: 0,
        message: "cleared".into(),
        data: None,
    }))
}
