//! Sends verification emails through SendGrid, and reads this module's email settings from the environment.
//!
//! Settings (all optional; with no API key or sender, registration answers 503 instead of sending):
//!   SENDGRID_API_KEY     SendGrid API key. A secret: never log it or put it in Git.
//!   SENDGRID_FROM_EMAIL  A sender verified in SendGrid.
//!   DEMO_EMAIL, DEMO_CODE  Together they enable the fixed demo code for that one address. A secret.
//!   SENDGRID_API_URL     Overrides the SendGrid endpoint (for a local test double).

use std::{sync::OnceLock, time::Duration};

use serde_json::json;

const DEFAULT_URL: &str = "https://api.sendgrid.com/v3/mail/send";

struct Settings {
    api_key: Option<String>,
    from: Option<String>,
    demo: Option<(String, String)>,
    url: String,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn settings() -> &'static Settings {
    static SETTINGS: OnceLock<Settings> = OnceLock::new();
    SETTINGS.get_or_init(|| Settings {
        api_key: var("SENDGRID_API_KEY"),
        from: var("SENDGRID_FROM_EMAIL"),
        // The demo email goes through the same normalization as any typed email, so it can be compared directly.
        demo: var("DEMO_EMAIL").and_then(|e| super::rules::email(&e)).zip(var("DEMO_CODE")),
        url: var("SENDGRID_API_URL").unwrap_or_else(|| DEFAULT_URL.to_string()),
    })
}

/// The demo (email, code) pair, only when both are configured.
pub fn demo() -> Option<(&'static str, &'static str)> {
    settings().demo.as_ref().map(|(email, code)| (email.as_str(), code.as_str()))
}

#[derive(Debug)]
pub enum SendError {
    /// No API key or sender is configured.
    NotConfigured,
    /// SendGrid or the network refused or failed; details are logged, never shown to the user.
    Failed,
}

pub async fn send_code(to: &str, code: &str) -> Result<(), SendError> {
    let s = settings();
    let (Some(key), Some(from)) = (&s.api_key, &s.from) else {
        eprintln!("verification email not sent: SENDGRID_API_KEY or SENDGRID_FROM_EMAIL is not set");
        return Err(SendError::NotConfigured);
    };
    let body = json!({
        "personalizations": [{ "to": [{ "email": to }] }],
        "from": { "email": from, "name": "Grace" },
        "subject": "Your Grace verification code",
        "content": [{
            "type": "text/plain",
            "value": format!(
                "Your Grace verification code is {code}.\n\nEnter it on the registration page to finish creating your account. \
                 If you did not try to register, you can ignore this email."
            ),
        }],
    });
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    let client = CLIENT.get_or_init(|| {
        reqwest::Client::builder().timeout(Duration::from_secs(10)).build().expect("http client")
    });
    match client.post(&s.url).bearer_auth(key).json(&body).send().await {
        Ok(r) if r.status().is_success() => Ok(()),
        Ok(r) => {
            let status = r.status();
            let detail: String = r.text().await.unwrap_or_default().chars().take(300).collect();
            eprintln!("SendGrid rejected the verification email: {status} {detail}");
            Err(SendError::Failed)
        }
        Err(e) => {
            // reqwest's Display can include the URL, never the key or body.
            eprintln!("SendGrid request failed: {e}");
            Err(SendError::Failed)
        }
    }
}
