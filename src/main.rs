// ≈ index.ts / server.ts — URL shortener entry point. See architecture.md.
//
// Feature-based layout:
//   features/   -> one folder per business domain (urls, health, ...)
//   the rest    -> shared, cross-cutting code (config, errors, middleware)
mod app;
mod config;
mod errors;
mod features;
mod middleware;
mod state;

use std::{sync::Arc, time::Duration};

use sqlx::postgres::PgPoolOptions;

use crate::{config::Config, features::urls::cache::UrlCache, state::AppState};

#[tokio::main]
async fn main() {
    let config = Config::from_env();

    // 1) Postgres connection pool (≈ `new Pool({ connectionString, max: 10 })`)
    let db = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await
        .expect("failed to connect to Postgres");
    println!("✅ Connected to Postgres");

    // 2) Run pending migrations. `migrate!` embeds ./migrations/*.sql INTO the binary
    //    at compile time, so the Docker image needs no separate migration step.
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("failed to run migrations");
    println!("✅ Migrations applied");

    // 3) Redis
    let cache = UrlCache::connect(&config.redis_url)
        .await
        .expect("failed to connect to Redis");
    println!("✅ Connected to Redis");

    let port = config.port;
    let state = AppState { db, cache, config: Arc::new(config) };
    let app = app::create_app(state);

    // 0.0.0.0, not 127.0.0.1 — required to be reachable from outside a container.
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("failed to bind port");

    println!("🚀 URL shortener running on http://localhost:{port}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

// Graceful shutdown ≈ process.on("SIGTERM", () => server.close())
//
// Important for Docker: `docker stop` sends SIGTERM. Our binary runs as PID 1 in
// the container, and PID 1 ignores SIGTERM unless it handles it explicitly —
// without this, `docker stop` hangs for 10s and then force-kills the app.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("failed to listen for Ctrl+C");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to listen for SIGTERM")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    // ≈ Promise.race([ctrlC, sigterm])
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    println!("🛑 Shutdown signal received, finishing in-flight requests...");
}
