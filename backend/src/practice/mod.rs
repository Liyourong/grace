//! Answering questions, saving where the user is, and finishing a paper.
//!
//! Offers other modules: [`answers`], [`progress`], [`Stats`].
//! Owns tables: `answers`, `paper_progress`.
//! Uses: `papers::get`, `papers::find_question` to look up questions and check ownership.

mod explain;
mod rules;
mod store;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{post, put},
};
use deadpool_postgres::{GenericClient, Object};
use serde::Deserialize;
use serde_json::{Value, json};

pub use rules::{Stats, normalize};
pub use store::{answers, progress};

use crate::{
    auth::CurrentUser,
    common::{AppError, AppState, Body, Result, parse_id},
    papers::{self, QuestionRow},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/papers/{paper_id}/answers/{question_id}", put(save_answer))
        .route("/api/papers/{paper_id}/answers/{question_id}/explain", post(explain_answer))
        .route("/api/papers/{paper_id}/position", put(save_position))
        .route("/api/papers/{paper_id}/finish", post(finish))
}

#[derive(Deserialize)]
struct Versioned<T> {
    expected_revision: Option<i64>,
    #[serde(flatten)]
    data: T,
}

impl<T> Versioned<T> {
    fn revision(&self) -> Result<i64> {
        self.expected_revision.filter(|r| *r >= 0).ok_or_else(|| AppError::new(
            StatusCode::PRECONDITION_REQUIRED, "revision_required", "Refresh the page before saving progress.",
        ))
    }
}

async fn save_answer(
    State(s): State<AppState>,
    user: CurrentUser,
    Path((paper_id, question_id)): Path<(String, String)>,
    Body(body): Body<Versioned<rules::Submission>>,
) -> Result<Json<Value>> {
    let paper_id = parse_id(&paper_id)?;
    let mut db = s.pool.get().await?;
    let q = find_question(&db, paper_id, user.id, &question_id).await?;
    let key = if q.kind.as_str() == "choice" {
        let option_ids = q.options.as_ref()
            .map(|options| options.iter().map(|o| o.id.as_str()).collect())
            .unwrap_or_default();
        rules::GradingKey::Choice {
            option_ids,
            correct_option_id: q.correct_option_id.as_deref(),
        }
    } else {
        rules::GradingKey::Text { accepted_answers: &q.accepted_answers }
    };
    let expected = body.revision()?;
    let (saved, is_correct) = rules::grade(body.data, key)?;
    let tx = db.transaction().await?;
    let revision = store::save_position(&tx, user.id, paper_id, q.id, expected).await?;
    let updated_at = store::save_answer(&tx, user.id, q.id, &saved, is_correct).await?;
    let stats = stats(&tx, paper_id, user.id).await?;
    tx.commit().await?;
    Ok(Json(json!({
        "answer": {
            "saved_answer": saved,
            "is_correct": is_correct,
            "correct_answer": q.correct_answer,
            "explanation": q.explanation,
            "updated_at": updated_at,
        },
        "stats": stats,
        "revision": revision,
    })))
}

/// Explain the signed-in user's last saved answer. Never changes grades or progress.
async fn explain_answer(
    State(s): State<AppState>,
    user: CurrentUser,
    Path((paper_id, question_id)): Path<(String, String)>,
) -> Result<Json<Value>> {
    let paper_id = parse_id(&paper_id)?;
    let db = s.pool.get().await?;
    let q = find_question(&db, paper_id, user.id, &question_id).await?;
    let existing = store::answers(&db, user.id, &[q.id]).await?;
    let saved = existing.get(&q.id).ok_or_else(|| {
        AppError::bad_request("answer_not_saved", "Submit an answer before requesting an explanation")
    })?;
    let payload = explain::payload(&q, &saved.answer, saved.is_correct)?;
    // Do not hold a PostgreSQL pool connection during an external HTTP request.
    drop(db);
    let explanation = explain::request(payload).await?;
    Ok(Json(json!({ "explanation": explanation })))
}

#[derive(Deserialize)]
struct PositionBody {
    question_id: String,
}

async fn save_position(
    State(s): State<AppState>,
    user: CurrentUser,
    Path(paper_id): Path<String>,
    Body(body): Body<Versioned<PositionBody>>,
) -> Result<Json<Value>> {
    let paper_id = parse_id(&paper_id)?;
    let db = s.pool.get().await?;
    let q = find_question(&db, paper_id, user.id, &body.data.question_id).await?;
    let revision = store::save_position(&db, user.id, paper_id, q.id, body.revision()?).await?;
    Ok(Json(json!({ "revision": revision })))
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
    if !papers::valid_id(question_id) {
        return Err(AppError::not_found());
    }
    papers::find_question(db, paper_id, user_id, question_id).await?.ok_or_else(AppError::not_found)
}

/// Results so far on a paper this user owns; 404 otherwise.
async fn stats(db: &impl GenericClient, paper_id: i64, user_id: i64) -> Result<Stats> {
    let paper = papers::get(db, paper_id, user_id).await?.ok_or_else(AppError::not_found)?;
    let saved = store::answers(db, user_id, &paper.question_ids).await?;
    Ok(Stats::of(paper.question_ids.len(), saved.values().map(|a| a.is_correct)))
}
