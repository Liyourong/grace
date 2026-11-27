-- This module's own tables. Run at start-up by `auth::init`, so every statement must be idempotent.

-- Password credentials, one row per user.
-- The CHECK mirrors `rules::username`: usernames are stored already normalized (lowercase a-z, 0-9, _; 3-32 chars).
CREATE TABLE IF NOT EXISTS auth_credentials (
    user_id BIGINT PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    username TEXT NOT NULL UNIQUE CHECK (username ~ '^[a-z0-9_]{3,32}$'),
    password_hash TEXT NOT NULL CHECK (password_hash <> ''),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The verified email of accounts registered with email verification. Accounts that predate it have NULL;
-- NULLs do not collide in a unique index, while a real address can belong to only one account.
ALTER TABLE auth_credentials ADD COLUMN IF NOT EXISTS email TEXT CHECK (email = lower(email) AND length(email) <= 254);
CREATE UNIQUE INDEX IF NOT EXISTS auth_credentials_email_key ON auth_credentials (email);

-- A registration waiting for its emailed code, one row per email address. Nothing in `users`, `auth_credentials` or
-- `sessions` exists until the code is accepted; accepting it deletes this row in the same transaction.
-- Only the password hash is kept, never the password.
CREATE TABLE IF NOT EXISTS email_verifications (
    email TEXT PRIMARY KEY CHECK (email = lower(email)),
    username TEXT NOT NULL CHECK (username ~ '^[a-z0-9_]{3,32}$'),
    password_hash TEXT NOT NULL CHECK (password_hash <> ''),
    code TEXT NOT NULL,
    failed_attempts INT NOT NULL DEFAULT 0,
    send_count INT NOT NULL DEFAULT 1,
    last_sent_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
