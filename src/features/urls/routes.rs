use axum::{Router, routing::get};

use super::handlers;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/urls", axum::routing::post(handlers::create_url))
        .route("/api/urls/{code}", get(handlers::get_stats).delete(handlers::delete_url))
        // Catch-all short code. Static routes like /health still win over this
        // because Axum's router prefers exact matches over `{param}` segments.
        .route("/{code}", get(handlers::redirect))
}
