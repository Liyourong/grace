CREATE TABLE users (
    id BIGSERIAL PRIMARY KEY,
    username TEXT NOT NULL UNIQUE CHECK (username ~ '^[a-z0-9_]{3,32}$'),
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX sessions_user_id ON sessions (user_id);

CREATE TABLE papers (
    id BIGSERIAL PRIMARY KEY,
    owner_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    format_version INT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX papers_owner ON papers (owner_id, created_at DESC, id DESC);

CREATE TABLE questions (
    id BIGSERIAL PRIMARY KEY,
    paper_id BIGINT NOT NULL REFERENCES papers (id) ON DELETE CASCADE,
    position INT NOT NULL CHECK (position >= 1),
    local_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('choice', 'translation', 'sentence')),
    prompt TEXT NOT NULL,
    options JSONB,
    correct_option_id TEXT,
    accepted_answers JSONB,
    correct_answer TEXT NOT NULL,
    explanation TEXT NOT NULL,
    UNIQUE (paper_id, position),
    UNIQUE (paper_id, local_id),
    -- lets paper_progress prove its current question belongs to the same paper
    UNIQUE (id, paper_id)
);

CREATE TABLE answers (
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    question_id BIGINT NOT NULL REFERENCES questions (id) ON DELETE CASCADE,
    saved_answer TEXT NOT NULL,
    is_correct BOOLEAN NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, question_id)
);
CREATE INDEX answers_question ON answers (question_id);

CREATE TABLE paper_progress (
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    paper_id BIGINT NOT NULL REFERENCES papers (id) ON DELETE CASCADE,
    current_question_id BIGINT NOT NULL,
    last_activity_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, paper_id),
    FOREIGN KEY (current_question_id, paper_id) REFERENCES questions (id, paper_id) ON DELETE CASCADE
);

CREATE FUNCTION utc_iso(t TIMESTAMPTZ) RETURNS TEXT LANGUAGE SQL STABLE AS
$$ SELECT to_char(t AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') $$;
