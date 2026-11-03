use axum::Router;

use crate::{features, middleware::logger::log_requests, state::AppState};

pub fn create_app(state: AppState) -> Router {
    Router::new()
        .merge(features::health::router())
        .merge(features::urls::router())
        .layer(axum::middleware::from_fn(log_requests))
        .with_state(state)
}
