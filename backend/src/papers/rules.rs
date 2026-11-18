//! Paper file validation and parsing rules as pure functions.

use std::collections::HashSet;

use serde::Serialize;
use serde_json::{Map, Value};

const MAX_QUESTIONS: usize = 200;
const MAX_TITLE_CHARS: usize = 200;
const MAX_TEXT_CHARS: usize = 2_000;
const MAX_ID_CHARS: usize = 64;
const MIN_OPTIONS: usize = 2;
const MAX_OPTIONS: usize = 10;
const MIN_ACCEPTED_ANSWERS: usize = 1;
const MAX_ACCEPTED_ANSWERS: usize = 20;

#[derive(Debug)]
pub struct PaperFile {
    pub format_version: i32,
    pub title: String,
    pub description: Option<String>,
    pub questions: Vec<Question>,
}

#[derive(Debug)]
pub struct Question {
    /// Author-supplied question ID from the file (1–64 ASCII letters, digits, `_`, or `-`).
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

/// Returns true if `id` is 1–64 ASCII letters, digits, `_`, or `-`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_CHARS
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Validates and parses a v1 paper JSON value, collecting all independent errors.
pub fn parse(file: &Value) -> Result<PaperFile, Vec<String>> {
    let Some(root) = file.as_object() else {
        return Err(vec!["The paper file must be a JSON object".into()]);
    };

    let mut errors = Vec::new();
    let format_version = match root.get("format_version") {
        None | Some(Value::Null) => {
            errors.push("format_version is missing".into());
            None
        }
        Some(v) => match v.as_i64() {
            Some(1) => Some(1),
            Some(_) => {
                errors.push("format_version must be 1".into());
                None
            }
            None => {
                errors.push("format_version must be the integer 1".into());
                None
            }
        },
    };

    let title = required_text(root, "title", "title", MAX_TITLE_CHARS, &mut errors);
    let description = optional_text(root, "description", "description", MAX_TEXT_CHARS, &mut errors);

    let mut questions = Vec::new();
    match root.get("questions") {
        None | Some(Value::Null) => errors.push("questions array is missing".into()),
        Some(v) => match v.as_array() {
            None => errors.push("questions must be an array".into()),
            Some(arr) => {
                if arr.is_empty() || arr.len() > MAX_QUESTIONS {
                    errors.push(format!("questions count must be between 1 and {MAX_QUESTIONS}"));
                }
                let mut seen_ids = HashSet::new();
                for (i, item) in arr.iter().enumerate() {
                    if let Some(q) = parse_question(i + 1, item, &mut seen_ids, &mut errors) {
                        questions.push(q);
                    }
                }
            }
        },
    }

    if errors.is_empty() {
        Ok(PaperFile {
            format_version: format_version.unwrap(),
            title: title.unwrap(),
            description,
            questions,
        })
    } else {
        Err(errors)
    }
}

fn parse_question(
    num: usize,
    raw: &Value,
    seen_ids: &mut HashSet<String>,
    errors: &mut Vec<String>,
) -> Option<Question> {
    let Some(obj) = raw.as_object() else {
        errors.push(format!("Question {num}: must be a JSON object"));
        return None;
    };

    let prefix = format!("Question {num}");
    let id = required_unique_id(obj, &prefix, "duplicates an earlier question", seen_ids, errors);
    let kind = parse_kind(obj, &prefix, errors);
    let prompt = required_text(obj, "prompt", &format!("{prefix}: prompt"), MAX_TEXT_CHARS, errors);
    let correct_answer = required_text(obj, "correct_answer", &format!("{prefix}: correct_answer"), MAX_TEXT_CHARS, errors);
    let explanation = required_text(obj, "explanation", &format!("{prefix}: explanation"), MAX_TEXT_CHARS, errors);

    let (options, correct_option_id, accepted_answers) = match kind {
        Some(Kind::Choice) => {
            let (opts, correct) = parse_choice_fields(obj, &prefix, errors);
            (opts, correct, Vec::new())
        }
        Some(k @ (Kind::Translation | Kind::Sentence)) => {
            let accepted = parse_accepted_answers(obj, &prefix, k, errors);
            (Vec::new(), None, accepted)
        }
        None => (Vec::new(), None, Vec::new()),
    };

    Some(Question {
        id: id?,
        kind: kind?,
        prompt: prompt?,
        options,
        correct_option_id,
        accepted_answers,
        correct_answer: correct_answer?,
        explanation: explanation?,
    })
}

fn reject_disallowed_field(obj: &Map<String, Value>, prefix: &str, kind: Kind, field: &str, errors: &mut Vec<String>) {
    // Reject non-null fields that belong to another question type so malformed hybrid questions fail fast.
    if obj.get(field).is_some_and(|v| !v.is_null()) {
        errors.push(format!("{prefix}: {} questions must not include {field}", kind.as_str()));
    }
}

fn parse_choice_fields(
    obj: &Map<String, Value>,
    prefix: &str,
    errors: &mut Vec<String>,
) -> (Vec<Choice>, Option<String>) {
    reject_disallowed_field(obj, prefix, Kind::Choice, "accepted_answers", errors);

    let mut options = Vec::new();
    let mut seen_option_ids = HashSet::new();
    let mut has_options_array = false;

    match obj.get("options") {
        None | Some(Value::Null) => errors.push(format!("{prefix}: options is missing")),
        Some(v) => match v.as_array() {
            None => errors.push(format!("{prefix}: options must be an array")),
            Some(arr) => {
                has_options_array = true;
                if !(MIN_OPTIONS..=MAX_OPTIONS).contains(&arr.len()) {
                    errors.push(format!("{prefix}: options count must be between {MIN_OPTIONS} and {MAX_OPTIONS}"));
                }
                for (j, raw_opt) in arr.iter().enumerate() {
                    let opt_prefix = format!("{prefix}, option {}", j + 1);
                    let Some(opt_obj) = raw_opt.as_object() else {
                        errors.push(format!("{opt_prefix}: must be a JSON object"));
                        continue;
                    };
                    let opt_id = required_unique_id(opt_obj, &opt_prefix, "duplicates an earlier option", &mut seen_option_ids, errors);
                    let opt_text = required_text(opt_obj, "text", &format!("{opt_prefix}: text"), MAX_TEXT_CHARS, errors);
                    if let (Some(id), Some(text)) = (opt_id, opt_text) {
                        options.push(Choice { id, text });
                    }
                }
            }
        },
    }

    let correct_option_id = match obj.get("correct_option_id") {
        None | Some(Value::Null) => {
            errors.push(format!("{prefix}: correct_option_id is missing"));
            None
        }
        Some(v) => check_nonblank_text(v, &format!("{prefix}: correct_option_id"), MAX_ID_CHARS, errors).and_then(|s| {
            if has_options_array && !seen_option_ids.contains(&s) {
                errors.push(format!("{prefix}: correct_option_id \"{s}\" is not one of this question's options"));
                None
            } else {
                Some(s)
            }
        }),
    };

    (options, correct_option_id)
}

fn parse_accepted_answers(
    obj: &Map<String, Value>,
    prefix: &str,
    kind: Kind,
    errors: &mut Vec<String>,
) -> Vec<String> {
    reject_disallowed_field(obj, prefix, kind, "options", errors);
    reject_disallowed_field(obj, prefix, kind, "correct_option_id", errors);

    let mut accepted = Vec::new();
    match obj.get("accepted_answers") {
        None | Some(Value::Null) => errors.push(format!("{prefix}: accepted_answers is missing")),
        Some(v) => match v.as_array() {
            None => errors.push(format!("{prefix}: accepted_answers must be an array")),
            Some(arr) => {
                if !(MIN_ACCEPTED_ANSWERS..=MAX_ACCEPTED_ANSWERS).contains(&arr.len()) {
                    errors.push(format!(
                        "{prefix}: accepted_answers count must be between {MIN_ACCEPTED_ANSWERS} and {MAX_ACCEPTED_ANSWERS}"
                    ));
                }
                for (j, raw_ans) in arr.iter().enumerate() {
                    let label = format!("{prefix}: accepted_answers item {}", j + 1);
                    if let Some(ans) = check_nonblank_text(raw_ans, &label, MAX_TEXT_CHARS, errors) {
                        if crate::practice::normalize(&ans).is_empty() {
                            errors.push(format!("{label} must not be empty after normalization"));
                        } else {
                            accepted.push(ans);
                        }
                    }
                }
            }
        },
    }
    accepted
}

fn parse_kind(obj: &Map<String, Value>, prefix: &str, errors: &mut Vec<String>) -> Option<Kind> {
    match obj.get("type") {
        None | Some(Value::Null) => {
            errors.push(format!("{prefix}: type is missing"));
            None
        }
        Some(v) => match v.as_str() {
            None => {
                errors.push(format!("{prefix}: type must be a string"));
                None
            }
            Some("choice") => Some(Kind::Choice),
            Some("translation") => Some(Kind::Translation),
            Some("sentence") => Some(Kind::Sentence),
            Some(_) => {
                errors.push(format!("{prefix}: type must be choice, translation or sentence"));
                None
            }
        },
    }
}

fn required_unique_id(
    obj: &Map<String, Value>,
    prefix: &str,
    dup_suffix: &str,
    seen: &mut HashSet<String>,
    errors: &mut Vec<String>,
) -> Option<String> {
    match obj.get("id") {
        None | Some(Value::Null) => {
            errors.push(format!("{prefix}: id is missing"));
            None
        }
        Some(v) => {
            let s = check_string(v, &format!("{prefix}: id"), MAX_ID_CHARS, errors)?;
            if !valid_id(s) {
                errors.push(format!("{prefix}: id must be 1–{MAX_ID_CHARS} ASCII letters, digits, _ or -"));
                return None;
            }
            if !seen.insert(s.to_string()) {
                errors.push(format!("{prefix}: id \"{s}\" {dup_suffix}"));
                return None;
            }
            Some(s.to_string())
        }
    }
}

// Slot numbers are permanent delivery IDs shared with the migration.
pub fn starters() -> &'static [(i16, PaperFile)] {
    static CATALOG: std::sync::OnceLock<Vec<(i16, PaperFile)>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| [
        (1, include_str!("default-paper.json")),
        (2, include_str!("starters/basic-sentences.json")),
        (3, include_str!("starters/questions.json")),
        (4, include_str!("starters/simple-subordinate-clauses.json")),
        (5, include_str!("starters/relative-clauses.json")),
        (6, include_str!("starters/complex-sentences.json")),
    ].into_iter().map(|(slot, file)| {
        let paper = parse(&serde_json::from_str(file).expect("starter JSON")).expect("valid starter paper");
        (slot, paper)
    }).collect())
}

fn check_string<'a>(v: &'a Value, label: &str, max_chars: usize, errors: &mut Vec<String>) -> Option<&'a str> {
    let Some(s) = v.as_str() else {
        errors.push(format!("{label} must be a string"));
        return None;
    };
    if s.contains('\0') {
        errors.push(format!("{label} must not contain NUL characters"));
        return None;
    }
    if s.chars().count() > max_chars {
        errors.push(format!("{label} must be at most {max_chars} characters"));
        return None;
    }
    Some(s)
}

fn required_text(
    obj: &Map<String, Value>,
    key: &str,
    label: &str,
    max_chars: usize,
    errors: &mut Vec<String>,
) -> Option<String> {
    match obj.get(key) {
        None | Some(Value::Null) => {
            errors.push(format!("{label} is missing"));
            None
        }
        Some(v) => check_nonblank_text(v, label, max_chars, errors),
    }
}

fn check_nonblank_text(v: &Value, label: &str, max_chars: usize, errors: &mut Vec<String>) -> Option<String> {
    let s = check_string(v, label, max_chars, errors)?;
    if s.trim().is_empty() {
        errors.push(format!("{label} must not be empty"));
        return None;
    }
    // Preserve leading/trailing whitespace as entered by the author.
    Some(s.to_string())
}

fn optional_text(
    obj: &Map<String, Value>,
    key: &str,
    label: &str,
    max_chars: usize,
    errors: &mut Vec<String>,
) -> Option<String> {
    let v = obj.get(key).filter(|v| !v.is_null())?;
    let s = check_string(v, label, max_chars, errors)?;
    (!s.trim().is_empty()).then(|| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_ids() {
        assert!(valid_id("q1"));
        assert!(valid_id("1"));
        assert!(valid_id("A_z-09"));
        assert!(valid_id(&"a".repeat(64)));
        assert!(!valid_id(""));
        assert!(!valid_id(&"a".repeat(65)));
        assert!(!valid_id("q 1"));
        assert!(!valid_id("q/1"));
        assert!(!valid_id("题1"));
        assert!(!valid_id("q\0"));
    }

    #[test]
    fn rejects_invalid_root_and_top_level_fields() {
        assert!(parse(&json!(5)).is_err());
        assert!(parse(&json!([])).is_err());

        let errs = parse(&json!({
            "format_version": 2,
            "title": "   ",
            "description": "ok\0bad",
            "questions": []
        }))
        .unwrap_err();
        assert_eq!(errs.len(), 4);

        let float_ver = parse(&json!({
            "format_version": 1.0,
            "title": "Valid",
            "questions": [{
                "id": "q1",
                "type": "translation",
                "prompt": "p",
                "accepted_answers": ["a"],
                "correct_answer": "a",
                "explanation": "e"
            }]
        }));
        assert!(float_ver.is_err());
    }

    #[test]
    fn grammar_starters_have_ten_four_choice_questions() {
        for (slot, paper) in starters().iter().skip(1) {
            assert_eq!(paper.questions.len(), 10, "slot {slot}");
            for q in &paper.questions {
                assert_eq!(q.kind, Kind::Choice);
                assert_eq!(q.options.len(), 4);
                let texts: std::collections::HashSet<_> = q.options.iter().map(|o| &o.text).collect();
                assert_eq!(texts.len(), 4);
                let answer = q.options.iter().find(|o| Some(&o.id) == q.correct_option_id.as_ref()).unwrap();
                assert_eq!(answer.text, q.correct_answer);
                assert!(!q.explanation.trim().is_empty());
            }
        }
    }

    #[test]
    fn parses_valid_sample_and_default_paper() {
        let sample: Value = serde_json::from_str(include_str!("../../../examples/french-basics.json")).unwrap();
        let p = parse(&sample).expect("french-basics.json should parse");
        assert_eq!(p.format_version, 1);
        assert_eq!(p.title, "French Basics");
        assert_eq!(p.description.as_deref(), Some("A sample paper with all three question types."));
        let ids: Vec<_> = p.questions.iter().map(|q| q.id.as_str()).collect();
        let kinds: Vec<_> = p.questions.iter().map(|q| q.kind).collect();
        assert_eq!(ids, ["q1", "q2", "q3"]);
        assert_eq!(kinds, [Kind::Choice, Kind::Translation, Kind::Sentence]);
        assert_eq!(p.questions[0].options.len(), 2);
        assert_eq!(p.questions[0].options[0].id, "a");
        assert_eq!(p.questions[0].options[0].text, "Bonjour");
        assert_eq!(p.questions[0].correct_option_id.as_deref(), Some("a"));
        assert_eq!(p.questions[1].accepted_answers, ["Merci", "Merci !"]);
        assert_eq!(p.questions[2].accepted_answers, ["J'aime le français."]);

        let def = &starters()[0].1;
        assert_eq!(def.questions.len(), 3);
        assert_eq!(def.questions[0].id, "q1");
    }

    #[test]
    fn validates_text_questions_and_invalid_example_and_boundaries() {
        let invalid: Value = serde_json::from_str(include_str!("../../../examples/invalid-paper.json")).unwrap();
        let errs = parse(&invalid).unwrap_err();
        assert!(errs.len() >= 3, "expected >= 3 errors, got: {errs:?}");

        let twenty_one: Vec<Value> = (1..=21).map(|i| json!(format!("ans {i}"))).collect();
        let errs = parse(&json!({
            "format_version": 1,
            "title": "a".repeat(201),
            "questions": [
                {
                    "id": "q1",
                    "type": "translation",
                    "prompt": "p",
                    "accepted_answers": [],
                    "options": [{ "id": "a", "text": "x" }],
                    "correct_option_id": "a",
                    "correct_answer": "a",
                    "explanation": "e"
                },
                {
                    "id": "q2",
                    "type": "sentence",
                    "prompt": "p",
                    "accepted_answers": twenty_one,
                    "correct_answer": "a",
                    "explanation": "e"
                },
                {
                    "id": "q3",
                    "type": "translation",
                    "prompt": "p",
                    "accepted_answers": ["  ", 42, "ok\0bad", "x".repeat(2001)],
                    "correct_answer": "a",
                    "explanation": "e"
                }
            ]
        }))
        .unwrap_err();
        assert!(errs.iter().any(|e| e.contains("title")));
        assert!(errs.iter().any(|e| e.contains("Question 1: accepted_answers count")));
        assert!(errs.iter().any(|e| e.contains("Question 1: translation questions must not include options")));
        assert!(errs.iter().any(|e| e.contains("Question 1: translation questions must not include correct_option_id")));
        assert!(errs.iter().any(|e| e.contains("Question 2: accepted_answers count")));
        assert!(errs.iter().any(|e| e.contains("Question 3: accepted_answers item 1")));
        assert!(errs.iter().any(|e| e.contains("Question 3: accepted_answers item 2")));
        assert!(errs.iter().any(|e| e.contains("Question 3: accepted_answers item 3")));
        assert!(errs.iter().any(|e| e.contains("Question 3: accepted_answers item 4")));

        // 200-char title passes; whitespace preserved; whitespace-only description -> None; forged owner_id ignored.
        let p = parse(&json!({
            "owner_id": 999,
            "format_version": 1,
            "title": "a".repeat(200),
            "description": "   \n\t ",
            "questions": [{
                "id": "q1",
                "type": "translation",
                "prompt": "  keep spaces  ",
                "accepted_answers": ["  Merci !  "],
                "correct_answer": " Merci. ",
                "explanation": " exp "
            }]
        }))
        .expect("200-char title and ignored owner_id should pass");
        assert_eq!(p.title.chars().count(), 200);
        assert_eq!(p.description, None);
        assert_eq!(p.questions[0].prompt, "  keep spaces  ");
        assert_eq!(p.questions[0].accepted_answers, ["  Merci !  "]);
        assert_eq!(p.questions[0].correct_answer, " Merci. ");
        assert_eq!(p.questions[0].explanation, " exp ");

        // 201 questions fail.
        let too_many: Vec<Value> = (1..=201)
            .map(|i| json!({
                "id": format!("q{i}"),
                "type": "translation",
                "prompt": "p",
                "accepted_answers": ["a"],
                "correct_answer": "a",
                "explanation": "e"
            }))
            .collect();
        let errs = parse(&json!({
            "format_version": 1,
            "title": "Too many",
            "questions": too_many
        }))
        .unwrap_err();
        assert!(errs.iter().any(|e| e.contains("questions count must be between 1 and 200")));

        // Upper boundaries that pass: 200 multi-byte chars title, 200 questions, 10 options, 20 accepted answers.
        let ten_opts: Vec<Value> = (1..=10).map(|i| json!({ "id": format!("o{i}"), "text": "opt" })).collect();
        let twenty_ans: Vec<Value> = (1..=20).map(|i| json!(format!("ans {i}"))).collect();
        let mut two_hundred: Vec<Value> = Vec::with_capacity(200);
        two_hundred.push(json!({
            "id": "q1",
            "type": "choice",
            "prompt": "p",
            "options": ten_opts,
            "correct_option_id": "o10",
            "correct_answer": "opt",
            "explanation": "e"
        }));
        two_hundred.push(json!({
            "id": "q2",
            "type": "sentence",
            "prompt": "p",
            "accepted_answers": twenty_ans,
            "correct_answer": "ans 1",
            "explanation": "e"
        }));
        for i in 3..=200 {
            two_hundred.push(json!({
                "id": format!("q{i}"),
                "type": "translation",
                "prompt": "p",
                "accepted_answers": ["a"],
                "correct_answer": "a",
                "explanation": "e"
            }));
        }
        let max_valid = parse(&json!({
            "format_version": 1,
            "title": "题".repeat(200),
            "questions": two_hundred
        }))
        .expect("upper boundaries (200 chars, 200 questions, 10 options, 20 accepted_answers) should pass");
        assert_eq!(max_valid.questions.len(), 200);
    }

    #[test]
    fn validates_choice_questions_and_rejects_opposite_fields() {
        // 1 option fails; 11 options fail; duplicate option id fails; mismatched correct_option_id fails;
        // accepted_answers on choice fails.
        let eleven: Vec<Value> = (1..=11).map(|i| json!({ "id": format!("o{i}"), "text": "t" })).collect();
        let errs = parse(&json!({
            "format_version": 1,
            "title": "Choice checks",
            "questions": [
                {
                    "id": "q1",
                    "type": "choice",
                    "prompt": "p",
                    "options": [{ "id": "a", "text": "only one" }],
                    "correct_option_id": "z",
                    "accepted_answers": ["nope"],
                    "correct_answer": "a",
                    "explanation": "e"
                },
                {
                    "id": "q2",
                    "type": "choice",
                    "prompt": "p",
                    "options": eleven,
                    "correct_option_id": "o1",
                    "correct_answer": "o1",
                    "explanation": "e"
                },
                {
                    "id": "q3",
                    "type": "choice",
                    "prompt": "p",
                    "options": [
                        { "id": "a", "text": "  " },
                        { "id": "a", "text": "ok" }
                    ],
                    "correct_option_id": "a",
                    "correct_answer": "ok",
                    "explanation": "e"
                }
            ]
        }))
        .unwrap_err();
        assert!(errs.iter().any(|e| e.contains("Question 1: options count must be between 2 and 10")));
        assert!(errs.iter().any(|e| e.contains("Question 1: correct_option_id \"z\" is not one of this question's options")));
        assert!(errs.iter().any(|e| e.contains("Question 1: choice questions must not include accepted_answers")));
        assert!(errs.iter().any(|e| e.contains("Question 2: options count must be between 2 and 10")));
        assert!(errs.iter().any(|e| e.contains("Question 3, option 1: text")));
        assert!(errs.iter().any(|e| e.contains("Question 3, option 2: id \"a\" duplicates an earlier option")));
    }

    #[test]
    fn rejects_invalid_question_common_fields_and_duplicate_ids() {
        let long_nul = format!("{}\0", "x".repeat(2001));
        let errs = parse(&json!({
            "format_version": 1,
            "title": "Test",
            "questions": [
                "not_an_object",
                {
                    "id": "q1",
                    "type": "unknown",
                    "prompt": long_nul,
                    "correct_answer": " ",
                    "explanation": 123
                },
                {
                    "id": "q1",
                    "type": "translation",
                    "prompt": "p",
                    "accepted_answers": ["a"],
                    "correct_answer": "a",
                    "explanation": "e"
                }
            ]
        }))
        .unwrap_err();
        assert_eq!(errs.len(), 6);
        assert!(errs.iter().any(|e| e.contains("Question 2: type")));
        assert!(errs.iter().any(|e| e.contains("Question 2: prompt") && e.contains("NUL")));
        assert!(errs.iter().any(|e| e.contains("Question 3: id \"q1\" duplicates an earlier question")));
    }

    #[test]
    fn covers_remaining_missing_and_wrong_type_rejections() {
        let errs = parse(&json!({})).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("format_version is missing")));
        assert!(errs.iter().any(|e| e.contains("title is missing")));
        assert!(errs.iter().any(|e| e.contains("questions array is missing")));

        let errs = parse(&json!({
            "format_version": "1",
            "title": 42,
            "description": "d".repeat(2001),
            "questions": "bad"
        }))
        .unwrap_err();
        assert!(errs.iter().any(|e| e.contains("format_version must be the integer 1")));
        assert!(errs.iter().any(|e| e.contains("title must be a string")));
        assert!(errs.iter().any(|e| e.contains("description must be at most 2000 characters")));
        assert!(errs.iter().any(|e| e.contains("questions must be an array")));

        let errs = parse(&json!({
            "format_version": 1,
            "title": "t",
            "description": 123,
            "questions": []
        }))
        .unwrap_err();
        assert!(errs.iter().any(|e| e.contains("description must be a string")));

        let errs = parse(&json!({
            "format_version": 1,
            "title": "t",
            "questions": [
                {},
                {
                    "id": "bad id",
                    "type": 9,
                    "prompt": "p",
                    "correct_answer": "c",
                    "explanation": "e"
                },
                {
                    "id": "q\0",
                    "type": "choice",
                    "prompt": "p",
                    "options": "nope",
                    "correct_answer": "c",
                    "explanation": "e"
                },
                {
                    "id": "q4",
                    "type": "choice",
                    "prompt": "p",
                    "options": ["not_obj", { "id": 1, "text": "x".repeat(2001) }],
                    "correct_option_id": "a",
                    "correct_answer": "c",
                    "explanation": "e"
                },
                {
                    "id": "q5",
                    "type": "sentence",
                    "prompt": "p",
                    "accepted_answers": "nope",
                    "correct_answer": "c",
                    "explanation": "e"
                },
                {
                    "id": 7,
                    "type": "translation",
                    "prompt": "p",
                    "correct_answer": "c",
                    "explanation": "e"
                }
            ]
        }))
        .unwrap_err();
        assert!(errs.iter().any(|e| e.contains("Question 1: id is missing")));
        assert!(errs.iter().any(|e| e.contains("Question 1: type is missing")));
        assert!(errs.iter().any(|e| e.contains("Question 1: prompt is missing")));
        assert!(errs.iter().any(|e| e.contains("Question 1: correct_answer is missing")));
        assert!(errs.iter().any(|e| e.contains("Question 1: explanation is missing")));
        assert!(errs.iter().any(|e| e.contains("Question 2: id must be 1–64")));
        assert!(errs.iter().any(|e| e.contains("Question 2: type must be a string")));
        assert!(errs.iter().any(|e| e.contains("Question 3: id must not contain NUL characters")));
        assert!(errs.iter().any(|e| e.contains("Question 3: options must be an array")));
        assert!(errs.iter().any(|e| e.contains("Question 3: correct_option_id is missing")));
        assert!(errs.iter().any(|e| e.contains("Question 4, option 1: must be a JSON object")));
        assert!(errs.iter().any(|e| e.contains("Question 4, option 2: id must be a string")));
        assert!(errs.iter().any(|e| e.contains("Question 4, option 2: text must be at most 2000 characters")));
        assert!(errs.iter().any(|e| e.contains("Question 5: accepted_answers must be an array")));
        assert!(errs.iter().any(|e| e.contains("Question 6: id must be a string")));
        assert!(errs.iter().any(|e| e.contains("Question 6: accepted_answers is missing")));
    }

    #[test]
    fn rejects_accepted_answers_empty_after_normalization() {
        let errs = parse(&json!({
            "format_version": 1,
            "title": "Normalization check",
            "questions": [
                {
                    "id": "q1",
                    "type": "translation",
                    "prompt": "p",
                    "accepted_answers": ["?!", "Merci !", "... —"],
                    "correct_answer": "Merci",
                    "explanation": "e"
                },
                {
                    "id": "q2",
                    "type": "sentence",
                    "prompt": "p",
                    "accepted_answers": ["' - '"],
                    "correct_answer": "a",
                    "explanation": "e"
                }
            ]
        }))
        .unwrap_err();
        assert_eq!(errs.len(), 3, "expected 3 errors, got: {errs:?}");
        assert!(errs[0].contains("Question 1: accepted_answers item 1"));
        assert!(errs[1].contains("Question 1: accepted_answers item 3"));
        assert!(errs[2].contains("Question 2: accepted_answers item 1"));
    }
}
