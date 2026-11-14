//! Accounts and sessions. Right now login and register both let anyone in under the name they typed.
//!
//! Offers other modules: [`CurrentUser`] — put it in a handler's arguments to require a logged-in user.
//! Owns tables: `users`, `sessions`.

mod rules;
mod store;

use axum::{
    Json, Router,
    extract::{FromRequestParts, State},
    http::{HeaderMap, StatusCode, header::SET_COOKIE, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::common::{AppError, AppState, Body, Result};

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

/// The password is sent by the frontend but not used yet.
#[derive(Deserialize)]
struct Credentials {
    #[serde(default)]
    username: String,
}

async fn register(s: State<AppState>, c: Body<Credentials>) -> Result<Response> {
    Ok((StatusCode::CREATED, sign_in(s, c).await?).into_response())
}

async fn login(s: State<AppState>, c: Body<Credentials>) -> Result<Response> {
    sign_in(s, c).await
}

async fn sign_in(State(s): State<AppState>, Body(c): Body<Credentials>) -> Result<Response> {
    let username = rules::username(&c.username);
    let db = s.pool.get().await?;
    let user_id = store::find_or_create_user(&db, &username).await?;
    let (token, token_hash) = rules::new_token();
    store::create_session(&db, user_id, &token_hash).await?;
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
