# URL Shortener (TinyURL clone) — Architecture

A learning project covering three backend fundamentals: **hashing**, **caching**,
and **database design**, built with Rust (Axum), PostgreSQL and Redis.

---

## 1. Requirements

### Functional
| # | Requirement |
|---|---|
| F1 | Given a long URL, return a short URL, e.g. `http://localhost:3000/aZ3kP9x` |
| F2 | Visiting the short URL redirects to the long URL |
| F3 | Shortening the same URL twice returns the same short code (deduplication) |
| F4 | Optional custom alias, e.g. `/my-blog` |
| F5 | Optional expiry; expired links return `410 Gone` |
| F6 | Stats per link: click count, created/expiry time |
| F7 | Delete a link |

### Non-functional
- **Read-heavy**: redirects vastly outnumber creations (typically 100:1), so reads must be fast. This is why we add a cache.
- **Short codes must be unique**: a collision would send users to the wrong site.
- **Cache failures must not break redirects**: if Redis is down, fall back to Postgres.

### Back-of-envelope sizing (why 7 characters?)
Base62 alphabet = `0-9 a-z A-Z` = 62 symbols.

| Length | Combinations |
|---|---|
| 6 | 62⁶ ≈ 56.8 billion |
| **7** | **62⁷ ≈ 3.5 trillion** |
| 8 | 62⁸ ≈ 218 trillion |

7 characters gives 3.5 trillion codes. That is plenty for a learning project and matches TinyURL/bit.ly-style lengths.

---

## 2. High-level architecture

```
                         ┌──────────────────────────────────────────┐
                         │            Axum API (Rust)               │
  POST /api/urls ───────▶│                                          │
                         │  handlers ─▶ service ─┬─▶ cache (Redis)  │──▶ Redis
  GET /{code}  ─────────▶│                       │                  │
   (302 redirect)        │                       └─▶ repository     │──▶ PostgreSQL
                         │                           (sqlx)         │
                         └──────────────────────────────────────────┘
```

### Write path: create a short URL
```
client ──POST {url}──▶ validate URL
                        │
                        ▼
                 sha256(url) = url_hash
                        │
          ┌─────────────┴──────────────┐
          │ url_hash already in DB?     │── yes ──▶ return existing code (dedupe)
          └─────────────┬──────────────┘
                        │ no
                        ▼
          code = base62_7(sha256(url + attempt))
                        │
                 INSERT INTO urls ...
                        │
         unique violation on `code`? ── yes ──▶ attempt += 1, retry (max 5)
                        │ no
                        ▼
                   201 {short_url}
```

### Read path: redirect (cache-aside pattern)
```
client ──GET /aZ3kP9x──▶ Redis GET url:aZ3kP9x
                              │
                 ┌────────────┴────────────┐
               HIT                         MISS
                 │                          │
                 │              SELECT ... FROM urls WHERE code = $1
                 │                          │
                 │               found? ── no ──▶ 404
                 │                          │ yes
                 │               expired? ─ yes ─▶ 410 Gone
                 │                          │ no
                 │               Redis SET url:aZ3kP9x (TTL)
                 │                          │
                 └──────────┬───────────────┘
                            ▼
              increment clicks (background task)
                            ▼
                  302 Location: <long_url>
```

---

## 3. Hashing: generating short codes

### Options considered
| Approach | How | Pros | Cons |
|---|---|---|---|
| **Counter + Base62** | `base62(auto_increment_id)` | No collisions, very short | Predictable (anyone can enumerate every link), needs one central counter |
| **Random** | 7 random Base62 chars | Simple, unpredictable | Collisions possible, no dedupe |
| **Hash + Base62** ✅ | 7 Base62 digits of `sha256(url)` | Deterministic, unpredictable, enables dedupe | Truncation means collisions are *possible*, so we retry |

We use **hash + Base62** because it teaches hashing, deterministic output and collision handling.

### Algorithm
```
input   = long_url               (first attempt)
        = long_url + "#" + n     (n-th retry after a collision)
digest  = SHA-256(input)                         → 32 bytes
number  = first 8 bytes of digest as u64         → 0 .. 18,446,744,073,709,551,615
code    = number written as exactly 7 Base62 digits  → "aZ3kP9x"
          (repeat 7×: digit = n % 62, n = n / 62)
```

### Two different hashes, two different jobs
| Column | Value | Purpose |
|---|---|---|
| `url_hash` | full SHA-256 hex (64 chars) of the URL | **Dedupe lookup.** Fixed-size and indexable, unlike long URLs, which can be thousands of characters and exceed B-tree index limits |
| `code` | 7-char truncated Base62 | **Public identifier.** Short and URL-safe |

### Collision handling
The `code` column is `UNIQUE`, so **the database is the source of truth**. If an
INSERT fails with a unique violation on `code`, the service re-hashes with a salt
(`#1`, `#2`, …) and retries, up to 5 times. Checking first and then inserting would
be a race condition; letting the constraint decide is not.

---

## 4. Database design (PostgreSQL)

### Schema
```sql
CREATE TABLE urls (
    id          BIGSERIAL    PRIMARY KEY,
    code        VARCHAR(32)  NOT NULL,
    long_url    TEXT         NOT NULL,
    url_hash    CHAR(64)     NOT NULL,          -- sha256 hex of long_url
    is_custom   BOOLEAN      NOT NULL DEFAULT FALSE,
    clicks      BIGINT       NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    expires_at  TIMESTAMPTZ  NULL,

    CONSTRAINT urls_code_key UNIQUE (code)
);

-- Dedupe: only ONE generated, non-expiring code per long URL.
-- Partial index: custom aliases and expiring links are excluded.
CREATE UNIQUE INDEX urls_dedupe_idx
    ON urls (url_hash)
    WHERE is_custom = FALSE AND expires_at IS NULL;
```

### Design decisions
| Decision | Why |
|---|---|
| `BIGSERIAL id` + separate `code` | Internal key is compact for joins and never exposed; `code` is the public key |
| `UNIQUE (code)` | The B-tree index makes redirect lookups O(log n) **and** guarantees collision safety |
| `url_hash` instead of indexing `long_url` | Fixed 64 bytes; B-tree entries are limited to about 2.7 KB, and URLs can be longer |
| **Partial** unique index on `url_hash` | Dedupe only applies to auto-generated permanent links; the same URL may have many custom aliases |
| `TIMESTAMPTZ` | Always store time with time zone (UTC) |
| `clicks` on the same row | Simple. At very large scale, move to a separate `clicks` events table or counters in Redis |

### Migrations
SQL files live in `migrations/` and are embedded into the binary with
`sqlx::migrate!()`. They run automatically on startup (like Prisma/Knex migrations).

---

## 5. Caching (Redis)

### Pattern: cache-aside (lazy loading)
1. Read: check Redis → on a miss, read Postgres → write to Redis.
2. Write/delete: write Postgres → **invalidate** (DEL) the Redis key.

### Keys and TTL
| Key | Value | TTL |
|---|---|---|
| `url:{code}` | long URL (string) | 24 h, or less if the link expires sooner |

The TTL is capped by `expires_at`, so an expired link can never be served from cache.

### Why Redis rather than an in-process cache?
| | In-process (e.g. `moka`) | Redis ✅ |
|---|---|---|
| Speed | Fastest (no network) | ~0.1–1 ms |
| Shared across app instances | ❌ each instance has its own | ✅ |
| Survives app restart | ❌ | ✅ |

With several API containers behind a load balancer, a shared cache keeps hit rates high
and makes invalidation simple.

### Resilience
Every Redis error is **logged and ignored**; the request falls back to Postgres.
The cache is an optimisation, not a dependency.

Every Redis call also has a **100 ms timeout**. Lesson learned while testing: with
Redis stopped, the client kept retrying its connection and a redirect took
**18.9 s** before falling back. With the timeout it takes about **0.1 s**. A cache
that is slower than the database is worse than no cache.

### HTTP redirect: 302, not 301
`301 Moved Permanently` is cached **by the browser**, so repeat visits never reach us
and clicks can't be counted. `302 Found` keeps every visit flowing through the
server (and its Redis cache).

---

## 6. API

| Method | Path | Body | Response |
|---|---|---|---|
| `POST` | `/api/urls` | `{ "url": "https://…", "custom_alias"?: "my-blog", "expires_in_days"?: 7 }` | `201` link info |
| `GET` | `/{code}` | – | `302` redirect · `404` · `410` expired |
| `GET` | `/api/urls/{code}` | – | `200` stats (clicks, dates) |
| `DELETE` | `/api/urls/{code}` | – | `204` (also evicts the cache) |
| `GET` | `/health` | – | `200` if Postgres + Redis are reachable, else `503` |

Errors are always JSON: `{ "error": "message" }`.

---

## 7. Project structure (feature-based)

```
├── architecture.md
├── Dockerfile               multi-stage build (+ corporate CA support, see certs/)
├── compose.yaml             api + postgres + redis
├── .env.example             config for local `cargo run`
├── build.rs                 recompile when migrations change (they're embedded)
├── certs/                   corporate proxy CA certs for Docker builds (git-ignored)
├── migrations/
│   └── 0001_create_urls.sql
└── src/
    ├── main.rs              startup: config → DB pool → migrations → Redis → serve
    ├── app.rs               router: mounts features + middleware
    ├── config.rs            env vars (DATABASE_URL, REDIS_URL, BASE_URL, PORT)
    ├── state.rs             AppState { db, redis, config }
    ├── errors.rs            AppError → JSON HTTP responses
    ├── middleware/logger.rs request logging
    └── features/
        ├── health.rs        GET /health (checks DB + Redis)
        └── urls/
            ├── model.rs       DB row + request/response DTOs
            ├── shortcode.rs   🔑 HASHING: sha256 → base62, alias validation
            ├── repository.rs  🗄️ DATABASE: SQL queries (sqlx)
            ├── cache.rs       ⚡ CACHING: Redis get/set/delete
            ├── service.rs     business logic: dedupe, collision retry, cache-aside
            ├── handlers.rs    HTTP layer
            └── routes.rs      URL → handler mapping
```

---

## 8. Build steps (how this project was built)

| Step | What | Concept |
|---|---|---|
| 1 | Write this document | System design |
| 2 | Add Postgres + Redis to Docker Compose, config via env vars | Infrastructure |
| 3 | Migration + repository with `sqlx` | Database design |
| 4 | `shortcode.rs` with unit tests | Hashing |
| 5 | `cache.rs` + cache-aside in the service | Caching |
| 6 | Service, handlers, routes; remove the old `users` feature | API |
| 7 | End-to-end test in Docker | Verification |

---

## 9. Running

```bash
# Everything in Docker
docker compose up --build

# Or: infra in Docker, app on host (faster dev loop)
cp .env.example .env
docker compose up -d postgres redis
cargo run
```

```bash
curl -X POST localhost:3000/api/urls -H 'content-type: application/json' \
     -d '{"url":"https://www.rust-lang.org/learn"}'
curl -i localhost:3000/<code>           # 302 → Location: https://www.rust-lang.org/learn
curl localhost:3000/api/urls/<code>     # stats
```

---

## 10. Verified behaviour

| Scenario | Result |
|---|---|
| Create link | `201`, 7-char code |
| Same URL again (even `HTTPS://WWW.Rust-Lang.org/...`) | `200`, **same** code (normalise + dedupe) |
| Redirect, 1st / 2nd time | `302` with `x-cache: MISS`, then `x-cache: HIT` |
| Forced hash collision | logged `🔁 collision`, re-hashed with `#1`, `201` with a different code |
| Custom alias taken / reserved / bad URL | `409` / `400` / `400` |
| Expired link | `410 Gone` |
| Delete, then redirect | `204`, then `404` (cache invalidated) |
| Redis stopped | redirects still `302` (~0.1 s), `/health` → `503 degraded` |
| Redis restarted | cache refills automatically (`MISS` → `HIT`) |

---

## 11. Ideas for later (exercises)
- **Rate limiting** on `POST /api/urls` (Redis `INCR` + `EXPIRE`, per IP)
- **Negative caching**: cache "not found" briefly so spam on bad codes doesn't hit the DB
- **Batched click counting**: `INCR clicks:{code}` in Redis, flush to Postgres every N seconds
- **Counter-based codes** using a Postgres sequence + Base62, and compare with hashing
- **Analytics table**: `clicks(code, ts, referrer, user_agent, country)`
- **Integration tests** with `sqlx::test`
