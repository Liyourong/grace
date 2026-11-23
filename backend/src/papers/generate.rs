use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use super::{rules, save};
use crate::{auth::CurrentUser, cloud, common::{AppError, AppState, Body, Result}};

static GENERATIONS: Semaphore = Semaphore::const_new(2);

#[derive(Deserialize)]
pub struct Request {
    topic: String,
    count: u8,
    #[serde(default)]
    question_types: Vec<rules::Kind>,
}

pub async fn generate(State(s): State<AppState>, user: CurrentUser, Body(req): Body<Request>) -> Result<(StatusCode, Json<Value>)> {
    let topic = req.topic.trim();
    if topic.is_empty() || topic.chars().count() > 300 || !(1..=10).contains(&req.count) {
        return Err(AppError::bad_request("invalid_generation", "Enter a topic of 1–300 characters and choose 1–10 questions"));
    }
    let types = &req.question_types;
    if types.len() > usize::from(req.count) || types.iter().enumerate().any(|(i, t)| types[..i].contains(t)) {
        return Err(AppError::bad_request("invalid_generation", "Select distinct question types and allow at least one question per type"));
    }
    let _slot = GENERATIONS.try_acquire().map_err(|_| AppError::new(
        StatusCode::TOO_MANY_REQUESTS, "generation_busy", "Paper generation is busy. Try again shortly."
    ))?;
    let request = json!({"action":"generate_paper", "topic":topic, "count":req.count, "question_types":types});
    let invalid = || AppError::new(StatusCode::BAD_GATEWAY, "generation_failed", "Could not generate a valid paper. Please try again.");
    let file: Value = cloud::call(&request, 55).await.map_err(|e| match e {
        cloud::FunctionError::Unavailable => AppError::new(StatusCode::SERVICE_UNAVAILABLE, "generation_unavailable", "Paper generation is unavailable"),
        cloud::FunctionError::Upstream => invalid(),
    })?;
    let paper = rules::parse(&file).map_err(|_| invalid())?;
    if paper.questions.len() != usize::from(req.count)
        || (!types.is_empty() && paper.questions.iter().any(|q| !types.contains(&q.kind)))
        || types.iter().any(|t| !paper.questions.iter().any(|q| q.kind == *t))
    {
        return Err(invalid());
    }
    save(&s, user, paper).await
}
