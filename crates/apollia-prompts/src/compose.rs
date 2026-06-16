//! Deterministic assembly of a chat/agent system prompt from typed blocks.
//!
//! The builder owns the block ORDER and separators, the callers own the data.
//! Order: temporal, base, working-directory, persona, plan, then the language
//! footer. Empty / `None` blocks are skipped, so a minimal prompt is just the
//! base plus the footer.

use crate::{blocks, LANGUAGE_FOOTER};

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
    working_dir_note: Option<&'a str>,
    persona: Option<&'a str>,
    plan: Option<&'a str>,
    append_language_footer: bool,
}

impl<'a> ChatPromptBuilder<'a> {
    /// Start a builder for the given base prompt (custom session prompt, or one
    /// of the [`BaseProfile`] prompts).
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
        let mut parts: Vec<&str> = Vec::with_capacity(6);
        if let Some(t) = self.temporal {
            parts.push(t);
        }
        parts.push(self.base);
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
