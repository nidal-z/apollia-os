//! The settings of one conversation.
//!
//! A conversation is tuned without touching its agent: its settings hold for
//! it alone and go with it when it is erased. They live in the `settings`
//! column of `chat_sessions`, read and written through [`SessionSettings`],
//! a closed type that refuses a field it does not know. A field left out is
//! inherited from the agent.
//!
//! The request settings are a fourth level above the three every call
//! resolves (the model, the agent's package, the preset): the preset of the
//! conversation takes the place of the third. The instructions are added to
//! the mission of the agent and never replace it. An access switched off is
//! taken out of the agent's list for the conversation; an access the agent
//! does not hold may be proposed for it, and each of its calls then asks.
//! A conversation may also be limited to a list of accesses: of what the
//! agent holds only they are kept, and the others of the list are proposed.
//! None of the three writes a rule or a consent.
//!
//! The model of the conversation, its profile and the profile switch of the
//! built-in assistant are columns of their own and are not settings.

use apollia_llm::request_defaults::RequestDefaults;
use apollia_llm::ReasoningEffort;
use apollia_oria::context_manager::ContextOverflow;
use serde::{Deserialize, Serialize};

/// Longest notes a conversation keeps, in characters.
pub const SESSION_NOTES_MAX_CHARS: usize = 20_000;

/// Most accesses a conversation switches off, most it proposes, and most it
/// is limited to.
pub const SESSION_ACCESSES_MAX: usize = 256;

/// What a conversation does when it reaches the context limit of its model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextOverflowSetting {
    /// The older middle of the conversation is summarized.
    #[default]
    SummarizeMiddle,
    /// The oldest messages leave the window, without a summary.
    SlidingWindow,
    /// The turn is refused once the limit is reached.
    StopAtLimit,
}

impl From<ContextOverflowSetting> for ContextOverflow {
    fn from(setting: ContextOverflowSetting) -> Self {
        match setting {
            ContextOverflowSetting::SummarizeMiddle => ContextOverflow::SummarizeMiddle,
            ContextOverflowSetting::SlidingWindow => ContextOverflow::SlidingWindow,
            ContextOverflowSetting::StopAtLimit => ContextOverflow::StopAtLimit,
        }
    }
}

/// The settings of one conversation. Every field is optional: one left out
/// is inherited from the agent. An unknown field is refused.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionSettings {
    /// Preset chosen for the conversation, a preset of its model. It takes
    /// the place of the agent's preset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<i64>,
    /// Reasoning on or off, for a model that reads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<bool>,
    /// Level of reasoning (`low`, `medium`, `high`), for a model that reads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Whether the reasoning of the earlier answers goes back to the model
    /// at each turn, for a model that reads it. It takes room in the
    /// context; left out, it is not sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preserve_thinking: Option<bool>,
    /// Sampling temperature, between 0 and 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Upper bound on the length of an answer, in tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// What happens at the context limit; summarizing when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_overflow: Option<ContextOverflowSetting>,
    /// Instructions added to the mission of the agent for this conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// Accesses of the agent switched off for this conversation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_accesses: Vec<String>,
    /// Accesses the agent does not hold, proposed for this conversation:
    /// the model may call them and each call asks for consent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposed_accesses: Vec<String>,
    /// Accesses the conversation is limited to. When the list is not empty,
    /// of the accesses of the agent only those it names are kept, and those
    /// it names that the agent does not hold are proposed: each of their
    /// calls asks for consent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub only_accesses: Vec<String>,
    /// For an installed agent, whether it may read the user profile in this
    /// conversation, whatever its own profile source says. Left out, the
    /// source of the agent decides. The profile is read by a tool and never
    /// placed in the prompt of an installed agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_access: Option<bool>,
}

/// Why settings or notes are refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum SessionSettingsError {
    /// The instructions are longer than the mission block allows.
    #[error(transparent)]
    Instructions(#[from] apollia_prompts::InstructionsTooLong),
    /// The temperature is not a number between 0 and 2.
    #[error("the temperature must be between 0 and 2, got {0}")]
    Temperature(f32),
    /// The answer length is zero.
    #[error("the answer length must be at least one token")]
    MaxTokens,
    /// An access is blank, or a list names too many.
    #[error("invalid access list: {0}")]
    Accesses(String),
    /// The notes are too long.
    #[error("the notes hold {0} characters, more than the {SESSION_NOTES_MAX_CHARS} allowed")]
    Notes(usize),
}

impl SessionSettings {
    /// The settings as they are stored: instructions trimmed and dropped when
    /// blank, access lists trimmed and without repeats.
    ///
    /// # Errors
    ///
    /// [`SessionSettingsError`] for instructions beyond the cap, a
    /// temperature outside `0..=2`, a zero answer length, a blank access, or
    /// a list beyond [`SESSION_ACCESSES_MAX`].
    pub fn checked(mut self) -> Result<Self, SessionSettingsError> {
        self.instructions = self
            .instructions
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty());
        if let Some(text) = &self.instructions {
            apollia_prompts::check_session_instructions(text)?;
        }
        if let Some(temperature) = self.temperature {
            if !temperature.is_finite() || !(0.0..=2.0).contains(&temperature) {
                return Err(SessionSettingsError::Temperature(temperature));
            }
        }
        if self.max_tokens == Some(0) {
            return Err(SessionSettingsError::MaxTokens);
        }
        self.disabled_accesses = checked_accesses(self.disabled_accesses)?;
        self.proposed_accesses = checked_accesses(self.proposed_accesses)?;
        self.only_accesses = checked_accesses(self.only_accesses)?;
        Ok(self)
    }

    /// Read the `settings` column. A value this binary cannot read leaves
    /// the conversation on the settings of its agent, and says so.
    pub fn from_column(session_id: &str, raw: &str) -> Self {
        if raw.trim().is_empty() {
            return Self::default();
        }
        match serde_json::from_str(raw) {
            Ok(settings) => settings,
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    session_id = %session_id,
                    error = %error,
                    detail = "the conversation runs on the settings of its agent",
                    "chat.session.settings.unreadable"
                );
                Self::default()
            }
        }
    }

    /// The value of the `settings` column.
    pub fn to_column(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }

    /// The request settings of the conversation, the fourth level of a call.
    pub fn request_level(&self) -> RequestDefaults {
        RequestDefaults {
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            ..RequestDefaults::default()
        }
    }

    /// What the conversation does at the context limit.
    pub fn overflow(&self) -> ContextOverflow {
        self.context_overflow.unwrap_or_default().into()
    }

    /// Whether the conversation is limited to a list of accesses.
    pub fn is_limited(&self) -> bool {
        !self.only_accesses.is_empty()
    }

    /// `accesses` cut down to the list the conversation is limited to when
    /// it has one, with the proposed ones added and the switched-off ones
    /// taken out. A proposed access already held is not repeated; switching
    /// off wins over proposing.
    pub fn apply_to_accesses(&self, mut accesses: Vec<String>) -> Vec<String> {
        if self.is_limited() {
            accesses.retain(|name| self.only_accesses.contains(name));
        }
        for proposed in self.only_accesses.iter().chain(&self.proposed_accesses) {
            if !accesses.contains(proposed) {
                accesses.push(proposed.clone());
            }
        }
        accesses.retain(|name| !apollia_tools::is_disabled(name, &self.disabled_accesses));
        accesses
    }

    /// The accesses the conversation proposes, or is limited to, that the
    /// agent does not hold in `granted`: those whose every call asks.
    pub fn proposed_beyond(&self, granted: &[String]) -> Vec<String> {
        let mut beyond: Vec<String> = Vec::new();
        for proposed in self.only_accesses.iter().chain(&self.proposed_accesses) {
            if !granted.contains(proposed) && !beyond.contains(proposed) {
                beyond.push(proposed.clone());
            }
        }
        beyond
    }

    /// Whether an installed agent may read the user profile in this
    /// conversation: the choice of the conversation when it made one, the
    /// profile source of the agent (`source_checked`) otherwise.
    pub fn profile_readable(&self, source_checked: bool) -> bool {
        self.profile_access.unwrap_or(source_checked)
    }
}

/// The settings stored for the conversation `session_id` in the chat
/// database of `data_dir`, read on a connection of their own: what a reader
/// outside the chat actor sees. `None` when the conversation, the file or
/// the column is absent, or the file cannot be read.
pub fn stored_settings(data_dir: &std::path::Path, session_id: &str) -> Option<SessionSettings> {
    let path = apollia_core::paths::DataFile::Chat.path(data_dir);
    if !path.exists() {
        return None;
    }
    let read = rusqlite::Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .and_then(|conn| {
        use rusqlite::OptionalExtension;
        conn.query_row(
            "SELECT settings FROM chat_sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
    });
    match read {
        Ok(raw) => raw.map(|raw| SessionSettings::from_column(session_id, &raw)),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                session_id = %session_id,
                error = %error,
                detail = "the conversation is read as changing nothing",
                "chat.session.settings.unreadable"
            );
            None
        }
    }
}

/// Notes as they are stored: unchanged, within the cap.
///
/// # Errors
///
/// [`SessionSettingsError::Notes`] beyond [`SESSION_NOTES_MAX_CHARS`].
pub fn checked_notes(notes: String) -> Result<String, SessionSettingsError> {
    let chars = notes.chars().count();
    if chars > SESSION_NOTES_MAX_CHARS {
        return Err(SessionSettingsError::Notes(chars));
    }
    Ok(notes)
}

/// The instructions an older session prompt becomes: trimmed, cut at the cap
/// on a character boundary, `None` when blank.
pub fn instructions_from_legacy_prompt(prompt: &str) -> Option<String> {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(
        trimmed
            .chars()
            .take(apollia_prompts::SESSION_INSTRUCTIONS_MAX_CHARS)
            .collect::<String>()
            .trim_end()
            .to_owned(),
    )
}

fn checked_accesses(accesses: Vec<String>) -> Result<Vec<String>, SessionSettingsError> {
    let mut kept: Vec<String> = Vec::with_capacity(accesses.len());
    for access in accesses {
        let access = access.trim().to_owned();
        if access.is_empty() {
            return Err(SessionSettingsError::Accesses("an access is blank".into()));
        }
        if !kept.contains(&access) {
            kept.push(access);
        }
    }
    if kept.len() > SESSION_ACCESSES_MAX {
        return Err(SessionSettingsError::Accesses(format!(
            "{} accesses, more than the {SESSION_ACCESSES_MAX} allowed",
            kept.len()
        )));
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_refuse_a_field_they_do_not_know() {
        // GIVEN a document with a known field and one nobody declared
        let known = r#"{"temperature":0.4,"max_tokens":512}"#;
        let unknown = r#"{"temperature":0.4,"system_prompt":"You are someone else."}"#;

        // WHEN each is read
        let read = serde_json::from_str::<SessionSettings>(known);
        let refused = serde_json::from_str::<SessionSettings>(unknown);

        // THEN the first is read field by field, the second refused by name
        let read = read.expect("known fields");
        assert_eq!(read.temperature, Some(0.4));
        assert_eq!(read.max_tokens, Some(512));
        let error = refused.expect_err("unknown field").to_string();
        assert!(error.contains("unknown field `system_prompt`"), "{error}");
    }

    #[test]
    fn test_every_field_round_trips_and_an_absent_one_is_inherited() {
        // GIVEN settings with every field set, and the empty document
        let full = SessionSettings {
            preset: Some(7),
            thinking: Some(true),
            reasoning_effort: Some(ReasoningEffort::High),
            preserve_thinking: Some(true),
            temperature: Some(0.3),
            max_tokens: Some(1024),
            context_overflow: Some(ContextOverflowSetting::StopAtLimit),
            instructions: Some("Answer in one sentence.".into()),
            disabled_accesses: vec!["web_fetch".into()],
            proposed_accesses: vec!["file_write".into()],
            only_accesses: vec!["mcp:notion/search".into()],
            profile_access: Some(false),
        };

        // WHEN the first goes through the column and the second is read
        let column = full.to_column();
        let back = SessionSettings::from_column("s", &column);
        let empty = SessionSettings::from_column("s", "{}");

        // THEN the first comes back whole, in the names the contract states,
        // and the second inherits everything
        assert_eq!(back, full);
        assert!(column.contains(r#""reasoning_effort":"high""#), "{column}");
        assert!(column.contains(r#""preserve_thinking":true"#), "{column}");
        assert!(
            column.contains(r#""context_overflow":"stop_at_limit""#),
            "{column}"
        );
        assert_eq!(empty, SessionSettings::default());
        assert_eq!(empty.to_column(), "{}");
        assert_eq!(empty.overflow(), ContextOverflow::SummarizeMiddle);
    }

    #[test]
    fn test_a_column_this_binary_cannot_read_falls_back_to_the_agent() {
        // GIVEN a column holding a field from elsewhere, and one that is not JSON
        // WHEN each is read for a conversation
        let foreign = SessionSettings::from_column("s", r#"{"voice":"alto"}"#);
        let broken = SessionSettings::from_column("s", "not json");

        // THEN the conversation inherits everything rather than failing
        assert_eq!(foreign, SessionSettings::default());
        assert_eq!(broken, SessionSettings::default());
    }

    #[test]
    fn test_checked_settings_trim_and_refuse_what_cannot_be_applied() {
        // GIVEN settings to tidy, and four that cannot be applied
        let tidy = SessionSettings {
            instructions: Some("  Be brief.  ".into()),
            disabled_accesses: vec![" web_fetch ".into(), "web_fetch".into()],
            ..SessionSettings::default()
        };
        let blank_instructions = SessionSettings {
            instructions: Some("   ".into()),
            ..SessionSettings::default()
        };
        let long = SessionSettings {
            instructions: Some("a".repeat(apollia_prompts::SESSION_INSTRUCTIONS_MAX_CHARS + 1)),
            ..SessionSettings::default()
        };
        let hot = SessionSettings {
            temperature: Some(2.5),
            ..SessionSettings::default()
        };
        let empty_answer = SessionSettings {
            max_tokens: Some(0),
            ..SessionSettings::default()
        };
        let blank_access = SessionSettings {
            proposed_accesses: vec![" ".into()],
            ..SessionSettings::default()
        };

        // WHEN each is checked
        let tidy = tidy.checked().expect("tidy");
        let blank_instructions = blank_instructions.checked().expect("blank instructions");

        // THEN the first two are stored clean, the others refused by cause
        assert_eq!(tidy.instructions.as_deref(), Some("Be brief."));
        assert_eq!(tidy.disabled_accesses, vec!["web_fetch".to_string()]);
        assert_eq!(blank_instructions.instructions, None);
        assert!(matches!(
            long.checked(),
            Err(SessionSettingsError::Instructions(_))
        ));
        assert!(matches!(
            hot.checked(),
            Err(SessionSettingsError::Temperature(_))
        ));
        assert_eq!(empty_answer.checked(), Err(SessionSettingsError::MaxTokens));
        assert!(matches!(
            blank_access.checked(),
            Err(SessionSettingsError::Accesses(_))
        ));
    }

    #[test]
    fn test_accesses_of_the_conversation_remove_and_propose() {
        // GIVEN an agent granted three accesses, a conversation that switches
        // one off, a connector operation off, and proposes two, one held
        let granted = vec![
            "file_read".to_string(),
            "web_fetch".to_string(),
            "conn:google:ada@example.com/gmail.send".to_string(),
        ];
        let settings = SessionSettings {
            disabled_accesses: vec!["web_fetch".into(), "gmail.send".into()],
            proposed_accesses: vec!["file_write".into(), "file_read".into()],
            ..SessionSettings::default()
        };

        // WHEN the settings are applied to the list
        let offered = settings.apply_to_accesses(granted.clone());
        let asks = settings.proposed_beyond(&granted);

        // THEN the switched-off accesses are gone, the proposed one is added
        // once, and only the access the agent did not hold asks at each call
        assert_eq!(
            offered,
            vec!["file_read".to_string(), "file_write".to_string()]
        );
        assert_eq!(asks, vec!["file_write".to_string()]);
    }

    #[test]
    fn test_a_limited_conversation_keeps_its_list_and_asks_for_what_the_agent_does_not_hold() {
        // GIVEN an agent granted two accesses, and a conversation limited to
        // one of them and to the tool of a server the agent does not hold
        let granted = vec!["file_read".to_string(), "web_fetch".to_string()];
        let settings = SessionSettings {
            only_accesses: vec!["mcp:notion/search".into(), "file_read".into()],
            ..SessionSettings::default()
        };

        // WHEN the settings are applied to the list
        let offered = settings.apply_to_accesses(granted.clone());
        let asks = settings.proposed_beyond(&granted);

        // THEN nothing outside the list is left, the access the agent held
        // is kept as granted, and the other one asks at each call
        assert!(settings.is_limited());
        assert_eq!(
            offered,
            vec!["file_read".to_string(), "mcp:notion/search".to_string()]
        );
        assert_eq!(asks, vec!["mcp:notion/search".to_string()]);
    }

    #[test]
    fn test_the_profile_of_an_installed_agent_follows_the_conversation_then_its_source() {
        // GIVEN a conversation that says nothing, one that allows, one that forbids
        let inherit = SessionSettings::default();
        let allow = SessionSettings {
            profile_access: Some(true),
            ..SessionSettings::default()
        };
        let forbid = SessionSettings {
            profile_access: Some(false),
            ..SessionSettings::default()
        };

        // WHEN each is read against an agent whose profile source is checked
        // or cleared
        // THEN the conversation decides when it chose, the source otherwise
        assert!(inherit.profile_readable(true));
        assert!(!inherit.profile_readable(false));
        assert!(allow.profile_readable(false));
        assert!(!forbid.profile_readable(true));
    }

    #[test]
    fn test_a_legacy_prompt_becomes_instructions_within_the_cap() {
        // GIVEN a blank prompt, a short one, and one over the cap in
        // multibyte characters
        let long = "€".repeat(apollia_prompts::SESSION_INSTRUCTIONS_MAX_CHARS + 50);

        // WHEN each becomes instructions
        let blank = instructions_from_legacy_prompt("  \n ");
        let short = instructions_from_legacy_prompt(" Answer in French. ");
        let cut = instructions_from_legacy_prompt(&long).expect("kept");

        // THEN blank gives none, short is trimmed, long is cut at the cap
        assert_eq!(blank, None);
        assert_eq!(short.as_deref(), Some("Answer in French."));
        assert_eq!(
            cut.chars().count(),
            apollia_prompts::SESSION_INSTRUCTIONS_MAX_CHARS
        );
    }

    #[test]
    fn test_notes_are_kept_as_typed_within_the_cap() {
        // GIVEN notes with their own spacing, and notes over the cap
        let typed = "  first line\n\nsecond line  ".to_string();
        let long = "n".repeat(SESSION_NOTES_MAX_CHARS + 1);

        // WHEN each is checked
        let kept = checked_notes(typed.clone());
        let refused = checked_notes(long);

        // THEN the first is unchanged and the second refused with its length
        assert_eq!(kept, Ok(typed));
        assert_eq!(
            refused,
            Err(SessionSettingsError::Notes(SESSION_NOTES_MAX_CHARS + 1))
        );
    }
}
