// ⚡ CACHING — Redis, cache-aside pattern.  (architecture.md §5)
//
//   key:   url:{code}   value: long_url   TTL: ≤ 24h (and never past expires_at)
//
// Resilience rule: the cache is an optimisation, not a dependency. Every Redis
// error is logged and swallowed, so callers just see a "miss" and use Postgres.
//
// ≈ in Node with ioredis:
//   await redis.get(`url:${code}`);  await redis.set(key, url, "EX", ttl);  await redis.del(key)

use std::{future::Future, time::Duration};

use chrono::{DateTime, Utc};
use redis::{AsyncCommands, RedisResult, aio::ConnectionManager};

const DEFAULT_TTL_SECS: u64 = 24 * 60 * 60;

/// Hard cap on every Redis call. Without it, when Redis is down the client keeps
/// retrying the connection and a redirect can hang for ~20s before "falling back".
/// A cache that's slower than the database is worse than no cache.
const CACHE_TIMEOUT: Duration = Duration::from_millis(100);

/// Runs a Redis command with a timeout; logs and returns None on error/timeout.
/// ≈ Promise.race([redisCall, sleep(100).then(() => { throw "timeout" })])
async fn with_timeout<T>(op: &str, code: &str, fut: impl Future<Output = RedisResult<T>>) -> Option<T> {
    match tokio::time::timeout(CACHE_TIMEOUT, fut).await {
        Ok(Ok(value)) => Some(value),
        Ok(Err(e)) => {
            eprintln!("⚠️  cache {op} failed for {code}: {e}");
            None
        }
        Err(_) => {
            eprintln!("⚠️  cache {op} timed out for {code} (>{CACHE_TIMEOUT:?})");
            None
        }
    }
}

/// Clone is cheap: ConnectionManager is a handle to one shared, auto-reconnecting
/// connection (≈ reusing one ioredis client everywhere).
#[derive(Clone)]
pub struct UrlCache {
    conn: ConnectionManager,
}

fn key(code: &str) -> String {
    format!("url:{code}")
}

impl UrlCache {
    pub async fn connect(redis_url: &str) -> redis::RedisResult<Self> {
        let client = redis::Client::open(redis_url)?;
        let conn = ConnectionManager::new(client).await?;
        Ok(UrlCache { conn })
    }

    /// Some(long_url) on hit, None on miss OR on Redis error.
    pub async fn get(&self, code: &str) -> Option<String> {
        let mut conn = self.conn.clone(); // commands need `&mut`, so use our own handle
        with_timeout("GET", code, conn.get::<_, Option<String>>(key(code)))
            .await
            .flatten() // Option<Option<String>> → Option<String>
    }

    pub async fn set(&self, code: &str, long_url: &str, expires_at: Option<DateTime<Utc>>) {
        // Cap TTL so an expiring link can never outlive its expiry in the cache.
        let ttl = match expires_at {
            Some(t) => {
                let secs_left = (t - Utc::now()).num_seconds();
                if secs_left <= 0 {
                    return; // already expired: don't cache
                }
                (secs_left as u64).min(DEFAULT_TTL_SECS)
            }
            None => DEFAULT_TTL_SECS,
        };

        let mut conn = self.conn.clone();
        with_timeout("SET", code, conn.set_ex::<_, _, ()>(key(code), long_url, ttl)).await;
    }

    /// Invalidate after delete — otherwise a deleted link would keep redirecting
    /// until its TTL ran out.
    pub async fn delete(&self, code: &str) {
        let mut conn = self.conn.clone();
        with_timeout("DEL", code, conn.del::<_, ()>(key(code))).await;
    }

    pub async fn ping(&self) -> bool {
        let mut conn = self.conn.clone();
        // `cmd` must live in a variable: the future borrows it until awaited.
        // (Inlining `redis::cmd("PING")` creates a temporary that's dropped too early.)
        let cmd = redis::cmd("PING");
        with_timeout("PING", "-", cmd.query_async::<String>(&mut conn)).await.is_some()
    }
}
