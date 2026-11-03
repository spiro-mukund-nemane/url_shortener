// URL shortener feature. See architecture.md §7 for what each file does.
pub mod cache; //      ⚡ caching   (Redis)
pub mod handlers;
pub mod model;
pub mod repository; // 🗄️ database  (Postgres)
pub mod routes;
pub mod service;
pub mod shortcode; //  🔑 hashing   (sha256 → base62)

pub use routes::router;
