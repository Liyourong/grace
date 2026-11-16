-- Password credentials, one row per user. Run at start-up by `auth::init`, so every statement must be idempotent.
-- The CHECK mirrors `rules::username`: usernames are stored already normalized (lowercase a-z, 0-9, _; 3-32 chars).
CREATE TABLE IF NOT EXISTS auth_credentials (
    user_id BIGINT PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    username TEXT NOT NULL UNIQUE CHECK (username ~ '^[a-z0-9_]{3,32}$'),
    password_hash TEXT NOT NULL CHECK (password_hash <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
