//! What an agent asks a human for when it pauses on the task path, and what
//! the human may answer.
//!
//! A pause used to carry a prompt alone, so the operator received a sentence
//! and the agent received a boolean. The payload types the question: a choice
//! among named propositions, a source, a threshold, a definition, a
//! confirmation, or an approval of a named gesture with its risk.
//!
//! Two properties are held here, and both sides of the pause depend on them.
//!
//! * **A payload is parsed, never trusted.** It arrives from agent code as JSON.
//!   [`HitlPayload::parse`] turns it into the typed form or answers a
//!   [`PayloadError`] naming the field, and the task fails on that error: an
//!   invalid payload is never persisted, listed, or shown to anyone.
//! * **An answer is checked against the pause it answers.**
//!   [`HitlPayload::check_answer`] refuses an id that names no proposition, free
//!   text where none is allowed, and any answer to an approval, whose decision
//!   is carried by `approved` alone.
//!
//! The wire names (`genre`, `libelle`, `portee`, ...) are the product's
//! vocabulary and are kept as the product defined them; they are data, not
//! prose.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The kinds of question an agent can ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionGenre {
    /// Pick one of the propositions.
    Choix,
    /// Name where something comes from.
    Source,
    /// Give a numeric bound.
    Seuil,
    /// Define a term.
    Definition,
    /// Confirm or deny.
    Confirmation,
}

/// How much is at stake in an approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risque {
    /// Easily undone.
    Low,
    /// Noticeable if wrong.
    Medium,
    /// Hard or impossible to undo.
    Critical,
}

/// One answer the human can pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposition {
    /// Stable identifier, what `answer` carries when this proposition is picked.
    pub id: String,
    /// What the human reads.
    pub libelle: String,
}

/// A typed question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Question {
    /// Which kind of question this is.
    pub genre: QuestionGenre,
    /// The question itself.
    pub question: String,
    /// The answers offered, possibly none.
    #[serde(default)]
    pub propositions: Vec<Proposition>,
    /// Whether a free-text answer outside the propositions is accepted.
    #[serde(default)]
    pub autre: bool,
    /// What the answer applies to, when it applies to less than everything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portee: Option<String>,
    /// Whether the answer should be remembered for next time.
    #[serde(default)]
    pub memoire: bool,
}

/// An approval request for one named gesture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Approbation {
    /// Always `"approbation"` on the wire.
    pub genre: ApprobationGenre,
    /// The gesture to approve, e.g. `"crm/delete_contact"`.
    pub geste: String,
    /// How much is at stake.
    pub risque: Risque,
    /// Lines describing what exactly will happen.
    #[serde(default)]
    pub detail: Vec<String>,
    /// How long the request stays valid, in seconds, when it expires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delai: Option<u64>,
}

/// The single `genre` value an approval carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprobationGenre {
    /// `"approbation"`.
    Approbation,
}

/// What a paused task asks for.
///
/// Untagged on purpose: `genre` is a field of both shapes and the values do
/// not overlap, so serde picks the shape from it without a wrapper the product
/// never specified.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HitlPayload {
    /// An approval request.
    Approbation(Approbation),
    /// A question.
    Question(Question),
}

/// Why a payload was refused at the pause.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PayloadError {
    /// The JSON matches neither shape.
    #[error("payload matches neither a question nor an approval: {0}")]
    Shape(String),
    /// A field that must carry text is empty.
    #[error("payload field `{0}` is empty")]
    Empty(&'static str),
    /// Two propositions share an id, so an answer could not say which it picked.
    #[error("payload proposes the id `{0}` twice")]
    DuplicateProposition(String),
    /// A `choix` offers nothing to choose and refuses free text.
    #[error("a `choix` needs at least one proposition, or `autre` set")]
    NothingToChoose,
}

/// Why an answer was refused at resume.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AnswerError {
    /// The pause carries no payload, so there is nothing an answer could match.
    #[error("the pending pause carries no payload, so it takes no answer")]
    NoPayload,
    /// An approval is decided by `approved`; an answer to it is ambiguous.
    #[error("an approval is decided by `approved` and takes no answer")]
    ApprovalTakesNoAnswer,
    /// An approved question needs an answer.
    #[error("the question needs an answer")]
    Missing,
    /// The answer names no proposition and free text is not allowed.
    #[error("`{0}` is not one of the propositions, and free text is not allowed")]
    NotAProposition(String),
    /// The answer has a JSON type this genre does not accept.
    #[error("a `{genre}` does not take {found} as an answer")]
    WrongType {
        /// The genre asked.
        genre: &'static str,
        /// The JSON type received.
        found: &'static str,
    },
}

impl HitlPayload {
    /// Parse and check a payload arriving from agent code.
    ///
    /// # Errors
    ///
    /// [`PayloadError`], naming what is wrong. The caller fails the task on it.
    pub fn parse(raw: &Value) -> Result<Self, PayloadError> {
        let payload: HitlPayload =
            serde_json::from_value(raw.clone()).map_err(|e| PayloadError::Shape(e.to_string()))?;
        payload.check()?;
        Ok(payload)
    }

    fn check(&self) -> Result<(), PayloadError> {
        match self {
            HitlPayload::Approbation(a) => {
                if a.geste.trim().is_empty() {
                    return Err(PayloadError::Empty("geste"));
                }
                Ok(())
            }
            HitlPayload::Question(q) => {
                if q.question.trim().is_empty() {
                    return Err(PayloadError::Empty("question"));
                }
                let mut seen = std::collections::HashSet::new();
                for p in &q.propositions {
                    if p.id.trim().is_empty() {
                        return Err(PayloadError::Empty("propositions[].id"));
                    }
                    if p.libelle.trim().is_empty() {
                        return Err(PayloadError::Empty("propositions[].libelle"));
                    }
                    if !seen.insert(p.id.as_str()) {
                        return Err(PayloadError::DuplicateProposition(p.id.clone()));
                    }
                }
                if q.genre == QuestionGenre::Choix && q.propositions.is_empty() && !q.autre {
                    return Err(PayloadError::NothingToChoose);
                }
                Ok(())
            }
        }
    }

    /// Check an answer submitted at resume against this pause.
    ///
    /// `approved` is the operator's decision. A declined question needs no
    /// answer; an approved one does.
    ///
    /// # Errors
    ///
    /// [`AnswerError`], which the resume route turns into a 422.
    pub fn check_answer(&self, approved: bool, answer: Option<&Value>) -> Result<(), AnswerError> {
        let q = match self {
            HitlPayload::Approbation(_) => {
                return match answer {
                    None | Some(Value::Null) => Ok(()),
                    Some(_) => Err(AnswerError::ApprovalTakesNoAnswer),
                };
            }
            HitlPayload::Question(q) => q,
        };

        let answer = match answer {
            None | Some(Value::Null) => {
                return if approved {
                    Err(AnswerError::Missing)
                } else {
                    Ok(())
                };
            }
            Some(a) => a,
        };

        let names_a_proposition = |s: &str| q.propositions.iter().any(|p| p.id == s);

        match (q.genre, answer) {
            (_, Value::String(s)) if names_a_proposition(s) => Ok(()),
            (QuestionGenre::Seuil, Value::Number(_)) => Ok(()),
            (QuestionGenre::Confirmation, Value::Bool(_)) => Ok(()),
            (_, Value::String(s)) => {
                if q.autre && !s.trim().is_empty() {
                    Ok(())
                } else {
                    Err(AnswerError::NotAProposition(s.clone()))
                }
            }
            (genre, other) => Err(AnswerError::WrongType {
                genre: genre_name(genre),
                found: json_type(other),
            }),
        }
    }
}

fn genre_name(genre: QuestionGenre) -> &'static str {
    match genre {
        QuestionGenre::Choix => "choix",
        QuestionGenre::Source => "source",
        QuestionGenre::Seuil => "seuil",
        QuestionGenre::Definition => "definition",
        QuestionGenre::Confirmation => "confirmation",
    }
}

fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn choix() -> Value {
        json!({
            "genre": "choix",
            "question": "Which list?",
            "propositions": [{"id": "a", "libelle": "List A"}, {"id": "b", "libelle": "List B"}],
            "autre": false
        })
    }

    #[test]
    fn a_question_and_an_approval_parse_to_their_own_shape() {
        // GIVEN one payload of each shape
        let q = choix();
        let a = json!({"genre": "approbation", "geste": "crm/delete", "risque": "critical",
                       "detail": ["id: 42"], "delai": 600});
        // WHEN both are parsed
        // THEN each lands in its own variant
        assert!(matches!(
            HitlPayload::parse(&q),
            Ok(HitlPayload::Question(_))
        ));
        assert!(matches!(
            HitlPayload::parse(&a),
            Ok(HitlPayload::Approbation(_))
        ));
    }

    #[test]
    fn an_unknown_genre_is_refused_rather_than_passed_on() {
        // GIVEN a payload whose genre is none of the six
        let raw = json!({"genre": "sondage", "question": "?"});
        // WHEN it is parsed
        // THEN it is refused as a shape error
        assert!(matches!(
            HitlPayload::parse(&raw),
            Err(PayloadError::Shape(_))
        ));
    }

    #[test]
    fn an_unknown_risk_level_is_refused() {
        // GIVEN an approval whose risk is outside low|medium|critical
        let raw = json!({"genre": "approbation", "geste": "x", "risque": "high"});
        // WHEN it is parsed
        // THEN it is refused
        assert!(HitlPayload::parse(&raw).is_err());
    }

    #[test]
    fn empty_and_duplicate_fields_are_named() {
        // GIVEN three malformed payloads
        let no_question = json!({"genre": "source", "question": "  "});
        let dup = json!({"genre": "choix", "question": "?",
            "propositions": [{"id": "a", "libelle": "A"}, {"id": "a", "libelle": "A2"}]});
        let nothing = json!({"genre": "choix", "question": "?", "propositions": []});
        // WHEN each is parsed
        // THEN each error names what is wrong
        assert_eq!(
            HitlPayload::parse(&no_question),
            Err(PayloadError::Empty("question"))
        );
        assert_eq!(
            HitlPayload::parse(&dup),
            Err(PayloadError::DuplicateProposition("a".into()))
        );
        assert_eq!(
            HitlPayload::parse(&nothing),
            Err(PayloadError::NothingToChoose)
        );
    }

    #[test]
    fn a_choice_answer_must_name_a_proposition_unless_free_text_is_allowed() {
        // GIVEN a choice with two propositions and no free text
        let p = HitlPayload::parse(&choix()).expect("valid");
        // WHEN answers are checked
        // THEN a proposition id passes and anything else is refused
        assert_eq!(p.check_answer(true, Some(&json!("b"))), Ok(()));
        assert_eq!(
            p.check_answer(true, Some(&json!("c"))),
            Err(AnswerError::NotAProposition("c".into()))
        );
        // AND with `autre`, free text passes
        let mut raw = choix();
        raw["autre"] = json!(true);
        let p = HitlPayload::parse(&raw).expect("valid");
        assert_eq!(p.check_answer(true, Some(&json!("list C"))), Ok(()));
    }

    #[test]
    fn a_threshold_takes_a_number_and_a_confirmation_a_boolean() {
        // GIVEN a seuil and a confirmation
        let seuil = HitlPayload::parse(&json!({"genre": "seuil", "question": "Max?"})).unwrap();
        let conf =
            HitlPayload::parse(&json!({"genre": "confirmation", "question": "Sure?"})).unwrap();
        // WHEN typed values are answered
        // THEN each accepts its own type and refuses the other
        assert_eq!(seuil.check_answer(true, Some(&json!(12.5))), Ok(()));
        assert_eq!(conf.check_answer(true, Some(&json!(false))), Ok(()));
        assert!(matches!(
            seuil.check_answer(true, Some(&json!(true))),
            Err(AnswerError::WrongType { genre: "seuil", .. })
        ));
    }

    #[test]
    fn an_approved_question_needs_an_answer_and_a_declined_one_does_not() {
        // GIVEN a question
        let p = HitlPayload::parse(&choix()).unwrap();
        // WHEN resumed without an answer
        // THEN approving is refused and declining is accepted
        assert_eq!(p.check_answer(true, None), Err(AnswerError::Missing));
        assert_eq!(p.check_answer(false, None), Ok(()));
    }

    #[test]
    fn an_approval_is_decided_by_approved_alone() {
        // GIVEN an approval request
        let p = HitlPayload::parse(&json!({"genre": "approbation", "geste": "x", "risque": "low"}))
            .unwrap();
        // WHEN resumed with and without an answer
        // THEN only the bare decision is accepted
        assert_eq!(p.check_answer(true, None), Ok(()));
        assert_eq!(
            p.check_answer(true, Some(&json!("yes"))),
            Err(AnswerError::ApprovalTakesNoAnswer)
        );
    }

    #[test]
    fn a_parsed_payload_serialises_back_to_the_product_vocabulary() {
        // GIVEN an approval parsed from the wire
        let raw = json!({"genre": "approbation", "geste": "crm/delete", "risque": "medium",
                         "detail": ["id: 42"]});
        let p = HitlPayload::parse(&raw).unwrap();
        // WHEN it is serialised for the listing
        let back = serde_json::to_value(&p).unwrap();
        // THEN the keys and values are the ones the product specified
        assert_eq!(back["genre"], "approbation");
        assert_eq!(back["risque"], "medium");
        assert_eq!(back["detail"], json!(["id: 42"]));
    }
}
