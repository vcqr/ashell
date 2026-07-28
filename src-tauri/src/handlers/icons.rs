//! 主机图标资源 REST 端点
//!
//! - GET /api/icons             返回 ~/.ashell/icons 下的图标列表
//! - GET /api/icons/{name}      返回单个图标的二进制内容（带 Content-Type）
//!
//! 用户只需把 svg / png / jpg / jpeg / gif / webp 文件放进 ~/.ashell/icons/ 即可。

use axum::body::Body;
use axum::extract::Path;
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;

use crate::errors::AppResult;
use crate::handlers::ApiResponse;
use crate::service::icons as icons_svc;

pub async fn list() -> AppResult<Json<ApiResponse<Vec<icons_svc::IconItem>>>> {
    let items = icons_svc::list()?;
    Ok(ApiResponse::ok(items))
}

pub async fn get(Path(name): Path<String>) -> AppResult<axum::response::Response> {
    let (path, mime) = icons_svc::resolve(&name)?;
    let bytes = tokio::fs::read(&path).await?;
    let resp = (
        [
            (header::CONTENT_TYPE, mime),
            // 图标基本不变，给个适度缓存即可
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        Body::from(bytes),
    )
        .into_response();
    Ok(resp)
}
