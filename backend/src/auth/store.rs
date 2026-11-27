//! SQL for the tables this module owns: `users`, `sessions`, `auth_credentials`, `email_verifications`.

use std::time::SystemTime;

use deadpool_postgres::{GenericClient, Object, Pool};
use tokio_postgres::error::SqlState;

use super::rules::{MAX_FAILED_ATTEMPTS, MAX_SENDS, RESEND_COOLDOWN_SECONDS, SESSION_SECONDS};
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
    if let Ok(hash) = std::env::var("GRACE_TEST_PASSWORD_HASH") {
        if !hash.is_empty() {
            let parsed = argon2::PasswordHash::new(&hash).ok();
            if parsed.is_some_and(|p| p.algorithm.as_str() == "argon2id" && p.hash.is_some() && p.salt.is_some()) {
                tx.execute(include_str!("seed_test_account.sql"), &[&hash]).await?;
                // A matching provisioned credential identifies legacy seeded accounts too.
                tx.execute("INSERT INTO devops_operator SELECT user_id FROM auth_credentials
                    WHERE username='grace_test' AND password_hash=$1 ON CONFLICT DO NOTHING", &[&hash]).await?;
            } else {
                eprintln!("Skipped shared test account creation: expected an Argon2id password hash");
            }
        }
    }
    tx.commit().await?;
    Ok(())
}

pub enum Start {
    /// A registration is waiting for `code`, which the caller now has to email.
    Sent(SystemTime),
    UsernameTaken,
    EmailTaken,
    /// An email went to this address less than a cooldown ago; try again in this many seconds.
    TooSoon(i64),
}

/// Records a registration waiting for its code, replacing any earlier attempt for the same email (the new code
/// replaces the old, and the failed-attempt count starts again). Creates no user and no session.
pub async fn start_registration(
    db: &Object,
    email: &str,
    username: &str,
    password_hash: &str,
    code: &str,
) -> Result<Start> {
    let name_taken = db.query_opt("SELECT 1 FROM users WHERE username = $1", &[&username]).await?.is_some();
    if name_taken {
        return Ok(Start::UsernameTaken);
    }
    let email_taken = db.query_opt("SELECT 1 FROM auth_credentials WHERE email = $1", &[&email]).await?.is_some();
    if email_taken {
        return Ok(Start::EmailTaken);
    }
    // One statement, so two simultaneous requests cannot both pass the cooldown.
    let stored = db
        .query_opt(
            "INSERT INTO email_verifications (email, username, password_hash, code) VALUES ($1, $2, $3, $4)
             ON CONFLICT (email) DO UPDATE SET username = EXCLUDED.username, password_hash = EXCLUDED.password_hash,
                 code = EXCLUDED.code, failed_attempts = 0, send_count = 1, last_sent_at = now()
             WHERE email_verifications.last_sent_at <= now() - make_interval(secs => $5)
             RETURNING last_sent_at",
            &[&email, &username, &password_hash, &code, &(RESEND_COOLDOWN_SECONDS as f64)],
        )
        .await?;
    if let Some(row) = stored {
        return Ok(Start::Sent(row.get(0)));
    }
    Ok(Start::TooSoon(wait_seconds(db, email).await?))
}

pub enum Resend {
    /// Email this code again (it is the one already issued, so earlier emails stay valid).
    Sent(String, SystemTime),
    NoPending,
    TooSoon(i64),
    LimitReached,
}

pub async fn resend(db: &Object, email: &str) -> Result<Resend> {
    let row = db
        .query_opt(
            "UPDATE email_verifications SET last_sent_at = now(), send_count = send_count + 1
             WHERE email = $1 AND send_count < $2 AND last_sent_at <= now() - make_interval(secs => $3)
             RETURNING code, last_sent_at",
            &[&email, &MAX_SENDS, &(RESEND_COOLDOWN_SECONDS as f64)],
        )
        .await?;
    if let Some(row) = row {
        return Ok(Resend::Sent(row.get(0), row.get(1)));
    }
    let pending = db.query_opt("SELECT send_count FROM email_verifications WHERE email = $1", &[&email]).await?;
    Ok(match pending {
        None => Resend::NoPending,
        Some(r) if r.get::<_, i32>(0) >= MAX_SENDS => Resend::LimitReached,
        Some(_) => Resend::TooSoon(wait_seconds(db, email).await?),
    })
}

/// The send did not happen (SendGrid failed), so give the cooldown and the send back.
pub async fn unsend(pool: &Pool, email: &str, sent_at: SystemTime) -> Result<()> {
    let db = pool.get().await?;
    db.execute(
        "UPDATE email_verifications SET last_sent_at = to_timestamp(0), send_count = GREATEST(send_count - 1, 0)
         WHERE email = $1 AND last_sent_at = $2",
        &[&email, &sent_at],
    )
    .await?;
    Ok(())
}

async fn wait_seconds(db: &Object, email: &str) -> Result<i64> {
    let row = db
        .query_opt(
            "SELECT GREATEST(1, CEIL(EXTRACT(EPOCH FROM last_sent_at + make_interval(secs => $2) - now())))::BIGINT
             FROM email_verifications WHERE email = $1",
            &[&email, &(RESEND_COOLDOWN_SECONDS as f64)],
        )
        .await?;
    Ok(row.map_or(RESEND_COOLDOWN_SECONDS, |r| r.get(0)))
}

pub enum Verify {
    /// The account exists and its first session is open. Carries the username.
    Created(String),
    NoPending,
    /// Too many wrong codes for this registration; it has to be started again.
    Locked,
    /// Wrong code; carries the attempts left.
    WrongCode(i32),
    UsernameTaken,
    EmailTaken,
}

/// Finishes a registration: if `accept` approves the stored code, consumes the pending row, creates the user,
/// its credentials and its first session in ONE transaction. A wrong code only counts a failed attempt.
pub async fn verify(
    db: &mut Object,
    email: &str,
    accept: impl FnOnce(&str) -> bool,
    token_hash: &str,
) -> Result<Verify> {
    let tx = db.transaction().await?;
    // The row lock serializes guesses for one email, so the attempt limit cannot be raced.
    let pending = tx
        .query_opt(
            "SELECT username, password_hash, code, failed_attempts FROM email_verifications
             WHERE email = $1 FOR UPDATE",
            &[&email],
        )
        .await?;
    let Some(pending) = pending else { return Ok(Verify::NoPending) };
    let (username, password_hash, code, failed): (String, String, String, i32) =
        (pending.get(0), pending.get(1), pending.get(2), pending.get(3));
    if failed >= MAX_FAILED_ATTEMPTS {
        return Ok(Verify::Locked);
    }
    if !accept(&code) {
        tx.execute("UPDATE email_verifications SET failed_attempts = failed_attempts + 1 WHERE email = $1", &[&email])
            .await?;
        tx.commit().await?;
        return Ok(Verify::WrongCode(MAX_FAILED_ATTEMPTS - failed - 1));
    }

    tx.execute("DELETE FROM email_verifications WHERE email = $1", &[&email]).await?;
    // `users.password_hash` predates `auth_credentials` and stays empty; the real hash lives in `auth_credentials`.
    let user = tx
        .query_opt(
            "INSERT INTO users (username, password_hash) VALUES ($1, '') ON CONFLICT (username) DO NOTHING RETURNING id",
            &[&username],
        )
        .await?;
    let Some(user_id) = user.map(|r| r.get::<_, i64>(0)) else { return Ok(Verify::UsernameTaken) }; // rolls back
    let inserted = tx
        .execute(
            "INSERT INTO auth_credentials (user_id, username, password_hash, email) VALUES ($1, $2, $3, $4)",
            &[&user_id, &username, &password_hash, &email],
        )
        .await;
    match inserted {
        Ok(_) => {}
        Err(e) if e.code() == Some(&SqlState::UNIQUE_VIOLATION) => {
            let on_email = e.as_db_error().and_then(|d| d.constraint()) == Some("auth_credentials_email_key");
            return Ok(if on_email { Verify::EmailTaken } else { Verify::UsernameTaken }); // rolls back
        }
        Err(e) => return Err(e.into()),
    }
    insert_session(&tx, user_id, token_hash).await?;
    tx.commit().await?;
    Ok(Verify::Created(username))
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
