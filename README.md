# url_shortener

A TinyURL-style URL shortener built in **Rust** to learn backend fundamentals:
**hashing**, **caching** and **database design**.

- **Axum** web framework (similar to Express/Hono)
- **PostgreSQL** via `sqlx` with embedded migrations
- **Redis** cache using the cache-aside pattern
- **Docker Compose** for the whole stack

> 📐 Design decisions, diagrams and trade-offs are in **[architecture.md](architecture.md)**.

---

## Features

- Shorten any `http`/`https` URL to a 7-character code (`SHA-256 → Base62`)
- **Deduplication**: the same URL always returns the same code
- **Collision-safe**: re-hashes with a salt when a code is already taken
- **Custom aliases** (`/my-blog`) and **expiring links** (`410 Gone` after expiry)
- **Click counting** and per-link stats
- **Redis cache** for redirects, with a 100 ms timeout and automatic fallback to Postgres
- Health check, graceful shutdown, non-root Docker image

---

## Quick start

### Option A: everything in Docker
```bash
docker compose up --build -d
curl localhost:3000/health
# {"postgres":"up","redis":"up","status":"ok"}
```

### Option B: database and cache in Docker, app on your machine (faster dev loop)
```bash
cp .env.example .env
docker compose up -d postgres redis
cargo run
```

Stop everything with `docker compose down`. Add `-v` to also delete the database data.

---

## API

### Create a short URL
```bash
curl -X POST localhost:3000/api/urls \
  -H 'content-type: application/json' \
  -d '{"url": "https://www.rust-lang.org/learn"}'
```
```json
{
  "code": "SU8annG",
  "short_url": "http://localhost:3000/SU8annG",
  "long_url": "https://www.rust-lang.org/learn",
  "is_custom": false,
  "clicks": 0,
  "created_at": "2026-11-03T09:52:48.567503Z",
  "expires_at": null
}
```
Optional fields: `"custom_alias": "my-blog"` (3–32 chars, `A-Z a-z 0-9 - _`) and `"expires_in_days": 7`.

### All endpoints

| Method | Path | Success | Errors |
|---|---|---|---|
| `POST` | `/api/urls` | `201` created · `200` existing (deduped) | `400` invalid input · `409` alias taken |
| `GET` | `/{code}` | `302` redirect (header `x-cache: HIT\|MISS`) | `404` · `410` expired |
| `GET` | `/api/urls/{code}` | `200` stats | `404` |
| `DELETE` | `/api/urls/{code}` | `204` | `404` |
| `GET` | `/health` | `200` | `503` if Postgres or Redis is down |

Errors are always JSON: `{"error": "message"}`.

```bash
curl -i localhost:3000/SU8annG          # 302 → Location: https://www.rust-lang.org/learn
curl localhost:3000/api/urls/SU8annG    # stats incl. clicks
curl -X DELETE localhost:3000/api/urls/SU8annG
```

---

## Configuration

Set via environment variables. Locally they are read from `.env`; in Docker they come from `compose.yaml`.

| Variable | Example | Required |
|---|---|---|
| `DATABASE_URL` | `postgres://shortener:shortener@localhost:5432/shortener` | ✅ |
| `REDIS_URL` | `redis://localhost:6379` | ✅ |
| `BASE_URL` | `http://localhost:3000` (prefix for `short_url`) | – |
| `PORT` | `3000` | – |

---

## Project structure

Feature-based layout: everything for one domain lives in one folder.

```
├── architecture.md         design doc (start here to understand the "why")
├── compose.yaml            api + postgres + redis
├── Dockerfile              multi-stage build → small runtime image
├── build.rs                rebuild when migrations change
├── migrations/             SQL, embedded into the binary, run on startup
├── certs/                  corporate proxy CA certs for Docker builds (see certs/README.md)
└── src/
    ├── main.rs             startup: config → Postgres → migrations → Redis → serve
    ├── app.rs              router + global middleware
    ├── config.rs           env vars
    ├── state.rs            shared state (DB pool, cache, config)
    ├── errors.rs           AppError → JSON responses
    ├── middleware/         request logger
    └── features/
        ├── health.rs
        └── urls/
            ├── shortcode.rs    🔑 hashing (sha256 → base62), validation
            ├── repository.rs   🗄️ SQL queries
            ├── cache.rs        ⚡ Redis
            ├── service.rs      business logic
            ├── handlers.rs     HTTP layer
            ├── routes.rs
            └── model.rs
```

---

## Development

| Task | Command | Node/Bun equivalent |
|---|---|---|
| Run | `cargo run` | `bun run index.ts` |
| Type-check only (fast) | `cargo check` | `tsc --noEmit` |
| Unit tests | `cargo test` | `bun test` |
| Format / lint | `cargo fmt` / `cargo clippy` | prettier / eslint |
| Release build | `cargo build --release` | `bun build` |
| Auto-reload | `cargo install bacon` → `bacon run` | `bun --watch` |
| Open DB shell | `docker exec -it url_shortener_db psql -U shortener` | |
| Inspect cache | `docker exec -it url_shortener_cache redis-cli KEYS 'url:*'` | |

### Adding a migration
Create `migrations/0002_<name>.sql`. It runs automatically on the next start.
Migrations are embedded at compile time, so rebuild after adding one.

---

## Troubleshooting

**Docker build fails with `SSL certificate ... self-signed certificate in certificate chain`**
A corporate HTTPS proxy (e.g. Netskope) is intercepting traffic from containers.
Put the proxy's root CA (`*.crt`, PEM) in `certs/`; see [certs/README.md](certs/README.md).
SSL verification stays on.

**`curl localhost:3000` returns old or unexpected responses**
Another process is probably using port 3000 (e.g. a leftover `cargo run`).
Check with `ss -ltnp | grep 3000`.

**App exits with `missing required env var DATABASE_URL`**
Running on the host without a `.env` file. Run `cp .env.example .env`.

---

## Learning roadmap / next exercises
See [architecture.md §11](architecture.md): rate limiting, negative caching,
batched click counting, counter-based codes, analytics, and integration tests.
