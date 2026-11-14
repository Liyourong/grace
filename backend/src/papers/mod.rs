//! Uploading papers, listing them, and showing one paper. Right now any JSON file is accepted.
//!
//! Offers other modules: [`get`], [`find_question`], [`QuestionRow`].
//! Owns tables: `papers`, `questions`.
//! Uses: `practice::answers`, `practice::progress`, `practice::Stats` for each paper's results.

mod rules;
mod store;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get as get_route,
};
use serde_json::{Value, json};

pub use store::{QuestionRow, find_question, get};

use crate::{
    auth::CurrentUser,
    common::{AppError, AppState, Body, Result, parse_id},
    practice::{self, Stats},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/papers", get_route(list).post(upload))
        .route("/api/papers/{paper_id}", get_route(detail))
}

async fn upload(State(s): State<AppState>, user: CurrentUser, Body(file): Body<Value>) -> Result<(StatusCode, Json<Value>)> {
    let paper = rules::parse(&file);
    let id = store::insert(&mut s.pool.get().await?, user.id, &paper).await?;
    eprintln!("paper {id} uploaded by user {} ({} questions)", user.id, paper.questions.len());
    Ok((
        StatusCode::CREATED,
        Json(json!({ "paper": {
            "id": id.to_string(),
            "title": paper.title,
            "description": paper.description,
            "question_count": paper.questions.len(),
        }})),
    ))
}

async fn list(State(s): State<AppState>, user: CurrentUser) -> Result<Json<Value>> {
    let mut db = s.pool.get().await?;
    // Every user has the built-in paper, created the first time their list is read.
    store::ensure_default(&mut db, user.id, &rules::default_paper()).await?;
    let papers = store::list(&db, user.id).await?;
    let paper_ids: Vec<i64> = papers.iter().map(|p| p.id).collect();
    let question_ids: Vec<i64> = papers.iter().flat_map(|p| p.question_ids.iter().copied()).collect();
    let answers = practice::answers(&db, user.id, &question_ids).await?;
    let progress = practice::progress(&db, user.id, &paper_ids).await?;

    let papers: Vec<Value> = papers
        .iter()
        .map(|p| {
            let results = p.question_ids.iter().filter_map(|q| answers.get(q)).map(|a| a.is_correct);
            json!({
                "id": p.id.to_string(),
                "title": p.title,
                "description": p.description,
                "uploaded_at": p.uploaded_at,
                "last_activity_at": progress.get(&p.id).map(|pr| &pr.last_activity_at),
                "stats": Stats::of(p.question_ids.len(), results),
            })
        })
        .collect();
    Ok(Json(json!({ "papers": papers })))
}

async fn detail(State(s): State<AppState>, user: CurrentUser, Path(paper_id): Path<String>) -> Result<Json<Value>> {
    let paper_id = parse_id(&paper_id)?;
    let db = s.pool.get().await?;
    let paper = store::get(&db, paper_id, user.id).await?.ok_or_else(AppError::not_found)?;
    let questions = store::questions(&db, paper_id, user.id).await?;
    let answers = practice::answers(&db, user.id, &paper.question_ids).await?;
    let progress = practice::progress(&db, user.id, &[paper_id]).await?.remove(&paper_id);

    let current = progress
        .and_then(|p| questions.iter().find(|q| q.id == p.question_id))
        .or(questions.first())
        .map(|q| &q.local_id);
    let views: Vec<Value> = questions
        .iter()
        .map(|q| {
            // Reference answer and explanation are feedback: only sent once the user has answered.
            let answer = answers.get(&q.id).map(|a| {
                json!({
                    "saved_answer": a.answer,
                    "is_correct": a.is_correct,
                    "correct_answer": q.correct_answer,
                    "explanation": q.explanation,
                    "updated_at": a.updated_at,
                })
            });
            json!({
                "id": q.local_id,
                "position": q.position,
                "kind": q.kind.as_str(),
                "prompt": q.prompt,
                "options": q.options,
                "answer": answer,
            })
        })
        .collect();

    Ok(Json(json!({
        "id": paper.id.to_string(),
        "title": paper.title,
        "description": paper.description,
        "current_question_id": current,
        "questions": views,
        "stats": Stats::of(questions.len(), answers.values().map(|a| a.is_correct)),
    })))
}
