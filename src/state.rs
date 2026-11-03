// Shared state handed to every handler via `State(...)`.
// All fields are cheap to clone (they're handles / Arc), which Axum requires.

use std::sync::Arc;

use sqlx::PgPool;

use crate::{config::Config, features::urls::cache::UrlCache};

#[derive(Clone)]
pub struct AppState {
    /// Postgres connection pool (≈ `new Pool()` from `pg`) — reuses connections.
    pub db: PgPool,
    pub cache: UrlCache,
    pub config: Arc<Config>,
}
