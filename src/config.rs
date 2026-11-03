// ≈ config/index.ts — all settings come from environment variables
// (12-factor style). Locally they're loaded from `.env` by dotenvy (≈ `dotenv` in Node);
// in Docker they come from compose.yaml.

#[derive(Clone)]
pub struct Config {
    pub port: u16,
    /// Public base for generated links, e.g. "http://localhost:3000" or "https://sho.rt"
    pub base_url: String,
    pub database_url: String,
    pub redis_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        // Loads `.env` if present; silently does nothing in Docker where there is none.
        dotenvy::dotenv().ok();

        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(3000);

        Config {
            port,
            base_url: std::env::var("BASE_URL")
                .unwrap_or_else(|_| format!("http://localhost:{port}"))
                .trim_end_matches('/')
                .to_string(),
            // `required` crashes at startup with a clear message — fail fast,
            // better than failing on the first request.
            database_url: required("DATABASE_URL"),
            redis_url: required("REDIS_URL"),
        }
    }
}

fn required(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("missing required env var {key}"))
}
