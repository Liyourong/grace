//! Paper file rules as pure functions. Right now there are none: any JSON is accepted,
//! and whatever fields are missing get defaults. Add format checks here.

use serde::Serialize;
use serde_json::Value;

pub struct PaperFile {
    pub format_version: i32,
    pub title: String,
    pub description: Option<String>,
    pub questions: Vec<Question>,
}

pub struct Question {
    /// Position-based ("1", "2", …) so it is always unique and URL-safe.
    pub id: String,
    pub kind: Kind,
    pub prompt: String,
    pub options: Vec<Choice>,
    pub correct_option_id: Option<String>,
    pub accepted_answers: Vec<String>,
    pub correct_answer: String,
    pub explanation: String,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Choice,
    Translation,
    Sentence,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Choice => "choice",
            Kind::Translation => "translation",
            Kind::Sentence => "sentence",
        }
    }

    /// The inverse of `as_str`; the table's CHECK constraint guarantees one of the three.
    pub fn from_db(s: &str) -> Self {
        match s {
            "choice" => Kind::Choice,
            "translation" => Kind::Translation,
            _ => Kind::Sentence,
        }
    }
}

#[derive(serde::Deserialize, Serialize, Debug)]
pub struct Choice {
    pub id: String,
    pub text: String,
}

/// Reads whatever it can from the file. Never fails.
pub fn parse(file: &Value) -> PaperFile {
    let questions = items(file, "questions")
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let options: Vec<Choice> = items(q, "options")
                .iter()
                .enumerate()
                .map(|(j, o)| Choice { id: text(o, "id").unwrap_or_else(|| (j + 1).to_string()), text: text(o, "text").unwrap_or_default() })
                .collect();
            // The frontend can only show a choice question that has options.
            let kind = match (options.is_empty(), q["type"].as_str()) {
                (false, _) => Kind::Choice,
                (true, Some("sentence")) => Kind::Sentence,
                _ => Kind::Translation,
            };
            Question {
                id: (i + 1).to_string(),
                kind,
                prompt: text(q, "prompt").unwrap_or_default(),
                options,
                correct_option_id: text(q, "correct_option_id"),
                accepted_answers: items(q, "accepted_answers").iter().filter_map(|a| a.as_str().map(clean)).collect(),
                correct_answer: text(q, "correct_answer").unwrap_or_default(),
                explanation: text(q, "explanation").unwrap_or_default(),
            }
        })
        .collect();
    PaperFile {
        format_version: file["format_version"].as_i64().unwrap_or(1) as i32,
        title: text(file, "title").filter(|t| !t.trim().is_empty()).unwrap_or_else(|| "未命名试卷".into()),
        description: text(file, "description").filter(|d| !d.trim().is_empty()),
        questions,
    }
}

/// The paper every user starts with (`default-paper.json`).
pub fn default_paper() -> PaperFile {
    parse(&serde_json::from_str(include_str!("default-paper.json")).expect("default-paper.json is valid JSON"))
}

fn items<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v[key].as_array().map(Vec::as_slice).unwrap_or_default()
}

fn text(v: &Value, key: &str) -> Option<String> {
    v[key].as_str().map(clean)
}

/// PostgreSQL TEXT cannot store NUL.
fn clean(s: &str) -> String {
    s.replace('\0', "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_anything() {
        let p = parse(&json!(5));
        assert_eq!((p.title.as_str(), p.questions.len()), ("未命名试卷", 0));

        let sample = serde_json::from_str(include_str!("../../../examples/french-basics.json")).unwrap();
        let p = parse(&sample);
        let kinds: Vec<_> = p.questions.iter().map(|q| q.kind).collect();
        assert_eq!(kinds, [Kind::Choice, Kind::Translation, Kind::Sentence]);
        assert_eq!(p.questions[2].id, "3");
        assert_eq!(default_paper().questions.len(), 3);

        let p = parse(&json!({ "title": "t\u{0}", "questions": [{ "type": "choice" }, "junk"] }));
        assert_eq!(p.title, "t");
        assert!(p.questions.iter().all(|q| q.kind == Kind::Translation));
    }
}
