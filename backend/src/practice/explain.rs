//! Read-only external explanation of an already-saved practice answer.
//! Function credentials live only in the backend process environment.

use std::time::Duration;

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::{
    common::{AppError, Result},
    papers::QuestionRow,
};

#[derive(Serialize)]
pub struct Payload {
    prompt: String,
    submitted_answer: String,
    reference_answer: String,
    is_correct: bool,
}

#[derive(Deserialize)]
struct FunctionResponse {
    explanation: String,
}

fn field_ok(s: &str) -> bool {
    !s.trim().is_empty() && s.chars().count() <= 2000
}

/// Convert saved option IDs into visible option texts for the Function.
pub fn payload(q: &QuestionRow, saved: &str, is_correct: bool) -> Result<Payload> {
    let (submitted_answer, reference_answer) = if q.kind.as_str() == "choice" {
        let options = q.options.as_ref().ok_or_else(AppError::internal)?;
        let correct_id = q.correct_option_id.as_deref().ok_or_else(AppError::internal)?;
        let selected = options
            .iter()
            .find(|o| o.id == saved)
            .ok_or_else(AppError::internal)?;
        let correct = options
            .iter()
            .find(|o| o.id == correct_id)
            .ok_or_else(AppError::internal)?;
        (selected.text.clone(), correct.text.clone())
    } else {
        (saved.to_owned(), q.correct_answer.clone())
    };

    if ![q.prompt.as_str(), submitted_answer.as_str(), reference_answer.as_str()]
        .into_iter()
        .all(field_ok)
    {
        return Err(AppError::bad_request("explain_unavailable", "This answer cannot be explained"));
    }
    Ok(Payload {
        prompt: q.prompt.clone(),
        submitted_answer,
        reference_answer,
        is_correct,
    })
}

fn unavailable() -> AppError {
    AppError::new(StatusCode::SERVICE_UNAVAILABLE, "explain_unavailable", "Explanation is temporarily unavailable")
}

fn upstream_error() -> AppError {
    AppError::new(StatusCode::BAD_GATEWAY, "explain_failed", "The explanation service could not complete the request")
}

/// Called only by the authenticated /explain route, never automatically on submissions.
pub async fn request(payload: Payload) -> Result<String> {
    let url = std::env::var("GRACE_EXPLAIN_URL").map_err(|_| unavailable())?;
    let token = std::env::var("GRACE_EXPLAIN_TOKEN").map_err(|_| unavailable())?;
    // Require HTTPS and nonempty configuration. Never include tokens in errors or logs.
    if !url.starts_with("https://") || token.trim().is_empty() {
        return Err(unavailable());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|_| unavailable())?;
    let response = client
        .post(url)
        .header("X-Require-Whisk-Auth", token)
        .json(&payload)
        .send()
        .await
        .map_err(|_| upstream_error())?;

    if !response.status().is_success() {
        return Err(upstream_error());
    }
    let result: FunctionResponse = response.json().await.map_err(|_| upstream_error())?;
    if result.explanation.trim().is_empty() || result.explanation.chars().count() > 8000 {
        return Err(upstream_error());
    }
    Ok(result.explanation)
}

#[cfg(test)]
mod tests {
    use super::field_ok;

    #[test]
    fn rejects_blank_and_overlong_function_fields() {
        assert!(!field_ok("   \n"));
        assert!(!field_ok(""));
        assert!(!field_ok(&"a".repeat(2001)));
        assert!(field_ok(&"é".repeat(2000)));
    }
}
