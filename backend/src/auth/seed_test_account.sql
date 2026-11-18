-- Only a newly inserted learner can receive credentials. Never adopt an existing username.
WITH learner AS (
    INSERT INTO users (username, password_hash)
    VALUES ('grace_test', '')
    ON CONFLICT (username) DO NOTHING
    RETURNING id, username
)
INSERT INTO auth_credentials (user_id, username, password_hash)
SELECT id, username, $1 FROM learner
