// Types for the urls feature.
//   UrlRecord      -> one row of the `urls` table (≈ a Prisma model)
//   CreateUrlRequest / UrlResponse -> API DTOs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A row from the `urls` table. `sqlx::FromRow` maps columns → fields by name
/// (≈ the row object `pg` gives you, but typed and checked).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UrlRecord {
    #[allow(dead_code)] // loaded from DB, not used by the API (internal key)
    pub id: i64,
    pub code: String,
    pub long_url: String,
    #[allow(dead_code)] // used in SQL lookups, not read in Rust
    pub url_hash: String,
    pub is_custom: bool,
    pub clicks: i64,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>, // nullable column → Option
}

impl UrlRecord {
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|t| t <= Utc::now())
    }
}

/// Data needed to insert a new row (id, clicks, created_at come from DB defaults).
pub struct NewUrl {
    pub code: String,
    pub long_url: String,
    #[allow(dead_code)] // used in SQL lookups, not read in Rust
    pub url_hash: String,
    pub is_custom: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

/// POST /api/urls body.  `Option` fields may be omitted in JSON (≈ `field?:` in TS).
#[derive(Debug, Deserialize)]
pub struct CreateUrlRequest {
    pub url: String,
    pub custom_alias: Option<String>,
    pub expires_in_days: Option<u32>,
}

/// What the API returns for a link.
#[derive(Debug, Serialize)]
pub struct UrlResponse {
    pub code: String,
    pub short_url: String,
    pub long_url: String,
    pub is_custom: bool,
    pub clicks: i64,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl UrlResponse {
    pub fn from_record(r: UrlRecord, base_url: &str) -> Self {
        UrlResponse {
            short_url: format!("{base_url}/{}", r.code),
            code: r.code,
            long_url: r.long_url,
            is_custom: r.is_custom,
            clicks: r.clicks,
            created_at: r.created_at,
            expires_at: r.expires_at,
        }
    }
}
