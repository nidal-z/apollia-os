//! Deterministic assembly of a chat/agent system prompt from typed blocks.
//!
//! The builder owns the block ORDER and separators, the callers own the data.
//! Order: temporal, base, the instructions of the conversation,
//! working-directory, persona, plan, then the language footer. Empty / `None`
//! blocks are skipped, so a minimal prompt is just the base plus the footer.
//!
//! The base is the mission of the agent. The instructions of a conversation
//! are added after it and never take its place.

use crate::{blocks, LANGUAGE_FOOTER};

/// Longest instructions a conversation may add to the mission, in characters.
pub const SESSION_INSTRUCTIONS_MAX_CHARS: usize = 4000;

/// The instructions of a conversation are longer than
/// [`SESSION_INSTRUCTIONS_MAX_CHARS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstructionsTooLong {
    /// Length of the refused text, in characters.
    pub chars: usize,
}

impl std::fmt::Display for InstructionsTooLong {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the instructions of the conversation hold {} characters, more than the {} allowed",
            self.chars, SESSION_INSTRUCTIONS_MAX_CHARS
        )
    }
}

impl std::error::Error for InstructionsTooLong {}

/// Checks that `text` fits the instructions of a conversation.
///
/// # Errors
///
/// [`InstructionsTooLong`] beyond [`SESSION_INSTRUCTIONS_MAX_CHARS`].
pub fn check_session_instructions(text: &str) -> Result<(), InstructionsTooLong> {
    let chars = text.chars().count();
    if chars > SESSION_INSTRUCTIONS_MAX_CHARS {
        return Err(InstructionsTooLong { chars });
    }
    Ok(())
}

/// Which built-in base prompt a turn uses, mirroring the autonomy tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseProfile {
    /// Reactive assisted tier, uses [`blocks::DEFAULT_SYSTEM_PROMPT`].
    Assisted,
    /// Autonomous tiers, use [`blocks::PERSEVERANCE_SYSTEM_PROMPT`].
    Autonomous,
}

impl BaseProfile {
    /// The base prompt text for this profile.
    pub fn prompt(self) -> &'static str {
        match self {
            BaseProfile::Assisted => blocks::DEFAULT_SYSTEM_PROMPT,
            BaseProfile::Autonomous => blocks::PERSEVERANCE_SYSTEM_PROMPT,
        }
    }
}

/// Assembles a chat/agent system prompt from its blocks in a fixed order.
///
/// All setters are optional except the base (set via [`Self::base`] or
/// [`Self::profile`]). Dynamic blocks (temporal, working directory, persona,
/// plan) are passed in by the caller; this crate performs no I/O.
#[derive(Debug, Default, Clone)]
pub struct ChatPromptBuilder<'a> {
    temporal: Option<&'a str>,
    base: &'a str,
    session_instructions: Option<String>,
    working_dir_note: Option<&'a str>,
    persona: Option<&'a str>,
    plan: Option<&'a str>,
    append_language_footer: bool,
}

impl<'a> ChatPromptBuilder<'a> {
    /// Start a builder for the given base prompt (the mission of the agent, or
    /// one of the [`BaseProfile`] prompts).
    pub fn new(base: &'a str) -> Self {
        Self {
            base,
            append_language_footer: true,
            ..Self::default()
        }
    }

    /// Start a builder from a built-in [`BaseProfile`].
    pub fn from_profile(profile: BaseProfile) -> Self {
        Self::new(profile.prompt())
    }

    /// The authoritative temporal/environment block (prepended first).
    pub fn temporal(mut self, block: &'a str) -> Self {
        self.temporal = Some(block);
        self
    }

    /// The instructions of the conversation, added after the base under their
    /// own heading (see [`blocks::session_instructions_block`]). Blank or
    /// `None` adds nothing.
    ///
    /// # Errors
    ///
    /// [`InstructionsTooLong`] beyond [`SESSION_INSTRUCTIONS_MAX_CHARS`]: the
    /// builder refuses the text instead of cutting it.
    pub fn session_instructions(
        mut self,
        instructions: Option<&str>,
    ) -> Result<Self, InstructionsTooLong> {
        self.session_instructions = match instructions.map(str::trim) {
            Some(text) if !text.is_empty() => {
                check_session_instructions(text)?;
                Some(blocks::session_instructions_block(text))
            }
            _ => None,
        };
        Ok(self)
    }

    /// The working-directory anchor note (see [`blocks::working_directory_note`]).
    pub fn working_dir_note(mut self, note: Option<&'a str>) -> Self {
        self.working_dir_note = note;
        self
    }

    /// The user-persona block (see [`blocks::persona_block`]).
    pub fn persona(mut self, persona: Option<&'a str>) -> Self {
        self.persona = persona;
        self
    }

    /// The plan-mode block (`PLAN_MODE_BLOCK` or `PLAN_EXECUTE_BLOCK`).
    pub fn plan(mut self, plan: Option<&'a str>) -> Self {
        self.plan = plan;
        self
    }

    /// Whether to append the language footer (default `true`). Disable for
    /// non-user-facing prompts.
    pub fn language_footer(mut self, append: bool) -> Self {
        self.append_language_footer = append;
        self
    }

    /// Assemble the final prompt, skipping empty blocks, joining with blank
    /// lines.
    pub fn build(self) -> String {
        let mut parts: Vec<&str> = Vec::with_capacity(7);
        if let Some(t) = self.temporal {
            parts.push(t);
        }
        parts.push(self.base);
        if let Some(i) = self.session_instructions.as_deref() {
            parts.push(i);
        }
        if let Some(w) = self.working_dir_note {
            parts.push(w);
        }
        if let Some(p) = self.persona {
            parts.push(p);
        }
        if let Some(p) = self.plan {
            parts.push(p);
        }
        if self.append_language_footer {
            parts.push(LANGUAGE_FOOTER);
        }
        parts
            .into_iter()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_instructions_of_a_conversation_follow_the_mission_and_never_replace_it() {
        // GIVEN a mission and instructions typed for one conversation
        let mission = "You file the invoices of the firm.";
        let instructions = "Answer in one sentence.";

        // WHEN the prompt is assembled
        let prompt = ChatPromptBuilder::new(mission)
            .session_instructions(Some(instructions))
            .expect("short instructions")
            .plan(Some("PLAN BLOCK"))
            .build();

        // THEN the mission is kept, the instructions come after it under
        // their heading, and before the blocks that follow
        let mission_at = prompt.find(mission).expect("mission kept");
        let heading_at = prompt
            .find(blocks::SESSION_INSTRUCTIONS_HEADING)
            .expect("heading");
        let text_at = prompt.find(instructions).expect("instructions");
        let plan_at = prompt.find("PLAN BLOCK").expect("plan");
        let footer_at = prompt.find(LANGUAGE_FOOTER).expect("footer");
        assert!(mission_at < heading_at && heading_at < text_at);
        assert!(text_at < plan_at && plan_at < footer_at);
    }

    #[test]
    fn blank_instructions_add_no_block() {
        // GIVEN instructions made of spaces
        // WHEN the prompt is assembled
        let prompt = ChatPromptBuilder::new("Mission.")
            .session_instructions(Some("   "))
            .expect("blank is accepted")
            .build();

        // THEN no heading is written
        assert!(!prompt.contains(blocks::SESSION_INSTRUCTIONS_HEADING));
    }

    #[test]
    fn instructions_beyond_the_cap_are_refused() {
        // GIVEN instructions one character over the cap, and some at the cap
        let over = "a".repeat(SESSION_INSTRUCTIONS_MAX_CHARS + 1);
        let at = "a".repeat(SESSION_INSTRUCTIONS_MAX_CHARS);

        // WHEN each is handed to the builder
        let refused = ChatPromptBuilder::new("Mission.").session_instructions(Some(&over));
        let accepted = ChatPromptBuilder::new("Mission.").session_instructions(Some(&at));

        // THEN the first is refused with its length, the second accepted
        assert_eq!(
            refused.err(),
            Some(InstructionsTooLong {
                chars: SESSION_INSTRUCTIONS_MAX_CHARS + 1
            })
        );
        assert!(accepted.is_ok());
    }
}
