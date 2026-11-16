//! Grading and statistics as pure functions. Right now there is no grading: every answer is correct.
//! Add answer matching here.

use serde::{Deserialize, Serialize};
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

/// Returns (answer to save, is it correct). Right now every answer counts as correct.
pub fn grade(s: Submission) -> (String, bool) {
    let answer = s.option_id.or(s.text).unwrap_or_default().replace('\0', ""); // PostgreSQL TEXT cannot store NUL
    (answer, true)
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

    #[test]
    fn everything_is_correct() {
        let s = Submission { option_id: None, text: Some("anything".into()) };
        assert_eq!(grade(s), ("anything".into(), true));
        let s = Stats::of(3, [true, false]);
        assert_eq!((s.answered, s.correct, s.incorrect, s.unanswered), (2, 1, 1, 1));
    }
}
