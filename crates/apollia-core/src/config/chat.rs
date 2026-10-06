use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
// ChatConfig
// ─────────────────────────────────────────────

/// Chat subsystem configuration (`[chat]` section in `apollia.toml`).
///
/// Holds the session-level defaults the runtime applies when a new chat session
/// is created. This is the single source of truth for those defaults: the
/// desktop seeds its own per-user preference from this value, and the runtime
/// itself uses it so non-desktop callers (API, CLI) inherit the same behavior.
/// Every field has a sane default via [`Default`] (plan mode off).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatConfig {
    /// Default plan-mode state inherited by every new chat session.
    ///
    /// When `true`, a session is created with plan mode already enabled, so the
    /// agent drafts a plan before acting. The per-session toggle still overrides
    /// this default for an individual session without changing the global value.
    /// Default: `false`.
    #[serde(default)]
    pub plan_mode_default: bool,

    /// LLM temperature applied to a chat turn that advertises tools to the model.
    ///
    /// Small local models tool-call erratically at conversational temperatures
    /// (0.6-0.7): they hallucinate tool names, malform arguments, or wrap calls
    /// in prose. Lowering the temperature whenever tools are exposed makes the
    /// structured tool-call output far more reliable, while plain conversational
    /// turns (no tools) keep their natural temperature. `None` resolves to `0.3`
    /// at the agent.
    #[serde(default)]
    pub tool_turn_temperature: Option<f32>,

    /// Whether a new conversation starts with the profile shared with the
    /// built-in assistant.
    ///
    /// This is the position the profile switch of a conversation is born in.
    /// A conversation that exists keeps its own switch: changing this key
    /// moves none of them. Default: `true`.
    #[serde(default = "default_share_profile")]
    pub share_profile_default: bool,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            plan_mode_default: false,
            tool_turn_temperature: None,
            share_profile_default: default_share_profile(),
        }
    }
}

fn default_share_profile() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        resolve_setting, write_setting, PolicySettings, SettingError, SettingSource,
    };

    /// Dotted key of the profile switch a new conversation is born with.
    const SHARE_PROFILE_DEFAULT_KEY: &str = "chat.share_profile_default";

    #[test]
    fn test_a_new_conversation_shares_the_profile_unless_the_file_says_otherwise() {
        // GIVEN a file without the key, and one that turns the sharing off
        let dir = tempfile::tempdir().unwrap();
        let absent = dir.path().join("absent.toml");
        let off = dir.path().join("off.toml");
        std::fs::write(&off, "[chat]\nshare_profile_default = false\n").unwrap();
        let none = PolicySettings::none();

        // WHEN the named setting is resolved against each
        let default = resolve_setting(&absent, SHARE_PROFILE_DEFAULT_KEY, &none).unwrap();
        let user = resolve_setting(&off, SHARE_PROFILE_DEFAULT_KEY, &none).unwrap();

        // THEN the absent key shares the profile and the file turns it off
        assert_eq!(default.value, serde_json::json!(true));
        assert_eq!(default.source, SettingSource::Default);
        assert_eq!(user.value, serde_json::json!(false));
        assert_eq!(user.source, SettingSource::User);
        assert!(ChatConfig::default().share_profile_default);
    }

    #[test]
    fn test_a_policy_fixes_the_profile_default() {
        // GIVEN a policy that imposes the sharing off
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apollia.toml");
        let policy =
            PolicySettings::none().impose(SHARE_PROFILE_DEFAULT_KEY, serde_json::json!(false));

        // WHEN the key is read, then written by the user
        let resolved = resolve_setting(&path, SHARE_PROFILE_DEFAULT_KEY, &policy).unwrap();
        let written = write_setting(
            &path,
            SHARE_PROFILE_DEFAULT_KEY,
            serde_json::json!(true),
            &policy,
        );

        // THEN the policy answers and the write is refused
        assert_eq!(resolved.value, serde_json::json!(false));
        assert_eq!(resolved.source, SettingSource::Policy);
        assert!(
            matches!(written, Err(SettingError::LockedByPolicy { .. })),
            "{written:?}"
        );
    }
}
