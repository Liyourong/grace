//! SQL for the tables this module owns: `papers`, `questions`.
//! Every read that starts from a paper filters by owner, so users never see each other's papers.

use deadpool_postgres::{GenericClient, Object};
use tokio_postgres::{Row, types::Json};

use super::rules::{Choice, Kind, PaperFile};
use crate::common::Result;

pub struct PaperRow {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub uploaded_at: String,
    pub is_default: bool,
    pub folder_id: Option<i64>,
    /// Database ids of the paper's questions, in order.
    pub question_ids: Vec<i64>,
}

pub struct QuestionRow {
    pub id: i64,
    pub local_id: String,
    pub position: i32,
    pub kind: Kind,
    pub prompt: String,
    pub options: Option<Vec<Choice>>,
    /// Server-internal grading keys consumed by `practice` (#18); never serialized to clients.
    #[allow(dead_code)]
    pub correct_option_id: Option<String>,
    #[allow(dead_code)]
    pub accepted_answers: Vec<String>,
    pub correct_answer: String,
    pub explanation: String,
}

const PAPER_COLUMNS: &str = "SELECT p.id, p.title, p.description, utc_iso(p.created_at), COALESCE(array_agg(q.id ORDER BY q.position) FILTER (WHERE q.id IS NOT NULL), '{}'), p.folder_id, p.is_default
     FROM papers p LEFT JOIN questions q ON q.paper_id = p.id";

const QUESTION_COLUMNS: &str = "SELECT q.id, q.local_id, q.position, q.kind, q.prompt, q.options, q.correct_option_id, q.accepted_answers, q.correct_answer, q.explanation
     FROM questions q JOIN papers p ON p.id = q.paper_id";

pub async fn insert(db: &mut Object, owner_id: i64, paper: &PaperFile) -> Result<i64> {
    let tx = db.transaction().await?;
    let id = insert_paper(&tx, owner_id, paper, false).await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn deliver_starters(db: &mut Object, owner_id: i64) -> Result<()> {
    let catalog = super::rules::starters();
    let delivered = db.query("SELECT slot FROM starter_deliveries WHERE owner_id=$1", &[&owner_id]).await?;
    let missing: Vec<_> = catalog.iter().filter(|(slot, _)| {
        !delivered.iter().any(|row| row.get::<_, i16>(0) == *slot)
    }).collect();
    if missing.is_empty() { return Ok(()); }

    let tx = db.transaction().await?;
    for (slot, paper) in missing {
        let claimed = tx.execute(
            "INSERT INTO starter_deliveries (owner_id,slot) VALUES ($1,$2) ON CONFLICT DO NOTHING",
            &[&owner_id, slot],
        ).await?;
        if claimed == 1 {
            let id = insert_paper(&tx, owner_id, paper, *slot == 1).await?;
            tx.execute("UPDATE starter_deliveries SET paper_id=$3 WHERE owner_id=$1 AND slot=$2",
                &[&owner_id, slot, &id]).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

async fn insert_paper(tx: &deadpool_postgres::Transaction<'_>, owner_id: i64, paper: &PaperFile, is_default: bool) -> Result<i64> {
    let row = tx.query_opt(
        "INSERT INTO papers (owner_id, format_version, title, description, is_default) VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (owner_id) WHERE is_default DO NOTHING RETURNING id",
        &[&owner_id, &paper.format_version, &paper.title, &paper.description, &is_default],
    ).await?;
    let Some(row) = row else {
        // An older app replica can still issue French Basics during a rolling upgrade.
        return Ok(tx.query_one("SELECT id FROM papers WHERE owner_id=$1 AND is_default", &[&owner_id]).await?.get(0));
    };
    let paper_id: i64 = row.get(0);
    let insert = tx
        .prepare(
            "INSERT INTO questions (paper_id, position, local_id, kind, prompt, options, correct_option_id,
                                    accepted_answers, correct_answer, explanation)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .await?;
    for (i, q) in paper.questions.iter().enumerate() {
        let choice = q.kind == Kind::Choice;
        let options = choice.then(|| Json(&q.options));
        let correct_option = if choice { q.correct_option_id.as_deref() } else { None };
        let accepted = (!choice).then(|| Json(&q.accepted_answers));
        tx.execute(
            &insert,
            &[
                &paper_id, &(i as i32 + 1), &q.id, &q.kind.as_str(), &q.prompt, &options,
                &correct_option, &accepted, &q.correct_answer, &q.explanation,
            ],
        )
        .await?;
    }
    Ok(paper_id)
}

/// The owner's papers, newest first.
pub async fn list(db: &impl GenericClient, owner_id: i64) -> Result<Vec<PaperRow>> {
    let sql = format!("{PAPER_COLUMNS} LEFT JOIN starter_deliveries d ON d.paper_id=p.id WHERE p.owner_id = $1 GROUP BY p.id, d.slot ORDER BY d.slot NULLS FIRST, p.created_at DESC, p.id DESC");
    Ok(db.query(&sql, &[&owner_id]).await?.iter().map(paper_row).collect())
}

/// `None` if the paper does not exist or belongs to someone else.
pub async fn get(db: &impl GenericClient, paper_id: i64, owner_id: i64) -> Result<Option<PaperRow>> {
    let sql = format!("{PAPER_COLUMNS} WHERE p.id = $1 AND p.owner_id = $2 GROUP BY p.id");
    Ok(db.query_opt(&sql, &[&paper_id, &owner_id]).await?.as_ref().map(paper_row))
}

/// All questions of a paper the owner can see, in order.
pub async fn questions(db: &impl GenericClient, paper_id: i64, owner_id: i64) -> Result<Vec<QuestionRow>> {
    let sql = format!("{QUESTION_COLUMNS} WHERE p.id = $1 AND p.owner_id = $2 ORDER BY q.position");
    Ok(db.query(&sql, &[&paper_id, &owner_id]).await?.iter().map(question_row).collect())
}

/// One question by its file id; `None` if it is not in a paper this owner has.
pub async fn find_question(db: &Object, paper_id: i64, owner_id: i64, local_id: &str) -> Result<Option<QuestionRow>> {
    let sql = format!("{QUESTION_COLUMNS} WHERE p.id = $1 AND p.owner_id = $2 AND q.local_id = $3");
    Ok(db.query_opt(&sql, &[&paper_id, &owner_id, &local_id]).await?.as_ref().map(question_row))
}

fn paper_row(r: &Row) -> PaperRow {
    PaperRow {
        id: r.get(0),
        title: r.get(1),
        description: r.get(2),
        uploaded_at: r.get(3),
        question_ids: r.get(4),
        folder_id: r.get(5),
        is_default: r.get(6),
    }
}

fn question_row(r: &Row) -> QuestionRow {
    QuestionRow {
        id: r.get(0),
        local_id: r.get(1),
        position: r.get(2),
        kind: Kind::from_db(r.get(3)),
        prompt: r.get(4),
        options: r.get::<_, Option<Json<Vec<Choice>>>>(5).map(|Json(o)| o),
        correct_option_id: r.get(6),
        accepted_answers: r.get::<_, Option<Json<Vec<String>>>>(7).map(|Json(a)| a).unwrap_or_default(),
        correct_answer: r.get(8),
        explanation: r.get(9),
    }
}
