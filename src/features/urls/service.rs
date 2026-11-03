// Business logic for the urls feature — ties hashing, database and cache together.
// No HTTP types here (≈ a framework-agnostic service class in Node).

use chrono::{Duration, Utc};
use sqlx::PgPool;

use super::{
    model::{CreateUrlRequest, NewUrl, UrlRecord},
    repository::{self, InsertError},
    shortcode,
};
use crate::{errors::AppError, state::AppState};

/// How many salted re-hashes to try on a code collision before giving up.
/// With 3.5 trillion possible codes, needing even 2 attempts is rare.
const MAX_ATTEMPTS: u32 = 5;
const MAX_EXPIRY_DAYS: u32 = 3650;

/// Result of creating a link: the record + whether it was newly created
/// (false = an existing deduped link was returned).
pub struct Created {
    pub record: UrlRecord,
    pub is_new: bool,
}

// ───────────────────────────── create ─────────────────────────────

pub async fn create(state: &AppState, req: CreateUrlRequest) -> Result<Created, AppError> {
    let long_url = shortcode::normalize_url(&req.url).map_err(AppError::BadRequest)?;
    let url_hash = shortcode::sha256_hex(&long_url);

    let expires_at = match req.expires_in_days {
        Some(d) if d == 0 || d > MAX_EXPIRY_DAYS => {
            return Err(AppError::BadRequest(format!(
                "expires_in_days must be 1-{MAX_EXPIRY_DAYS}"
            )));
        }
        Some(d) => Some(Utc::now() + Duration::days(d.into())),
        None => None,
    };

    // 1) Custom alias: user picks the code, no hashing / dedupe.
    if let Some(alias) = req.custom_alias {
        shortcode::validate_alias(&alias).map_err(AppError::BadRequest)?;
        let new = NewUrl { code: alias.clone(), long_url, url_hash, is_custom: true, expires_at };
        return match repository::insert(&state.db, &new).await {
            Ok(record) => Ok(Created { record, is_new: true }),
            Err(InsertError::CodeTaken) => {
                Err(AppError::Conflict(format!("alias '{alias}' is already taken")))
            }
            Err(InsertError::DuplicateUrl) => Err(AppError::Internal("unexpected dedupe".into())),
            Err(InsertError::Db(e)) => Err(e.into()),
        };
    }

    // 2) Dedupe: same permanent URL → same code (uses partial index urls_dedupe_idx).
    if expires_at.is_none() {
        if let Some(record) = repository::find_dedupe(&state.db, &url_hash).await? {
            return Ok(Created { record, is_new: false });
        }
    }

    // 3) Hash → code, retry with a salt on collision. The UNIQUE constraint decides.
    for attempt in 0..MAX_ATTEMPTS {
        let code = shortcode::generate_code(&long_url, attempt);
        let new = NewUrl {
            code: code.clone(),
            long_url: long_url.clone(),
            url_hash: url_hash.clone(),
            is_custom: false,
            expires_at,
        };
        match repository::insert(&state.db, &new).await {
            Ok(record) => return Ok(Created { record, is_new: true }),
            Err(InsertError::CodeTaken) => {
                println!("🔁 collision on code {code} (attempt {attempt}), re-hashing");
                continue;
            }
            // Race: a concurrent request inserted the same URL between our dedupe
            // check and our insert → just return theirs.
            Err(InsertError::DuplicateUrl) => {
                let record = repository::find_dedupe(&state.db, &url_hash)
                    .await?
                    .ok_or_else(|| AppError::Internal("dedupe row vanished".into()))?;
                return Ok(Created { record, is_new: false });
            }
            Err(InsertError::Db(e)) => return Err(e.into()),
        }
    }

    Err(AppError::Internal(format!(
        "could not generate a unique code after {MAX_ATTEMPTS} attempts"
    )))
}

// ───────────────────────────── resolve (redirect) ─────────────────────────────

pub enum CacheStatus {
    Hit,
    Miss,
}

/// Cache-aside read: Redis first, Postgres on miss, then populate Redis.
pub async fn resolve(state: &AppState, code: &str) -> Result<(String, CacheStatus), AppError> {
    // 1) Cache hit → no database query at all.
    if let Some(long_url) = state.cache.get(code).await {
        count_click(state.db.clone(), code);
        return Ok((long_url, CacheStatus::Hit));
    }

    // 2) Miss → database.
    let record = repository::find_by_code(&state.db, code)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("short url '{code}' not found")))?;

    if record.is_expired() {
        return Err(AppError::Gone(format!("short url '{code}' has expired")));
    }

    // 3) Populate cache for next time.
    state.cache.set(code, &record.long_url, record.expires_at).await;
    count_click(state.db.clone(), code);
    Ok((record.long_url, CacheStatus::Miss))
}

/// Fire-and-forget: don't make the user wait for the UPDATE.
/// ≈ `void incrementClicks(code)` without await in Node.
/// `tokio::spawn` needs owned data ('static), hence the owned PgPool clone (cheap,
/// it's a handle) and `code.to_string()`; `async move` moves them into the task.
fn count_click(db: PgPool, code: &str) {
    let code = code.to_string();
    tokio::spawn(async move {
        if let Err(e) = repository::increment_clicks(&db, &code).await {
            eprintln!("⚠️  failed to count click for {code}: {e}");
        }
    });
}

// ───────────────────────────── stats / delete ─────────────────────────────

pub async fn stats(state: &AppState, code: &str) -> Result<UrlRecord, AppError> {
    repository::find_by_code(&state.db, code)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("short url '{code}' not found")))
}

pub async fn delete(state: &AppState, code: &str) -> Result<(), AppError> {
    if !repository::delete_by_code(&state.db, code).await? {
        return Err(AppError::NotFound(format!("short url '{code}' not found")));
    }
    // Write DB first, THEN invalidate cache (cache-aside write rule).
    state.cache.delete(code).await;
    Ok(())
}

