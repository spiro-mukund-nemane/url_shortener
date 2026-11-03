// AppError → JSON HTTP response. ≈ custom error classes + Express error middleware.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    BadRequest(String), // 400
    NotFound(String),   // 404
    Conflict(String),   // 409 e.g. alias taken
    Gone(String),       // 410 expired link
    Internal(String),   // 500 — details logged, NOT sent to the client
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, m),
            AppError::Conflict(m) => (StatusCode::CONFLICT, m),
            AppError::Gone(m) => (StatusCode::GONE, m),
            AppError::Internal(m) => {
                eprintln!("❌ internal error: {m}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal server error".to_string())
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

/// Lets `?` convert database errors automatically:
///   repository::find(...).await?   // sqlx::Error → AppError::Internal
impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(format!("database error: {e}"))
    }
}
