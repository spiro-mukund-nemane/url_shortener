// HTTP layer for urls — extract input, call service, shape the response.

use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

use super::{
    model::{CreateUrlRequest, UrlResponse},
    service::{self, CacheStatus},
};
use crate::{errors::AppError, state::AppState};

// POST /api/urls  → 201 Created (new) or 200 OK (existing deduped link)
pub async fn create_url(
    State(state): State<AppState>,
    Json(body): Json<CreateUrlRequest>,
) -> Result<(StatusCode, Json<UrlResponse>), AppError> {
    let created = service::create(&state, body).await?;
    let status = if created.is_new { StatusCode::CREATED } else { StatusCode::OK };
    Ok((status, Json(UrlResponse::from_record(created.record, &state.config.base_url))))
}

// GET /{code}  → 302 redirect
pub async fn redirect(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Response, AppError> {
    let (long_url, cache) = service::resolve(&state, &code).await?;
    let cache_header = match cache {
        CacheStatus::Hit => "HIT",
        CacheStatus::Miss => "MISS",
    };

    // 302 (not 301): browsers cache 301 forever, so we'd never see repeat clicks.
    // ≈ res.set("X-Cache", ...).redirect(302, longUrl)
    Ok((
        StatusCode::FOUND,
        [(header::LOCATION, long_url), (header::HeaderName::from_static("x-cache"), cache_header.to_string())],
    )
        .into_response())
}

// GET /api/urls/{code}  → stats
pub async fn get_stats(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<UrlResponse>, AppError> {
    let record = service::stats(&state, &code).await?;
    Ok(Json(UrlResponse::from_record(record, &state.config.base_url)))
}

// DELETE /api/urls/{code}  → 204 No Content
pub async fn delete_url(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<StatusCode, AppError> {
    service::delete(&state, &code).await?;
    Ok(StatusCode::NO_CONTENT)
}
