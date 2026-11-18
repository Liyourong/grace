//! SQL for the tables this module owns: `answers`, `paper_progress`.
//! Callers must already have checked that the user owns the paper (via `papers::get` / `papers::find_question`).

use std::collections::HashMap;

use deadpool_postgres::{GenericClient, Object};

use axum::http::StatusCode;
use crate::common::{AppError, Result};

pub struct Saved {
    pub answer: String,
    pub is_correct: bool,
    pub updated_at: String,
}

pub struct Progress {
    /// Database id of the question the user was last on.
    pub question_id: i64,
    pub last_activity_at: String,
    pub revision: i64,
}

/// The user's latest answer for each of these questions that has one, keyed by question id.
pub async fn answers(db: &impl GenericClient, user_id: i64, question_ids: &[i64]) -> Result<HashMap<i64, Saved>> {
    let rows = db
        .query(
            "SELECT question_id, saved_answer, is_correct, utc_iso(updated_at)
             FROM answers WHERE user_id = $1 AND question_id = ANY($2)",
            &[&user_id, &question_ids],
        )
        .await?;
    Ok(rows
        .iter()
        .map(|r| (r.get(0), Saved { answer: r.get(1), is_correct: r.get(2), updated_at: r.get(3) }))
        .collect())
}

/// Where the user is on each of these papers that they have started, keyed by paper id.
pub async fn progress(db: &impl GenericClient, user_id: i64, paper_ids: &[i64]) -> Result<HashMap<i64, Progress>> {
    let rows = db
        .query(
            "SELECT paper_id, current_question_id, to_char(last_activity_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'), revision
             FROM paper_progress WHERE user_id = $1 AND paper_id = ANY($2)",
            &[&user_id, &paper_ids],
        )
        .await?;
    Ok(rows.iter().map(|r| (r.get(0), Progress { question_id: r.get(1), last_activity_at: r.get(2), revision: r.get(3) })).collect())
}

/// Called inside the transaction that has reserved the next progress revision.
pub async fn save_answer(
    db: &deadpool_postgres::Transaction<'_>,
    user_id: i64,
    question_id: i64,
    answer: &str,
    is_correct: bool,
) -> Result<String> {
    let updated_at = db
        .query_one(
            "INSERT INTO answers (user_id, question_id, saved_answer, is_correct) VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id, question_id)
             DO UPDATE SET saved_answer = EXCLUDED.saved_answer, is_correct = EXCLUDED.is_correct, updated_at = now()
             RETURNING utc_iso(updated_at)",
            &[&user_id, &question_id, &answer, &is_correct],
        )
        .await?
        .get(0);
    Ok(updated_at)
}

pub async fn save_position(
    db: &impl GenericClient, user_id: i64, paper_id: i64, question_id: i64, expected: i64,
) -> Result<i64> {
    // Zero covers both an unopened paper and an existing row from before the migration.
    let row = if expected == 0 {
        db.query_opt(
            "INSERT INTO paper_progress (user_id, paper_id, current_question_id, revision) VALUES ($1,$2,$3,1)
             ON CONFLICT (user_id, paper_id) DO UPDATE SET current_question_id = EXCLUDED.current_question_id,
                 revision = 1, last_activity_at = now() WHERE paper_progress.revision = 0 RETURNING revision",
            &[&user_id, &paper_id, &question_id],
        ).await?
    } else {
        db.query_opt(
            "UPDATE paper_progress SET current_question_id = $3, revision = revision + 1, last_activity_at = now()
             WHERE user_id = $1 AND paper_id = $2 AND revision = $4 RETURNING revision",
            &[&user_id, &paper_id, &question_id, &expected],
        ).await?
    };
    row.map(|r| r.get(0)).ok_or_else(|| AppError::new(
        StatusCode::CONFLICT, "progress_conflict", "Practice progress has changed. Reopen the paper to sync.",
    ))
}

/// Bumps `last_activity_at` if the user has started the paper.
pub async fn touch(db: &Object, user_id: i64, paper_id: i64) -> Result<()> {
    db.execute(
        "UPDATE paper_progress SET last_activity_at = now() WHERE user_id = $1 AND paper_id = $2",
        &[&user_id, &paper_id],
    )
    .await?;
    Ok(())
}
