//! SQL for the tables this module owns: `users`, `sessions`.

use deadpool_postgres::Object;

use super::rules::SESSION_SECONDS;
use crate::common::Result;

/// The user behind an unexpired session, as (id, username).
pub async fn session_user(db: &Object, token_hash: &str) -> Result<Option<(i64, String)>> {
    let row = db
        .query_opt(
            "SELECT u.id, u.username FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = $1 AND s.expires_at > now()",
            &[&token_hash],
        )
        .await?;
    Ok(row.map(|r| (r.get(0), r.get(1))))
}

/// The id of the user with this name, creating the user if needed. No password is stored.
pub async fn find_or_create_user(db: &Object, username: &str) -> Result<i64> {
    let row = db
        .query_one(
            "INSERT INTO users (username, password_hash) VALUES ($1, '')
             ON CONFLICT (username) DO UPDATE SET username = EXCLUDED.username RETURNING id",
            &[&username],
        )
        .await?;
    Ok(row.get(0))
}

pub async fn create_session(db: &Object, user_id: i64, token_hash: &str) -> Result<()> {
    db.execute(
        "INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ($1, $2, now() + make_interval(secs => $3))",
        &[&token_hash, &user_id, &(SESSION_SECONDS as f64)],
    )
    .await?;
    Ok(())
}

pub async fn delete_session(db: &Object, token_hash: &str) -> Result<()> {
    db.execute("DELETE FROM sessions WHERE token_hash = $1", &[&token_hash]).await?;
    Ok(())
}
