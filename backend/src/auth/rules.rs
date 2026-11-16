//! Account rules as pure functions: username and password rules, password hashing and checking, session tokens, cookies.

use std::sync::OnceLock;

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::http::{HeaderMap, header::COOKIE};
use rand_core::RngCore;
use sha2::{Digest, Sha256};

pub const SESSION_SECONDS: i64 = 7 * 24 * 3600;

const USERNAME_LEN: std::ops::RangeInclusive<usize> = 3..=32;
const PASSWORD_LEN: std::ops::RangeInclusive<usize> = 8..=128;

/// The normalized username (ASCII letters lowercased), or `None` unless it is 3-32 characters of `a-z`, `0-9`, `_`.
/// Registration and login both go through this, so `Alice_01` and `alice_01` are the same account.
pub fn username(raw: &str) -> Option<String> {
    let name = raw.to_ascii_lowercase();
    let ok = name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    (ok && USERNAME_LEN.contains(&name.len())).then_some(name)
}

/// Passwords are 8-128 characters (not bytes) and are never trimmed or otherwise altered.
pub fn password_ok(password: &str) -> bool {
    PASSWORD_LEN.contains(&password.chars().count())
}

/// Argon2id hash in PHC string format, with a fresh random salt. Slow on purpose: call from `spawn_blocking`.
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default().hash_password(password.as_bytes(), &salt)?.to_string())
}

/// Whether `password` matches a stored PHC hash. A malformed stored hash counts as a mismatch. Slow: use `spawn_blocking`.
pub fn verify_password(password: &str, stored_hash: &str) -> bool {
    PasswordHash::new(stored_hash)
        .is_ok_and(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
}

/// A valid hash nobody knows the password of. Login verifies against it when the username does not exist,
/// so an unknown username costs the same time as a wrong password.
pub fn dummy_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| hash_password("grace-dummy-password").expect("argon2 hashing"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_is_lowercased() {
        assert_eq!(username("Alice_01").as_deref(), Some("alice_01"));
        assert_eq!(username("ALICE_01"), username("alice_01"));
    }

    #[test]
    fn username_length_bounds() {
        assert_eq!(username("abc").as_deref(), Some("abc"));
        assert_eq!(username("ab"), None);
        assert_eq!(username(""), None);
        assert!(username(&"a".repeat(32)).is_some());
        assert_eq!(username(&"a".repeat(33)), None);
    }

    #[test]
    fn username_rejects_everything_but_ascii_word_characters() {
        for bad in ["a b c", " abc", "abc ", "abc\n", "ab-c", "ab.c", "用户名abc", "ünï_code", "abc\0", "a@b.com"] {
            assert_eq!(username(bad), None, "{bad:?}");
        }
        // 3 characters but 6 bytes: still rejected, and never panics on a char boundary.
        assert_eq!(username("ééé"), None);
    }

    #[test]
    fn password_length_counts_characters() {
        assert!(!password_ok("1234567"));
        assert!(password_ok("12345678"));
        assert!(password_ok(&"x".repeat(128)));
        assert!(!password_ok(&"x".repeat(129)));
        assert!(password_ok("éééééééé")); // 8 characters, 16 bytes
        assert!(password_ok(&"é".repeat(128))); // 128 characters, 256 bytes
        assert!(password_ok("        ")); // spaces are kept
    }

    #[test]
    fn hashes_verify_and_use_fresh_salts() {
        let a = hash_password("Pässwörd 123!").unwrap();
        let b = hash_password("Pässwörd 123!").unwrap();
        assert_ne!(a, b);
        assert!(a.starts_with("$argon2id$"));
        assert!(verify_password("Pässwörd 123!", &a));
        assert!(verify_password("Pässwörd 123!", &b));
    }

    #[test]
    fn password_is_compared_exactly() {
        let h = hash_password("Secret Pass 1!").unwrap();
        for wrong in ["secret pass 1!", "Secret Pass 1! ", " Secret Pass 1!", "Secret  Pass 1!", "Secret Pass 1"] {
            assert!(!verify_password(wrong, &h), "{wrong:?}");
        }
    }

    #[test]
    fn malformed_stored_hash_never_verifies() {
        assert!(!verify_password("whatever1", ""));
        assert!(!verify_password("whatever1", "not a phc string"));
        assert!(!verify_password("grace-dummy-password", "$argon2id$garbage"));
    }

    #[test]
    fn dummy_hash_is_valid_but_unguessable() {
        assert!(PasswordHash::new(dummy_hash()).is_ok());
        assert!(!verify_password("password", dummy_hash()));
    }
}
