//! Pure answer normalization, grading, and current-answer statistics.

use serde::{Deserialize, Serialize};
use crate::common::{AppError, Result};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

/// Compare written answers without accents, punctuation, case, or extra whitespace.
/// NFD (not NFKD) preserves compatibility distinctions such as ligatures.
pub fn normalize(input: &str) -> String {
    let mut result = String::new();
    let mut pending_space = false;
    for ch in input.nfd().filter(|ch| !is_combining_mark(*ch)).flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            if pending_space && !result.is_empty() {
                result.push(' ');
            }
            result.push(ch);
            pending_space = false;
        } else if ch.is_whitespace() {
            pending_space = true;
        }
        // All other characters, including punctuation, are deleted.
    }
    result
}

/// What the user sent: `option_id` for choice questions, `text` for the others.
#[derive(Deserialize)]
pub struct Submission {
    pub option_id: Option<String>,
    pub text: Option<String>,
}

#[derive(Serialize)]
pub struct Stats {
    pub total: i64,
    pub answered: i64,
    pub correct: i64,
    pub incorrect: i64,
    pub unanswered: i64,
}

impl Stats {
    /// `results` holds one `is_correct` per answered question.
    pub fn of(total: usize, results: impl IntoIterator<Item = bool>) -> Self {
        let (mut answered, mut correct) = (0, 0);
        for ok in results {
            answered += 1;
            correct += ok as i64;
        }
        let total = total as i64;
        Self { total, answered, correct, incorrect: answered - correct, unanswered: total - answered }
    }
}

/// The server-side answer key; never send it with an unanswered paper.
pub enum GradingKey<'a> {
    Choice {
        option_ids: Vec<&'a str>,
        correct_option_id: Option<&'a str>,
    },
    Text {
        accepted_answers: &'a [String],
    },
}

fn invalid(message: &'static str) -> AppError {
    AppError::bad_request("invalid_answer", message)
}

/// Grade before persisting. Returns the ORIGINAL answer and its grade, or a 400 error.
pub fn grade(submission: Submission, key: GradingKey<'_>) -> Result<(String, bool)> {
    match key {
        GradingKey::Choice { option_ids, correct_option_id } => {
            let selected = submission.option_id
                .ok_or_else(|| invalid("选择题需要提交 option_id"))?;
            if !option_ids.iter().any(|id| *id == selected.as_str()) {
                return Err(invalid("该选项不属于本题"));
            }
            let is_correct = Some(selected.as_str()) == correct_option_id;
            Ok((selected, is_correct))
        }
        GradingKey::Text { accepted_answers } => {
            let original = submission.text
                .ok_or_else(|| invalid("需要提交 text"))?;
            if original.contains('\0') || original.chars().count() > 2_000 {
                return Err(invalid("答案不能超过 2000 个字符，也不能包含 NUL 字符"));
            }
            let normalized = normalize(&original);
            if normalized.is_empty() {
                return Err(invalid("答案不能为空"));
            }
            let is_correct = accepted_answers.iter().any(|answer| normalize(answer) == normalized);
            Ok((original, is_correct))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_contract() {
        assert_eq!(normalize("  J'aime   le FRANÇAIS. "), "jaime le francais");
        assert_eq!(normalize("Ça va?"), "ca va");
        assert_eq!(normalize("Ñandú"), "nandu");
        assert_eq!(normalize("Merci !"), normalize("merci"));
        assert_eq!(normalize("?! ..."), "");
        assert_ne!(normalize("le chat"), normalize("chat le"));
        assert_eq!(normalize("peut-être"), "peutetre");
        assert_eq!(normalize("peut être"), "peut etre");
        assert_ne!(normalize("ﬁn"), normalize("fin"));
        assert_ne!(normalize("ＭＥＲＣＩ"), normalize("merci"));
    }

    #[test]
    fn whitespace_is_collapsed_without_inserting_spaces_for_punctuation() {
        assert_eq!(normalize("a\t\n  b"), "a b");
        assert_eq!(normalize("a-b"), "ab");
        assert_eq!(normalize("!a!!b!!"), "ab");
        assert_eq!(normalize(" a!!    b "), "a b");
    }

    fn choice(id: Option<&str>) -> Submission {
        Submission { option_id: id.map(str::to_owned), text: None }
    }

    fn text(input: Option<&str>) -> Submission {
        Submission { option_id: None, text: input.map(str::to_owned) }
    }

    fn choice_key<'a>() -> GradingKey<'a> {
        GradingKey::Choice { option_ids: vec!["a", "b"], correct_option_id: Some("a") }
    }

    fn text_key<'a>(answers: &'a [String]) -> GradingKey<'a> {
        GradingKey::Text { accepted_answers: answers }
    }

    #[test]
    fn grades_choice_by_known_option_ids() {
        assert_eq!(grade(choice(Some("a")), choice_key()).unwrap(), ("a".into(), true));
        assert_eq!(grade(choice(Some("b")), choice_key()).unwrap(), ("b".into(), false));
        assert!(grade(choice(Some("z")), choice_key()).is_err());
        assert!(grade(choice(None), choice_key()).is_err());
        let only_text = Submission { option_id: None, text: Some("a".into()) };
        assert!(grade(only_text, choice_key()).is_err());
    }

    #[test]
    fn grades_text_using_all_accepted_answers_and_preserves_original() {
        let answers = vec!["Bonjour".into(), "Merci !".into()];
        assert_eq!(
            grade(text(Some("  merci!! ")), text_key(&answers)).unwrap(),
            ("  merci!! ".into(), true),
        );
        assert_eq!(
            grade(text(Some("salut")), text_key(&answers)).unwrap(),
            ("salut".into(), false),
        );
    }

    #[test]
    fn rejects_missing_empty_oversized_or_nul_text() {
        let answers = vec!["merci".to_owned()];
        assert!(grade(text(None), text_key(&answers)).is_err());
        assert!(grade(text(Some(" ?! ... ")), text_key(&answers)).is_err());
        assert!(grade(text(Some("ok\0bye")), text_key(&answers)).is_err());
        let boundary = "a".repeat(2_000);
        let over = "a".repeat(2_001);
        assert_eq!(grade(text(Some(&boundary)), text_key(&answers)).unwrap().0, boundary);
        assert!(grade(text(Some(&over)), text_key(&answers)).is_err());
        // char-count boundary: a multibyte Unicode scalar is one Rust char.
        let unicode_boundary = "é".repeat(2_000);
        assert!(grade(text(Some(&unicode_boundary)), text_key(&answers)).is_ok());
    }

    #[test]
    fn stats_count_current_answers_not_submission_history() {
        let s = Stats::of(4, [true, false, true]);
        assert_eq!((s.total, s.answered, s.correct, s.incorrect, s.unanswered), (4, 3, 2, 1, 1));
        let s = Stats::of(3, [true]);
        assert_eq!((s.total, s.answered, s.correct, s.incorrect, s.unanswered), (3, 1, 1, 0, 2));
    }
}
