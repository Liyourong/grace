//! Account rules as pure functions. Right now there are none: any name logs in, no password is checked.
//! Add username rules, password hashing and checking here.

use axum::http::{HeaderMap, header::COOKIE};
use rand_core::RngCore;
use sha2::{Digest, Sha256};

pub const SESSION_SECONDS: i64 = 7 * 24 * 3600;

/// Whatever was typed becomes the account name; blank means "guest".
pub fn username(raw: &str) -> String {
    let name = raw.replace('\0', ""); // PostgreSQL TEXT cannot store NUL
    if name.trim().is_empty() { "guest".into() } else { name }
}

/// Returns (token for the cookie, hash for the database). Only the hash is stored.
pub fn new_token() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand_core::OsRng.fill_bytes(&mut bytes);
    let token = hex::encode(bytes);
    let hash = hash_token(&token);
    (token, hash)
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub fn session_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|kv| kv.trim().strip_prefix("session="))
        .filter(|t| !t.is_empty())
}

pub fn session_cookie(token: &str, secure: bool) -> String {
    cookie(token, SESSION_SECONDS, secure)
}

pub fn clear_cookie(secure: bool) -> String {
    cookie("", 0, secure)
}

fn cookie(value: &str, max_age: i64, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!("session={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}")
}
