use serde::Deserialize;

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
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChatConfig {
    /// Default plan-mode state inherited by every new chat session.
    ///
    /// When `true`, a session is created with plan mode already enabled, so the
    /// agent drafts a plan before acting. The per-session toggle still overrides
    /// this default for an individual session without changing the global value.
    /// Default: `false`.
    #[serde(default)]
    pub plan_mode_default: bool,

    /// Default working directory for free-chat (project-less) sessions.
    ///
    /// When set to an existing directory, free-chat file tools and bash anchor
    /// there instead of the process working directory, and the agent is told its
    /// working directory so it stops guessing paths. Project-linked sessions keep
    /// their own `workspace_path`. `None` falls back to `~/.apollia`.
    #[serde(default)]
    pub default_workspace: Option<String>,

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
}
