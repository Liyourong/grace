use std::time::Duration;
use serde::{Serialize, de::DeserializeOwned};

pub enum FunctionError { Unavailable, Upstream }

pub async fn call<T: DeserializeOwned>(payload: &impl Serialize, timeout_secs: u64) -> Result<T, FunctionError> {
    let url = std::env::var("GRACE_EXPLAIN_URL").map_err(|_| FunctionError::Unavailable)?;
    let token = std::env::var("GRACE_EXPLAIN_TOKEN").map_err(|_| FunctionError::Unavailable)?;
    // Require HTTPS and nonempty configuration. Never include tokens in errors or logs.
    if !url.starts_with("https://") || token.trim().is_empty() {
        return Err(FunctionError::Unavailable);
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build()
        .map_err(|_| FunctionError::Unavailable)?;
    let response = client
        .post(url)
        .header("X-Require-Whisk-Auth", token)
        .json(payload)
        .send()
        .await
        .map_err(|err| {
            if err.is_timeout() {
                eprintln!("AI Function request timed out");
            } else if err.is_connect() {
                eprintln!("AI Function connection failed");
            } else {
                eprintln!("AI Function request failed");
            }

            FunctionError::Upstream
    })?;

    if !response.status().is_success() {
        eprintln!(
        "AI Function returned HTTP {}",
        response.status().as_u16()
        );
        return Err(FunctionError::Upstream);
    }
    response.json().await.map_err(|_| FunctionError::Upstream)
}
