//! Accounts and sessions: registration creates an account, login verifies its password.
//!
//! Offers other modules: [`CurrentUser`] — put it in a handler's arguments to require a logged-in user.
//! Owns tables: `users`, `sessions`, `auth_credentials` (created by [`init`] from `credentials.sql`).

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

use crate::common::{AppError, AppState, Body, Result};

/// Creates this module's own tables. Call once at start-up, after the shared migrations (it references `users`).
pub async fn init(pool: &Pool) -> Result<()> {
    store::init(pool).await
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/auth/register", post(register))
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
        let token = rules::session_token(&parts.headers).ok_or_else(AppError::unauthorized)?;
        let (id, username) = store::session_user(&s.pool.get().await?, &rules::hash_token(token))
            .await?
            .ok_or_else(AppError::unauthorized)?;
        Ok(Self { id, username })
    }
}

#[derive(Deserialize)]
struct Credentials {
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
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

/// Hashing and verifying take tens of milliseconds of CPU, so they run off the async workers.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    spawn_blocking(f).await.map_err(|e| {
        eprintln!("blocking task failed: {e}");
        AppError::internal()
    })
}

async fn register(State(s): State<AppState>, Body(c): Body<Credentials>) -> Result<Response> {
    let username = rules::username(&c.username).ok_or_else(invalid_username)?;
    if !rules::password_ok(&c.password) {
        return Err(invalid_password());
    }
    // Hash before opening the transaction and before taking a connection: no connection is held while hashing.
    let password = c.password;
    let hash = blocking(move || rules::hash_password(&password)).await?.map_err(|e| {
        eprintln!("password hashing failed: {e}");
        AppError::internal()
    })?;

    let (token, token_hash) = rules::new_token();
    let mut db = s.pool.get().await?;
    store::register(&mut db, &username, &hash, &token_hash).await?.ok_or_else(username_taken)?;
    Ok((StatusCode::CREATED, [(SET_COOKIE, rules::session_cookie(&token, s.cookie_secure))], user_json(&username))
        .into_response())
}

async fn login(State(s): State<AppState>, Body(c): Body<Credentials>) -> Result<Response> {
    // A name or password that could never have been registered cannot match any account.
    let (Some(username), true) = (rules::username(&c.username), rules::password_ok(&c.password)) else {
        return Err(invalid_credentials());
    };
    let mut db = s.pool.get().await?;
    let account = store::credentials(&db, &username).await?;
    let (user_id, stored_hash) = match &account {
        Some((id, hash)) => (Some(*id), hash.clone()),
        None => (None, rules::dummy_hash().to_string()), // same work as a real check, so timing does not give names away
    };
    let password = c.password;
    let matches = blocking(move || rules::verify_password(&password, &stored_hash)).await?;
    let Some(user_id) = user_id.filter(|_| matches) else { return Err(invalid_credentials()) };

    let (token, token_hash) = rules::new_token();
    store::login_session(&mut db, user_id, &token_hash).await?;
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

fn user_json(username: &str) -> Json<Value> {
    Json(json!({ "user": { "username": username } }))
}
