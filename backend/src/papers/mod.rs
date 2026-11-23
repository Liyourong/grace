//! Uploading papers, listing them, and showing one paper.
//!
//! Offers other modules: [`get`], [`find_question`], [`QuestionRow`], [`valid_id`].
//! Owns tables: `papers`, `questions`.
//! Uses: `practice::answers`, `practice::progress`, `practice::Stats`, `practice::normalize`.

mod folders;
mod generate;
mod rules;
mod store;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get as get_route,
};
use serde_json::{Value, json};

// Exported for `practice` (#18) to validate question IDs in route parameters and position updates.
#[allow(unused_imports)]
pub use rules::valid_id;
pub use store::{QuestionRow, find_question, get};

use crate::{
    auth::CurrentUser,
    common::{AppError, AppState, Body, Result, parse_id},
    practice::{self, Stats},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/papers", get_route(list).post(upload))
        .route("/api/papers/generate", axum::routing::post(generate::generate))
        .route("/api/papers/{paper_id}", get_route(detail).patch(folders::move_paper).delete(folders::delete_paper))
        .route("/api/folders", axum::routing::post(folders::create))
        .route("/api/folders/{folder_id}", axum::routing::patch(folders::rename).delete(folders::delete))
}

async fn upload(State(s): State<AppState>, user: CurrentUser, Body(file): Body<Value>) -> Result<(StatusCode, Json<Value>)> {
    let paper = rules::parse(&file)
        .map_err(|details| AppError::bad_request("invalid_paper_file", "The paper file is not valid").with_details(details))?;
    save(&s, user, paper).await
}

async fn save(s: &AppState, user: CurrentUser, paper: rules::PaperFile) -> Result<(StatusCode, Json<Value>)> {
    let id = store::insert(&mut s.pool.get().await?, user.id, &paper).await?;
    eprintln!("paper {id} saved by user {} ({} questions)", user.id, paper.questions.len());
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
    store::deliver_starters(&mut db, user.id).await?;
    let db = db.build_transaction().isolation_level(tokio_postgres::IsolationLevel::RepeatableRead)
        .read_only(true).start().await?;
    let folders: Vec<Value> = db.query("SELECT id,name FROM paper_folders WHERE owner_id=$1 ORDER BY lower(name),id", &[&user.id])
        .await?.iter().map(|r| json!({"id": r.get::<_,i64>(0).to_string(), "name": r.get::<_,String>(1)})).collect();
    let papers = store::list(&db, user.id).await?;
    let paper_ids: Vec<i64> = papers.iter().map(|p| p.id).collect();
    let question_ids: Vec<i64> = papers.iter().flat_map(|p| p.question_ids.iter().copied()).collect();
    let answers = practice::answers(&db, user.id, &question_ids).await?;
    let progress = practice::progress(&db, user.id, &paper_ids).await?;

    let study_seconds: i64 = db.query_one(
        "SELECT floor(coalesce(sum(upper(span) - lower(span)), 0) / 1000)::bigint
         FROM study_time CROSS JOIN LATERAL unnest(intervals) span WHERE user_id=$1",
        &[&user.id],
    ).await?.get(0);
    let papers: Vec<Value> = papers
        .iter()
        .map(|p| {
            let results = p.question_ids.iter().filter_map(|q| answers.get(q)).map(|a| a.is_correct);
            json!({
                "id": p.id.to_string(),
                "title": p.title,
                "description": p.description,
                "uploaded_at": p.uploaded_at,
                "is_default": p.is_default,
                "current_question_number": progress.get(&p.id)
                    .and_then(|pr| p.question_ids.iter().position(|id| *id == pr.question_id)).unwrap_or(0) + 1,
                "folder_id": p.folder_id.map(|id| id.to_string()),
                "last_activity_at": progress.get(&p.id).map(|pr| &pr.last_activity_at),
                "stats": Stats::of(p.question_ids.len(), results),
            })
        })
        .collect();
    db.commit().await?;
    Ok(Json(json!({ "papers": papers, "folders": folders, "study_seconds": study_seconds })))
}

async fn detail(State(s): State<AppState>, user: CurrentUser, Path(paper_id): Path<String>) -> Result<Json<Value>> {
    let paper_id = parse_id(&paper_id)?;
    let mut conn = s.pool.get().await?;
    let db = conn.build_transaction().isolation_level(tokio_postgres::IsolationLevel::RepeatableRead)
        .read_only(true).start().await?;
    let paper = store::get(&db, paper_id, user.id).await?.ok_or_else(AppError::not_found)?;
    let questions = store::questions(&db, paper_id, user.id).await?;
    let answers = practice::answers(&db, user.id, &paper.question_ids).await?;
    let progress = practice::progress(&db, user.id, &[paper_id]).await?.remove(&paper_id);

    db.commit().await?;
    let revision = progress.as_ref().map_or(0, |p| p.revision);
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
        "revision": revision,
        "questions": views,
        "stats": Stats::of(questions.len(), answers.values().map(|a| a.is_correct)),
    })))
}
