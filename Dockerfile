# syntax=docker/dockerfile:1
#
# Multi-stage build ≈ building in a node:XX image, then copying only `dist/` into a
# small runtime image. Rust compiles to ONE native binary, so the final image
# needs no Rust toolchain, no node_modules — just the binary (~10-20 MB of app).

# ---------- Stage 1: build ----------
FROM rust:1.99-slim-trixie AS builder
WORKDIR /app

# Corporate HTTPS proxy support (Netskope/Zscaler re-sign TLS traffic with their own CA).
# Any *.crt in certs/ is added to the trust store, so cargo can verify crates.io
# through the proxy. SSL verification stays ON. With certs/ empty this is a no-op.
COPY certs/ /usr/local/share/ca-certificates/
RUN update-ca-certificates

# 1) Cache dependencies (≈ COPY package.json + bun install before COPY src).
#    Build once with a dummy main.rs so the slow dependency compile is cached
#    and only re-runs when Cargo.toml / Cargo.lock change.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src

# 2) Build the real app. `touch` makes cargo notice main.rs changed.
#    migrations/ must be present at COMPILE time: sqlx::migrate!() embeds the .sql
#    files into the binary, so the runtime image needs no migration files or tools.
COPY build.rs ./
COPY migrations ./migrations
COPY src ./src
RUN touch src/main.rs && cargo build --release --locked

# ---------- Stage 2: runtime ----------
# Same Debian release as the builder so the glibc versions match.
FROM debian:trixie-slim AS runtime

# Run as a non-root user (security best practice).
RUN useradd --system --uid 10001 --no-create-home appuser
USER appuser

COPY --from=builder /app/target/release/url_shortener /usr/local/bin/url_shortener

ENV PORT=3000
EXPOSE 3000

# Exec form (JSON array) so the binary is PID 1 and receives SIGTERM directly
# from `docker stop` — our shutdown_signal() handles it gracefully.
CMD ["url_shortener"]
