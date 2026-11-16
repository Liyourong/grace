//! SQL for the tables this module owns: `users`, `sessions`, `auth_credentials`.

use deadpool_postgres::{GenericClient, Object, Pool};
use tokio_postgres::error::SqlState;

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

/// Creates `auth_credentials` (see `credentials.sql`). Safe to run on every start and from several pods at once.
pub async fn init(pool: &Pool) -> Result<()> {
    let mut db = pool.get().await?;
    let tx = db.transaction().await?;
    tx.batch_execute("SELECT pg_advisory_xact_lock(727373)").await?;
    tx.batch_execute(include_str!("credentials.sql")).await?;
    tx.commit().await?;
    Ok(())
}

/// Creates the user, its credentials and its first session in one transaction.
/// `None` if the username is already taken (any capitalization was normalized away before this point).
pub async fn register(db: &mut Object, username: &str, password_hash: &str, token_hash: &str) -> Result<Option<()>> {
    let tx = db.transaction().await?;
    // `users.password_hash` predates `auth_credentials` and stays empty; the real hash lives in `auth_credentials`.
    let user = tx
        .query_opt(
            "INSERT INTO users (username, password_hash) VALUES ($1, '') ON CONFLICT (username) DO NOTHING RETURNING id",
            &[&username],
        )
        .await?;
    let Some(user_id) = user.map(|r| r.get::<_, i64>(0)) else { return Ok(None) };
    match tx
        .execute(
            "INSERT INTO auth_credentials (user_id, username, password_hash) VALUES ($1, $2, $3)",
            &[&user_id, &username, &password_hash],
        )
        .await
    {
        Ok(_) => {}
        Err(e) if e.code() == Some(&SqlState::UNIQUE_VIOLATION) => return Ok(None), // dropping `tx` rolls back
        Err(e) => return Err(e.into()),
    }
    insert_session(&tx, user_id, token_hash).await?;
    tx.commit().await?;
    Ok(Some(()))
}

/// The user id and password hash for a normalized username.
pub async fn credentials(db: &Object, username: &str) -> Result<Option<(i64, String)>> {
    let row = db
        .query_opt("SELECT user_id, password_hash FROM auth_credentials WHERE username = $1", &[&username])
        .await?;
    Ok(row.map(|r| (r.get(0), r.get(1))))
}

/// Drops this user's expired sessions, then creates a fresh one.
pub async fn login_session(db: &mut Object, user_id: i64, token_hash: &str) -> Result<()> {
    let tx = db.transaction().await?;
    tx.execute("DELETE FROM sessions WHERE user_id = $1 AND expires_at <= now()", &[&user_id]).await?;
    insert_session(&tx, user_id, token_hash).await?;
    tx.commit().await?;
    Ok(())
}

async fn insert_session(db: &impl GenericClient, user_id: i64, token_hash: &str) -> Result<()> {
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
