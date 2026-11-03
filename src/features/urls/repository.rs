// 🗄️ DATABASE layer — the only file that writes SQL for urls.
// ≈ a repository using `pg`:  await pool.query("SELECT ... WHERE code = $1", [code])
//
// We use runtime-checked `sqlx::query_as` (not the compile-time `query_as!` macro)
// so the project builds without a live database (e.g. inside `docker build`).

use sqlx::PgPool;

use super::model::{NewUrl, UrlRecord};

/// Why an insert failed — the service reacts differently to each.
pub enum InsertError {
    /// `code` already taken → hash collision (retry) or alias taken (409)
    CodeTaken,
    /// Another request just created the same deduped URL (race) → re-read it
    DuplicateUrl,
    Db(sqlx::Error),
}

// SQL is written as string *literals*. sqlx 0.9 refuses `format!`-built SQL at
// compile time (trait `SqlSafeStr`), because building SQL dynamically is how SQL
// injection happens. Values always go through `.bind()` → $1, $2, ...

pub async fn find_by_code(db: &PgPool, code: &str) -> Result<Option<UrlRecord>, sqlx::Error> {
    sqlx::query_as::<_, UrlRecord>(
        "SELECT id, code, long_url, url_hash, is_custom, clicks, created_at, expires_at
         FROM urls WHERE code = $1",
    )
        .bind(code) // $1 — parameters are always bound, never string-concatenated (no SQL injection)
        .fetch_optional(db) // 0 rows → None, 1 row → Some(row)
        .await
}

/// Finds the existing auto-generated, permanent link for a URL (dedupe).
/// The WHERE matches the partial index `urls_dedupe_idx`, so this is an index lookup.
pub async fn find_dedupe(db: &PgPool, url_hash: &str) -> Result<Option<UrlRecord>, sqlx::Error> {
    sqlx::query_as::<_, UrlRecord>(
        "SELECT id, code, long_url, url_hash, is_custom, clicks, created_at, expires_at
         FROM urls
         WHERE url_hash = $1 AND is_custom = FALSE AND expires_at IS NULL",
    )
    .bind(url_hash)
    .fetch_optional(db)
    .await
}

pub async fn insert(db: &PgPool, new: &NewUrl) -> Result<UrlRecord, InsertError> {
    let result = sqlx::query_as::<_, UrlRecord>(
        "INSERT INTO urls (code, long_url, url_hash, is_custom, expires_at)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, code, long_url, url_hash, is_custom, clicks, created_at, expires_at",
    )
    .bind(&new.code)
    .bind(&new.long_url)
    .bind(&new.url_hash)
    .bind(new.is_custom)
    .bind(new.expires_at)
    .fetch_one(db)
    .await;

    // Let the database's UNIQUE constraints decide (no check-then-insert race).
    result.map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            match db_err.constraint() {
                Some("urls_code_key") => InsertError::CodeTaken,
                Some("urls_dedupe_idx") => InsertError::DuplicateUrl,
                _ => InsertError::Db(e),
            }
        }
        _ => InsertError::Db(e),
    })
}

pub async fn increment_clicks(db: &PgPool, code: &str) -> Result<(), sqlx::Error> {
    // Atomic in SQL — safe even with many concurrent redirects.
    sqlx::query("UPDATE urls SET clicks = clicks + 1 WHERE code = $1")
        .bind(code)
        .execute(db)
        .await?;
    Ok(())
}

/// Returns true if a row was deleted.
pub async fn delete_by_code(db: &PgPool, code: &str) -> Result<bool, sqlx::Error> {
    let res = sqlx::query("DELETE FROM urls WHERE code = $1")
        .bind(code)
        .execute(db)
        .await?;
    Ok(res.rows_affected() > 0)
}
