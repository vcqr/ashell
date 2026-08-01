use axum::extract::{Path, State};
use axum::Json;

use crate::errors::AppResult;
use crate::handlers::ApiResponse;
use crate::models::{
    AiEngine, AiEngineActivate, AiEngineUpdate, AiEnginesState, AiProvider, AiProviderCreate,
    AiProviderUpdate,
};
use crate::service::AppState;

pub async fn list(State(s): State<AppState>) -> AppResult<Json<ApiResponse<Vec<AiProvider>>>> {
    let providers = crate::service::ai_provider::list(&s.db, &s.config.crypto_key).await?;
    Ok(ApiResponse::ok(providers))
}

pub async fn detail(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<AiProvider>>> {
    let provider =
        crate::service::ai_provider::get_by_id(&s.db, &s.config.crypto_key, &id).await?;
    Ok(ApiResponse::ok(provider))
}

pub async fn create(
    State(s): State<AppState>,
    Json(input): Json<AiProviderCreate>,
) -> AppResult<Json<ApiResponse<AiProvider>>> {
    let provider =
        crate::service::ai_provider::create(&s.db, &s.config.crypto_key, input).await?;
    Ok(ApiResponse::ok(provider))
}

pub async fn update(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(input): Json<AiProviderUpdate>,
) -> AppResult<Json<ApiResponse<AiProvider>>> {
    let provider =
        crate::service::ai_provider::update(&s.db, &s.config.crypto_key, &id, input).await?;
    Ok(ApiResponse::ok(provider))
}

pub async fn delete(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<serde_json::Value>>> {
    crate::service::ai_provider::delete(&s.db, &id).await?;
    Ok(Json(ApiResponse {
        code: 0,
        message: "deleted".into(),
        data: None,
    }))
}

// ────────────────────────── 引擎配置 ──────────────────────────

pub async fn list_engines(
    State(s): State<AppState>,
) -> AppResult<Json<ApiResponse<AiEnginesState>>> {
    let state = crate::service::ai_provider::list_engines(&s.db, &s.config.crypto_key).await?;
    Ok(ApiResponse::ok(state))
}

pub async fn update_engine(
    State(s): State<AppState>,
    Path(engine): Path<String>,
    Json(input): Json<AiEngineUpdate>,
) -> AppResult<Json<ApiResponse<AiEngine>>> {
    let updated =
        crate::service::ai_provider::update_engine(&s.db, &s.config.crypto_key, &engine, input)
            .await?;
    Ok(ApiResponse::ok(updated))
}

pub async fn activate_engine(
    State(s): State<AppState>,
    Json(input): Json<AiEngineActivate>,
) -> AppResult<Json<ApiResponse<AiEnginesState>>> {
    let state =
        crate::service::ai_provider::activate_engine(&s.db, &s.config.crypto_key, &input.engine)
            .await?;
    Ok(ApiResponse::ok(state))
}
