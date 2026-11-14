//! Answering questions, saving where the user is, and finishing a paper.
//!
//! Offers other modules: [`answers`], [`progress`], [`Stats`].
//! Owns tables: `answers`, `paper_progress`.
//! Uses: `papers::get`, `papers::find_question` to look up questions and check ownership.

mod rules;
mod store;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{post, put},
};
use deadpool_postgres::Object;
use serde::Deserialize;
use serde_json::{Value, json};

pub use rules::Stats;
pub use store::{answers, progress};

use crate::{
    auth::CurrentUser,
    common::{AppError, AppState, Body, Result, parse_id},
    papers::{self, QuestionRow},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/papers/{paper_id}/answers/{question_id}", put(save_answer))
        .route("/api/papers/{paper_id}/position", put(save_position))
        .route("/api/papers/{paper_id}/finish", post(finish))
}

async fn save_answer(
    State(s): State<AppState>,
    user: CurrentUser,
    Path((paper_id, question_id)): Path<(String, String)>,
    Body(submission): Body<rules::Submission>,
) -> Result<Json<Value>> {
    let paper_id = parse_id(&paper_id)?;
    let mut db = s.pool.get().await?;
    let q = find_question(&db, paper_id, user.id, &question_id).await?;
    let (saved, is_correct) = rules::grade(submission);
    let updated_at = store::save_answer(&mut db, user.id, paper_id, q.id, &saved, is_correct).await?;
    Ok(Json(json!({
        "answer": {
            "saved_answer": saved,
            "is_correct": is_correct,
            "correct_answer": q.correct_answer,
            "explanation": q.explanation,
            "updated_at": updated_at,
        },
        "stats": stats(&db, paper_id, user.id).await?,
    })))
}

#[derive(Deserialize)]
struct PositionBody {
    question_id: String,
}

async fn save_position(
    State(s): State<AppState>,
    user: CurrentUser,
    Path(paper_id): Path<String>,
    Body(body): Body<PositionBody>,
) -> Result<StatusCode> {
    let paper_id = parse_id(&paper_id)?;
    let db = s.pool.get().await?;
    let q = find_question(&db, paper_id, user.id, &body.question_id).await?;
    store::save_position(&db, user.id, paper_id, q.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Finishing does not lock the paper or start a new round; it only reports the latest results.
async fn finish(State(s): State<AppState>, user: CurrentUser, Path(paper_id): Path<String>) -> Result<Json<Value>> {
    let paper_id = parse_id(&paper_id)?;
    let db = s.pool.get().await?;
    let stats = stats(&db, paper_id, user.id).await?;
    store::touch(&db, user.id, paper_id).await?;
    Ok(Json(json!({ "stats": stats })))
}

/// The question, if it is in a paper this user owns; otherwise 404.
async fn find_question(db: &Object, paper_id: i64, user_id: i64, question_id: &str) -> Result<QuestionRow> {
    if question_id.contains('\0') {
        return Err(AppError::not_found());
    }
    papers::find_question(db, paper_id, user_id, question_id).await?.ok_or_else(AppError::not_found)
}

/// Results so far on a paper this user owns; 404 otherwise.
async fn stats(db: &Object, paper_id: i64, user_id: i64) -> Result<Stats> {
    let paper = papers::get(db, paper_id, user_id).await?.ok_or_else(AppError::not_found)?;
    let saved = store::answers(db, user_id, &paper.question_ids).await?;
    Ok(Stats::of(paper.question_ids.len(), saved.values().map(|a| a.is_correct)))
}
