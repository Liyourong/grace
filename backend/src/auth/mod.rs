//! Accounts and sessions. Registration needs a code emailed to the new user (SendGrid) before the account and its
//! first session exist; login verifies the password and is unchanged for existing accounts.
//!
//! Offers other modules: [`CurrentUser`] — put it in a handler's arguments to require a logged-in user.
//! Owns tables: `users`, `sessions`, `auth_credentials`, `email_verifications` (the last two created by [`init`]
//! from `credentials.sql`).

mod email;
mod rules;
mod store;

use axum::{
    Json, Router,
    extract::{FromRequestParts, State},
    http::{HeaderMap, StatusCode, header::SET_COOKIE, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use deadpool_postgres::Pool;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::task::spawn_blocking;

use crate::observation::step;
use crate::common::{AppError, AppState, Body, Result};

/// Creates this module's own tables. Call once at start-up, after the shared migrations (it references `users`).
pub async fn init(pool: &Pool) -> Result<()> {
    store::init(pool).await
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/register", post(register))
        .route("/api/auth/register/resend", post(resend))
        .route("/api/auth/verify", post(verify))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
}

/// The logged-in user, read from the session cookie; 401 if there is none.
pub struct CurrentUser {
    pub id: i64,
    pub username: String,
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, s: &AppState) -> Result<Self> {
        step("Session check", async {
            let token = rules::session_token(&parts.headers).ok_or_else(AppError::unauthorized)?;
            let (id, username) = store::session_user(&s.pool.get().await?, &rules::hash_token(token))
                .await?
                .ok_or_else(AppError::unauthorized)?;
            Ok(Self { id, username })
        }).await
    }
}

#[derive(Deserialize)]
struct Credentials {
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
}

#[derive(Deserialize)]
struct Registration {
    #[serde(flatten)]
    credentials: Credentials,
    #[serde(default)]
    email: String,
}

#[derive(Deserialize)]
struct EmailOnly {
    #[serde(default)]
    email: String,
}

#[derive(Deserialize)]
struct Verification {
    #[serde(default)]
    email: String,
    #[serde(default)]
    code: String,
}

fn invalid_username() -> AppError {
    AppError::bad_request("invalid_username", "Username must be 3–32 letters, digits or underscores")
}

fn invalid_password() -> AppError {
    AppError::bad_request("invalid_password", "Password must be 8–128 characters")
}

fn username_taken() -> AppError {
    AppError::new(StatusCode::CONFLICT, "username_taken", "That username is already taken")
}

/// The one answer for an unknown username and a wrong password, so neither can be told from the other.
fn invalid_credentials() -> AppError {
    AppError::new(StatusCode::UNAUTHORIZED, "invalid_credentials", "Incorrect username or password")
}

fn invalid_email() -> AppError {
    AppError::bad_request("invalid_email", "Enter a valid email address")
}

fn email_taken() -> AppError {
    AppError::new(StatusCode::CONFLICT, "email_taken", "An account already exists for that email. Sign in with your username and password.")
}

fn too_soon(wait: i64) -> AppError {
    AppError::new(
        StatusCode::TOO_MANY_REQUESTS,
        "resend_too_soon",
        format!("Please wait {wait} seconds before requesting another code"),
    )
}

fn no_pending() -> AppError {
    AppError::new(StatusCode::NOT_FOUND, "no_pending_registration", "No registration is waiting for that email. Please start again.")
}

/// Hashing and verifying take tens of milliseconds of CPU, so they run off the async workers.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    spawn_blocking(f).await.map_err(|e| {
        eprintln!("blocking task failed: {e}");
        AppError::internal()
    })
}

/// Step 1 of registration: check the fields, remember the registration and email its code. Creates no account and
/// no session; `/api/auth/verify` does that once the code is right.
async fn register(State(s): State<AppState>, Body(c): Body<Registration>) -> Result<Response> {
    let username = rules::username(&c.credentials.username).ok_or_else(invalid_username)?;
    if !rules::password_ok(&c.credentials.password) {
        return Err(invalid_password());
    }
    let email = rules::email(&c.email).ok_or_else(invalid_email)?;
    // Hash before taking a connection: no connection is held while hashing.
    let password = c.credentials.password;
    let hash = blocking(move || rules::hash_password(&password)).await?.map_err(|e| {
        eprintln!("password hashing failed: {e}");
        AppError::internal()
    })?;

    let code = rules::generate_code();
    let sent_at = match store::start_registration(&s.pool.get().await?, &email, &username, &hash, &code).await? {
        store::Start::Sent(sent_at) => sent_at,
        store::Start::UsernameTaken => return Err(username_taken()),
        store::Start::EmailTaken => return Err(email_taken()),
        store::Start::TooSoon(wait) => return Err(too_soon(wait)),
    };
    deliver(&s.pool, &email, &code, sent_at).await?;
    Ok((StatusCode::ACCEPTED, verification_json(&email)).into_response())
}

/// Emails the same code again. It does not invalidate the earlier emails.
async fn resend(State(s): State<AppState>, Body(c): Body<EmailOnly>) -> Result<Response> {
    let email = rules::email(&c.email).ok_or_else(invalid_email)?;
    let (code, sent_at) = match store::resend(&s.pool.get().await?, &email).await? {
        store::Resend::Sent(code, sent_at) => (code, sent_at),
        store::Resend::NoPending => return Err(no_pending()),
        store::Resend::TooSoon(wait) => return Err(too_soon(wait)),
        store::Resend::LimitReached => {
            return Err(AppError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "resend_limit",
                "Too many codes were sent for this registration. Please start again.",
            ));
        }
    };
    deliver(&s.pool, &email, &code, sent_at).await?;
    Ok((StatusCode::ACCEPTED, verification_json(&email)).into_response())
}

/// Sends the code, or skips the send for the demo address, whose code is the configured one. If sending fails the
/// cooldown is given back so the user can retry immediately.
async fn deliver(pool: &Pool, email: &str, code: &str, sent_at: std::time::SystemTime) -> Result<()> {
    if email::demo().is_some_and(|(demo_email, _)| demo_email == email) {
        return Ok(());
    }
    let Err(failure) = email::send_code(email, code).await else { return Ok(()) };
    if let Err(e) = store::unsend(pool, email, sent_at).await {
        eprintln!("could not restore the resend allowance for a failed send: {e:?}");
    }
    Err(match failure {
        email::SendError::NotConfigured => AppError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "email_unavailable",
            "Email verification is temporarily unavailable. Please try again later.",
        ),
        email::SendError::Failed => AppError::new(
            StatusCode::BAD_GATEWAY,
            "email_failed",
            "We couldn't send the verification email. Please try again.",
        ),
    })
}

/// Step 2 of registration: a correct code creates the account and its first session. A wrong code never does.
async fn verify(State(s): State<AppState>, Body(c): Body<Verification>) -> Result<Response> {
    let email = rules::email(&c.email).ok_or_else(invalid_email)?;
    let code = rules::code(&c.code)
        .ok_or_else(|| AppError::bad_request("invalid_code", "Enter the verification code from your email"))?;

    // The demo code is accepted only for the one configured demo email, and only here: it still needs a pending
    // registration (so the user picks their own username and password), is limited by the same attempt counter,
    // and cannot sign in to an existing account. Once that email has an account, registration is refused outright.
    let demo = email::demo().filter(|(demo_email, _)| *demo_email == email);
    let accept = move |stored: &str| match demo {
        Some((_, demo_code)) => rules::codes_match(demo_code, &code),
        None => rules::codes_match(stored, &code),
    };

    let (token, token_hash) = rules::new_token();
    let mut db = s.pool.get().await?;
    match store::verify(&mut db, &email, accept, &token_hash).await? {
        store::Verify::Created(username) => Ok((
            StatusCode::CREATED,
            [(SET_COOKIE, rules::session_cookie(&token, s.cookie_secure))],
            user_json(&username),
        )
            .into_response()),
        store::Verify::NoPending => Err(no_pending()),
        store::Verify::Locked => Err(AppError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "too_many_attempts",
            "Too many incorrect codes. Please start registration again to get a new code.",
        )),
        store::Verify::WrongCode(left) => Err(AppError::bad_request(
            "invalid_code",
            format!("That code is incorrect. {left} attempt{} left.", if left == 1 { "" } else { "s" }),
        )),
        store::Verify::UsernameTaken => Err(username_taken()),
        store::Verify::EmailTaken => Err(email_taken()),
    }
}

async fn login(State(s): State<AppState>, Body(c): Body<Credentials>) -> Result<Response> {
    // A name or password that could never have been registered cannot match any account.
    let (Some(username), true) = (rules::username(&c.username), rules::password_ok(&c.password)) else {
        return Err(invalid_credentials());
    };
    let (mut db, account) = step("Account lookup", async {
        let db = s.pool.get().await?;
        let account = store::credentials(&db, &username).await?;
        Ok((db, account))
    }).await?;
    let (user_id, stored_hash) = match &account {
        Some((id, hash)) => (Some(*id), hash.clone()),
        None => (None, rules::dummy_hash().to_string()), // same work as a real check, so timing does not give names away
    };
    let password = c.password;
    let user_id = step("Password check", async {
        let matches = blocking(move || rules::verify_password(&password, &stored_hash)).await?;
        user_id.filter(|_| matches).ok_or_else(invalid_credentials)
    }).await?;

    let (token, token_hash) = rules::new_token();
    step("Session creation", store::login_session(&mut db, user_id, &token_hash)).await?;
    Ok(([(SET_COOKIE, rules::session_cookie(&token, s.cookie_secure))], user_json(&username)).into_response())
}

async fn logout(State(s): State<AppState>, headers: HeaderMap) -> Result<Response> {
    if let Some(token) = rules::session_token(&headers) {
        store::delete_session(&s.pool.get().await?, &rules::hash_token(token)).await?;
    }
    Ok((StatusCode::NO_CONTENT, [(SET_COOKIE, rules::clear_cookie(s.cookie_secure))]).into_response())
}

async fn me(user: CurrentUser) -> Json<Value> {
    user_json(&user.username)
}

fn verification_json(email: &str) -> Json<Value> {
    Json(json!({ "verification": { "email": email, "resend_after_seconds": rules::RESEND_COOLDOWN_SECONDS } }))
}

fn user_json(username: &str) -> Json<Value> {
    Json(json!({ "user": { "username": username } }))
}
