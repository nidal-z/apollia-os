//! Profile proposal tool: an agent proposes a fact about the person, and the
//! person decides.
//!
//! The profile is read in every conversation it is switched on for, so a
//! wrong fact written unseen would follow the person everywhere. An agent
//! therefore never writes the profile: it calls this tool with one fact, the
//! call is put to the person like any sensitive call, with the fact shown,
//! and only an accepted call reaches [`ProfileProposeTool::run`]. Governance
//! asks for each proposal: no standing consent covers this tool.
//!
//! An accepted fact is kept among the facts of the default profile, with the
//! agent that proposed it as its author. The fields of the profile form
//! (name, role and the others) are not facts: the person fills those.

use std::path::PathBuf;

use apollia_core::SandboxProfile;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::descriptor::{ToolDescriptor, ToolKind};

/// Name of the profile proposal tool.
pub const PROFILE_PROPOSE_TOOL: &str = crate::source_access::PROFILE_PROPOSE_TOOL;

/// Longest key a fact is kept under, in characters.
pub const MAX_KEY_CHARS: usize = 80;

/// Longest fact, in characters.
pub const MAX_FACT_CHARS: usize = 500;

/// Proposes one fact for the profile of the person.
#[derive(Debug, Clone)]
pub struct ProfileProposeTool {
    /// The database of the profiles (`user_memory.db`).
    database: PathBuf,
    /// The agent that proposes, recorded as the author of the fact.
    author: String,
}

/// Errors produced by [`ProfileProposeTool`].
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProfileProposeError {
    /// The key is empty, too long, reserved, or a field of the profile form.
    #[error("the key `{0}` cannot hold a fact: use a short name of your own, not a field of the profile form")]
    InvalidKey(String),
    /// The fact is empty or longer than [`MAX_FACT_CHARS`].
    #[error("the fact must hold between 1 and {MAX_FACT_CHARS} characters")]
    InvalidFact,
    /// The profile could not be opened or written.
    #[error("the profile could not be written: {0}")]
    WriteFailed(String),
}

/// Input of a proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileProposeInput {
    /// Short name of the fact, for example `preferred_report_format`.
    pub key: String,
    /// The fact, as one sentence about the person.
    pub fact: String,
}

/// Output of an accepted proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileProposeOutput {
    /// The key the fact is kept under.
    pub key: String,
    /// Always `true`: the person accepted, and the fact is in the profile.
    pub saved: bool,
}

impl ProfileProposeTool {
    /// A tool writing the profiles of `database` in the name of `author`.
    pub fn new(database: PathBuf, author: String) -> Self {
        Self { database, author }
    }

    /// Keep the accepted fact `input` in the default profile.
    ///
    /// # Errors
    ///
    /// [`ProfileProposeError::InvalidKey`] and
    /// [`ProfileProposeError::InvalidFact`] before anything is opened, and
    /// [`ProfileProposeError::WriteFailed`] when the profile cannot be
    /// written.
    pub async fn run(
        &self,
        input: ProfileProposeInput,
    ) -> Result<ProfileProposeOutput, ProfileProposeError> {
        let key = input.key.trim().to_owned();
        if key.is_empty()
            || key.chars().count() > MAX_KEY_CHARS
            || key.starts_with("__")
            || apollia_memory::profile_schema::is_canonical(&key)
        {
            return Err(ProfileProposeError::InvalidKey(key));
        }
        let fact = input.fact.trim().to_owned();
        if fact.is_empty() || fact.chars().count() > MAX_FACT_CHARS {
            return Err(ProfileProposeError::InvalidFact);
        }
        let database = self.database.clone();
        let author = self.author.clone();
        let stored_key = key.clone();
        let failed = |e: &dyn std::fmt::Display| ProfileProposeError::WriteFailed(e.to_string());
        tokio::task::spawn_blocking(move || {
            let profiles = apollia_memory::user_memory::UserMemoryRepository::new(&database)
                .map_err(|e| failed(&e))?;
            profiles
                .set(
                    &stored_key,
                    &fact,
                    apollia_memory::user_memory::WrittenBy::Agent(author),
                )
                .map_err(|e| failed(&e))
        })
        .await
        .map_err(|e| failed(&e))??;
        Ok(ProfileProposeOutput { key, saved: true })
    }

    /// Returns the [`ToolDescriptor`] for registration in the `ToolRegistry`.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: PROFILE_PROPOSE_TOOL.to_string(),
            version: "1.0.0".to_string(),
            description: "Propose one lasting fact about the person for their profile: a \
                preference, a constraint or a piece of their context that every agent would \
                do well to know. The person sees the fact and accepts or refuses it; nothing \
                is written before. Propose only what the person said themselves, one fact \
                per call, never a secret, and never something that only matters to the task \
                at hand: keep that in your own memory."
                .to_string(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "maxLength": MAX_KEY_CHARS,
                        "description": "Short name of the fact, in snake case, for example `preferred_report_format`."
                    },
                    "fact": {
                        "type": "string",
                        "maxLength": MAX_FACT_CHARS,
                        "description": "The fact, as one sentence about the person, in their language."
                    }
                },
                "required": ["key", "fact"]
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "The key the fact is kept under" },
                    "saved": { "type": "boolean", "description": "Whether the fact is in the profile" }
                },
                "required": ["key", "saved"]
            })),
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec!["profile".to_string(), "write".to_string()],
            family: crate::descriptor::ActionFamily::WriteOrDelete,
            family_source: crate::descriptor::FamilySource::Builtin,
            target_arg: Some("fact".to_string()),
            is_read_only: false,
            risk_score: 0,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_memory::user_memory::{UserMemoryRepository, WrittenBy};

    fn proposal(key: &str, fact: &str) -> ProfileProposeInput {
        ProfileProposeInput {
            key: key.into(),
            fact: fact.into(),
        }
    }

    #[tokio::test]
    async fn test_an_accepted_fact_is_kept_in_the_profile_with_its_author() {
        // GIVEN a profile database and an agent named `quotes`
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("user_memory.db");
        let tool = ProfileProposeTool::new(database.clone(), "quotes".into());

        // WHEN an accepted proposal reaches the tool
        let saved = tool
            .run(proposal(
                " preferred_report_format ",
                "Prefers a table over prose.",
            ))
            .await
            .unwrap();

        // THEN the fact is in the default profile, written by that agent
        assert_eq!(
            saved,
            ProfileProposeOutput {
                key: "preferred_report_format".into(),
                saved: true,
            }
        );
        let entry = UserMemoryRepository::new(&database)
            .unwrap()
            .get("preferred_report_format")
            .unwrap()
            .unwrap();
        assert_eq!(entry.value, "Prefers a table over prose.");
        assert_eq!(entry.written_by, WrittenBy::Agent("quotes".into()));
    }

    #[tokio::test]
    async fn test_a_field_of_the_form_a_reserved_key_and_an_empty_fact_are_refused() {
        // GIVEN a profile whose name the person filled
        let dir = tempfile::tempdir().unwrap();
        let database = dir.path().join("user_memory.db");
        UserMemoryRepository::new(&database)
            .unwrap()
            .set("name", "Ada", WrittenBy::User)
            .unwrap();
        let tool = ProfileProposeTool::new(database.clone(), "quotes".into());

        // WHEN a proposal names a field of the form, a reserved key, then
        // carries no fact
        let field = tool.run(proposal("name", "Grace")).await.unwrap_err();
        let reserved = tool.run(proposal("__phase", "done")).await.unwrap_err();
        let empty = tool.run(proposal("format", "  ")).await.unwrap_err();

        // THEN each is refused and the name the person wrote stands
        assert!(matches!(field, ProfileProposeError::InvalidKey(ref k) if k == "name"));
        assert!(matches!(reserved, ProfileProposeError::InvalidKey(_)));
        assert!(matches!(empty, ProfileProposeError::InvalidFact));
        let name = UserMemoryRepository::new(&database)
            .unwrap()
            .get("name")
            .unwrap()
            .unwrap();
        assert_eq!(name.value, "Ada");
    }

    #[test]
    fn test_the_tool_writes_the_data_of_the_person_and_shows_the_fact() {
        // GIVEN the descriptor of the tool
        // WHEN it is read
        let descriptor = ProfileProposeTool::descriptor();

        // THEN it belongs to the writing family, and the fact is the argument
        // a consent request shows
        assert_eq!(descriptor.name, PROFILE_PROPOSE_TOOL);
        assert_eq!(
            descriptor.family,
            crate::descriptor::ActionFamily::WriteOrDelete
        );
        assert_eq!(descriptor.target_arg.as_deref(), Some("fact"));
    }
}
