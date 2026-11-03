-- See architecture.md §4 for the reasoning behind each choice.

CREATE TABLE urls (
    id          BIGSERIAL    PRIMARY KEY,                -- internal key, never exposed
    code        VARCHAR(32)  NOT NULL,                   -- public short code / alias
    long_url    TEXT         NOT NULL,
    url_hash    CHAR(64)     NOT NULL,                   -- sha256 hex of long_url (fixed size, indexable)
    is_custom   BOOLEAN      NOT NULL DEFAULT FALSE,
    clicks      BIGINT       NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    expires_at  TIMESTAMPTZ  NULL,                       -- NULL = never expires

    -- Unique index on code: fast redirect lookups AND collision protection.
    CONSTRAINT urls_code_key UNIQUE (code)
);

-- Dedupe: at most ONE auto-generated, permanent code per long URL.
-- Partial index → custom aliases and expiring links are not deduplicated.
CREATE UNIQUE INDEX urls_dedupe_idx
    ON urls (url_hash)
    WHERE is_custom = FALSE AND expires_at IS NULL;
