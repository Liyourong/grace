CREATE TABLE devops_operator (
    user_id bigint PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE
);
CREATE TABLE operation_log (
    seq bigserial PRIMARY KEY,
    request_id text UNIQUE NOT NULL,
    occurred_at timestamptz NOT NULL,
    entry jsonb NOT NULL
);
CREATE INDEX operation_log_time ON operation_log (occurred_at);
