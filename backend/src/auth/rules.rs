//! Account rules as pure functions: username, password and email rules, verification codes, password hashing and
//! checking, session tokens, cookies.

use std::sync::OnceLock;

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::http::{HeaderMap, header::COOKIE};
use rand_core::RngCore;
use sha2::{Digest, Sha256};

pub const SESSION_SECONDS: i64 = 7 * 24 * 3600;

/// Wrong codes allowed per issued code; after that the registration must be started again.
pub const MAX_FAILED_ATTEMPTS: i32 = 5;
/// Minimum gap between two emails to the same address.
pub const RESEND_COOLDOWN_SECONDS: i64 = 60;
/// Emails sent for one registration (the first one plus resends).
pub const MAX_SENDS: i32 = 5;

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

/// The normalized email (trimmed, lowercased), or `None` if it is not a plain ASCII `local@domain.tld` address.
/// Deliberately stricter than RFC 5322: no quoted names, comments or IP-literal domains.
pub fn email(raw: &str) -> Option<String> {
    let email = raw.trim().to_ascii_lowercase();
    if email.len() > 254 || !email.is_ascii() {
        return None;
    }
    let (local, domain) = email.split_once('@')?;
    let local_ok = (1..=64).contains(&local.len())
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && local.bytes().all(|b| b.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(&b));
    let labels: Vec<&str> = domain.split('.').collect();
    let domain_ok = labels.len() >= 2
        && labels.iter().all(|l| {
            (1..=63).contains(&l.len())
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        && labels.last().is_some_and(|tld| tld.len() >= 2);
    (local_ok && domain_ok).then_some(email)
}

/// A random six-digit code, zeros included (`004217`), drawn without modulo bias.
pub fn generate_code() -> String {
    const LIMIT: u32 = 4_294_000_000; // the largest multiple of 1_000_000 that fits in a u32
    loop {
        let n = rand_core::OsRng.next_u32();
        if n < LIMIT {
            return format!("{:06}", n % 1_000_000);
        }
    }
}

/// The code as typed: surrounding whitespace removed, 1-32 printable characters. This is not the code's format
/// (the demo code may be any string); a wrong code is rejected by comparison.
pub fn code(raw: &str) -> Option<String> {
    let code = raw.trim();
    ((1..=32).contains(&code.chars().count()) && !code.chars().any(char::is_control)).then(|| code.to_string())
}

/// Equality that takes the same time wherever the strings differ.
pub fn codes_match(a: &str, b: &str) -> bool {
    let (a, b) = (Sha256::digest(a.as_bytes()), Sha256::digest(b.as_bytes()));
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
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

    #[test]
    fn email_is_normalized() {
        assert_eq!(email("  Alice.Smith+grace@Example.COM ").as_deref(), Some("alice.smith+grace@example.com"));
    }

    #[test]
    fn email_rejects_malformed_addresses() {
        for bad in [
            "", "alice", "alice@", "@example.com", "alice@example", "alice@@example.com", "a@b@example.com",
            "alice@exa mple.com", "alice smith@example.com", ".alice@example.com", "alice.@example.com",
            "al..ice@example.com", "alice@-example.com", "alice@example-.com", "alice@example..com",
            "alice@example.c", "<alice>@example.com", "alice@example.com,bob@example.com", "alice@exämple.com",
            "alice@[127.0.0.1]", "\"alice\"@example.com",
        ] {
            assert_eq!(email(bad), None, "{bad:?}");
        }
        assert_eq!(email(&format!("{}@example.com", "a".repeat(65))), None);
        assert!(email(&format!("{}@example.com", "a".repeat(64))).is_some());
        assert_eq!(email(&format!("a@{}.com", "d".repeat(250))), None); // over 254 bytes
    }

    #[test]
    fn generated_codes_are_six_digits() {
        let codes: Vec<String> = (0..2000).map(|_| generate_code()).collect();
        assert!(codes.iter().all(|c| c.len() == 6 && c.bytes().all(|b| b.is_ascii_digit())));
        assert!(codes.iter().any(|c| c != &codes[0]), "codes should vary");
    }

    #[test]
    fn typed_code_is_trimmed_and_bounded() {
        assert_eq!(code(" 123456\n").as_deref(), Some("123456"));
        assert_eq!(code("12 34"), Some("12 34".into()));
        assert_eq!(code(""), None);
        assert_eq!(code("   "), None);
        assert_eq!(code(&"9".repeat(33)), None);
        assert_eq!(code("12\u{0}34"), None);
    }

    #[test]
    fn codes_match_only_when_equal() {
        assert!(codes_match("004217", "004217"));
        assert!(!codes_match("004217", "4217"));
        assert!(!codes_match("004217", "004218"));
        assert!(!codes_match("", "004217"));
    }
}
