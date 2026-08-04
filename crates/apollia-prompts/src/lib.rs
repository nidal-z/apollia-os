#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Single source of truth for Apollia OS system-prompt text and composition.
//!
//! Every instruction the platform sends to an LLM, the chat base prompts, the
//! plan-mode blocks, the ORIA planner/replanner, the verification critic, lives
//! here in English. User-facing prompts append [`LANGUAGE_FOOTER`] so the model
//! answers in the user's language. This keeps a single coherent prompt voice
//! across chat, ORIA, and the critic instead of the previous FR/EN patchwork.
//!
//! The crate is a leaf: it performs no I/O. Dynamic blocks (temporal,
//! persona, project, workspace) are produced by callers and passed in as
//! `&str`.

pub mod blocks;
pub mod compose;

pub use compose::{BaseProfile, ChatPromptBuilder};

/// Appended to user-facing system prompts so the model replies in the user's
/// language even though the instructions themselves are English.
pub const LANGUAGE_FOOTER: &str = "Always respond in the language of the user's latest message. \
The instructions above are in English for precision, but your reply to the user must match \
their language; never switch to English unless the user writes in English.";

/// Substitute `{key}` placeholders in `template` with their values.
///
/// Used by the ORIA reasoner to fill the planner/replanner prompts. A key that
/// does not appear in the template is a no-op; a placeholder with no matching
/// key is left untouched (so a missing value is visible rather than silently
/// blanked).
pub fn render(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (key, value) in vars {
        out = out.replace(&format!("{{{key}}}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_orders_blocks_and_appends_footer() {
        // GIVEN every optional block plus a base profile
        let prompt = ChatPromptBuilder::from_profile(BaseProfile::Assisted)
            .temporal("TEMPORAL_BLOCK")
            .working_dir_note(Some("WORKDIR_NOTE"))
            .persona(Some("PERSONA_BLOCK"))
            .plan(Some("PLAN_BLOCK"))
            .build();

        // WHEN inspecting positions
        let pos = |needle: &str| prompt.find(needle).unwrap_or(usize::MAX);

        // THEN the order is temporal, base, working-dir, persona, plan, footer
        assert!(pos("TEMPORAL_BLOCK") < pos("Act first"));
        assert!(pos("Act first") < pos("WORKDIR_NOTE"));
        assert!(pos("WORKDIR_NOTE") < pos("PERSONA_BLOCK"));
        assert!(pos("PERSONA_BLOCK") < pos("PLAN_BLOCK"));
        assert!(pos("PLAN_BLOCK") < pos("language of the user"));
    }

    #[test]
    fn test_builder_skips_absent_blocks() {
        // GIVEN only a base, no optional blocks
        let prompt = ChatPromptBuilder::new("BASE").build();
        // THEN it is just the base plus the footer
        assert!(prompt.starts_with("BASE"));
        assert!(prompt.contains("language of the user"));
        assert!(!prompt.contains("PERSONA"));
    }

    #[test]
    fn test_language_footer_can_be_disabled() {
        // GIVEN a builder with the footer disabled (non-user-facing)
        let prompt = ChatPromptBuilder::new("BASE")
            .language_footer(false)
            .build();
        // THEN the footer is absent
        assert!(!prompt.contains("language of the user"));
    }

    #[test]
    fn test_render_substitutes_placeholders() {
        // GIVEN a template with placeholders
        let out = render(
            "max {max_steps}, tools {tool_names}",
            &[("max_steps", "5"), ("tool_names", "a, b")],
        );
        // THEN every key is substituted
        assert_eq!(out, "max 5, tools a, b");
    }

    #[test]
    fn environment_block_contains_all_supplied_fields() {
        // GIVEN every fact supplied
        let block = blocks::environment_block(
            "macos",
            Some("15.5"),
            "aarch64",
            Some("/bin/sh"),
            Some("/Users/x/workspace"),
        );

        // WHEN reading the rendered block
        // THEN each fact appears under the stable header
        assert!(block.starts_with("## HOST ENVIRONMENT"));
        assert!(block.contains("macos 15.5 (aarch64)"));
        assert!(block.contains("/bin/sh"));
        assert!(block.contains("/Users/x/workspace"));
        assert!(block.contains("Do not assume a different OS"));
    }

    #[test]
    fn environment_block_names_shell_prerequisite_when_absent() {
        // GIVEN a host with no POSIX shell (Windows before Git Bash/WSL)
        let block = blocks::environment_block("windows", None, "x86_64", None, None);

        // WHEN reading the shell line
        // THEN it names the providers of the missing prerequisite
        assert!(block.contains("Git Bash"), "missing Git Bash: {block}");
        assert!(block.contains("WSL"), "missing WSL: {block}");
        assert!(block.contains("bash_executor will fail"));
    }

    #[test]
    fn environment_block_never_says_sandbox() {
        // GIVEN both shapes of the block
        let with_all = blocks::environment_block(
            "linux",
            Some("Ubuntu 24.04"),
            "x86_64",
            Some("/bin/sh"),
            Some("/home/x"),
        );
        let with_none = blocks::environment_block("windows", None, "x86_64", None, None);

        // WHEN scanning for the reserved word
        // THEN "sandbox" is absent: the docs reserve it for OS confinement,
        // and the filesystem root is a path check, not confinement
        for block in [with_all, with_none] {
            assert!(
                !block.to_lowercase().contains("sandbox"),
                "environment block must not use the word sandbox: {block}"
            );
        }
    }

    #[test]
    fn test_blocks_are_english_no_french_accents() {
        // GIVEN every static block that should be English-only
        let corpus = [
            blocks::DEFAULT_SYSTEM_PROMPT,
            blocks::PERSEVERANCE_SYSTEM_PROMPT,
            blocks::PLAN_MODE_BLOCK,
            blocks::PLAN_EXECUTE_BLOCK,
            blocks::PLANNER_SYSTEM_PROMPT,
            blocks::REPLANNER_SYSTEM_PROMPT,
            blocks::CRITIC_SYSTEM_PROMPT,
            LANGUAGE_FOOTER,
        ]
        .concat();

        // WHEN scanning for the common French accented characters
        // THEN none are present (the blocks are English)
        for ch in ['é', 'è', 'ê', 'à', 'â', 'ç', 'ù', 'û', 'î', 'ô'] {
            assert!(
                !corpus.contains(ch),
                "found French accent '{ch}' in an English prompt block"
            );
        }
    }
}
