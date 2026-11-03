// GET /health — readiness check used by Docker / load balancers.
// 200 only if BOTH Postgres and Redis respond, otherwise 503.

use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde_json::{Value, json};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

async fn health(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let db_ok = sqlx::query("SELECT 1").execute(&state.db).await.is_ok();
    let cache_ok = state.cache.ping().await;

    let status = if db_ok && cache_ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    let label = |ok: bool| if ok { "up" } else { "down" };
    (
        status,
        Json(json!({
            "status": if status == StatusCode::OK { "ok" } else { "degraded" },
            "postgres": label(db_ok),
            "redis": label(cache_ok),
        })),
    )
}
