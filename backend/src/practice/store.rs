//! SQL for the tables this module owns: `answers`, `paper_progress`.
//! Callers must already have checked that the user owns the paper (via `papers::get` / `papers::find_question`).

use std::collections::HashMap;

use deadpool_postgres::{GenericClient, Object};

use crate::common::Result;

pub struct Saved {
    pub answer: String,
    pub is_correct: bool,
    pub updated_at: String,
}

pub struct Progress {
    /// Database id of the question the user was last on.
    pub question_id: i64,
    pub last_activity_at: String,
}

/// The user's latest answer for each of these questions that has one, keyed by question id.
pub async fn answers(db: &Object, user_id: i64, question_ids: &[i64]) -> Result<HashMap<i64, Saved>> {
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
pub async fn progress(db: &Object, user_id: i64, paper_ids: &[i64]) -> Result<HashMap<i64, Progress>> {
    let rows = db
        .query(
            "SELECT paper_id, current_question_id, utc_iso(last_activity_at)
             FROM paper_progress WHERE user_id = $1 AND paper_id = ANY($2)",
            &[&user_id, &paper_ids],
        )
        .await?;
    Ok(rows.iter().map(|r| (r.get(0), Progress { question_id: r.get(1), last_activity_at: r.get(2) })).collect())
}

/// Saves the latest answer and moves progress to that question, together. Returns `updated_at`.
pub async fn save_answer(
    db: &mut Object,
    user_id: i64,
    paper_id: i64,
    question_id: i64,
    answer: &str,
    is_correct: bool,
) -> Result<String> {
    let tx = db.transaction().await?;
    let updated_at = tx
        .query_one(
            "INSERT INTO answers (user_id, question_id, saved_answer, is_correct) VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id, question_id)
             DO UPDATE SET saved_answer = EXCLUDED.saved_answer, is_correct = EXCLUDED.is_correct, updated_at = now()
             RETURNING utc_iso(updated_at)",
            &[&user_id, &question_id, &answer, &is_correct],
        )
        .await?
        .get(0);
    save_position(&tx, user_id, paper_id, question_id).await?;
    tx.commit().await?;
    Ok(updated_at)
}

pub async fn save_position(db: &impl GenericClient, user_id: i64, paper_id: i64, question_id: i64) -> Result<()> {
    db.execute(
        "INSERT INTO paper_progress (user_id, paper_id, current_question_id) VALUES ($1, $2, $3)
         ON CONFLICT (user_id, paper_id)
         DO UPDATE SET current_question_id = EXCLUDED.current_question_id, last_activity_at = now()",
        &[&user_id, &paper_id, &question_id],
    )
    .await?;
    Ok(())
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
