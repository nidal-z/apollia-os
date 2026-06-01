//! `ApolliaCoach`: meta-chat routine for the product guide.
//!
//! Always-on, local-first coach invoked by:
//!   - the dedicated sidebar agent "Apollia Guide" (`/chat?agent=apollia-guide`)
//!   - the contextual onboarding widget (`OnboardingCoachWidget`)
//!
//! # Principles
//!
//! - **Never allocates a second LLM.** Reuses [`LlmRouter::get(None)`], the
//!   user's configured default backend. Local (`llama.cpp` / Ollama) means 100%
//!   local; cloud means the same API key the user already consented to.
//! - **Never calls the cloud without consent.** The backend selection is the
//!   user's responsibility; this routine never routes around it.
//! - **Bounded blast radius.** Structured output `{ text, action_buttons }`
//!   where `action_buttons.action` is restricted to `navigate` / `invoke`,
//!   and the frontend enforces a route whitelist on top.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::router::LlmRouter;
use crate::types::{ChatMessage, CompletionRequest, LlmError};

/// Max time we wait for the user's LLM to respond. Beyond that, a stub
/// reply is returned so the UI never hangs.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// System prompt header injected before the knowledge base.
///
/// The knowledge base itself lives in
/// `agents/system/apollia-guide/knowledge/*.md` and is embedded at compile
/// time via [`include_str!`] so the routine is fully offline-capable.
const PROMPT_HEADER: &str = "\
You are Apollia Guide, the built-in product coach for Apollia OS.

Ground rules:
1. NEVER invent a capability. Only describe features listed in the \
knowledge base below. If unsure, say so and point to the docs.
2. Be warm, concise (2-4 sentences) and respond in the user's language.
3. When a next step is obvious, append a single fenced JSON block with up \
to 3 action buttons. The frontend enforces a route whitelist - unknown \
routes are silently dropped.

   ```apollia-actions
   [{\"label\":\"Ouvrir les Connexions\",\"action\":\"navigate\",\"payload\":{\"route\":\"/integrations\"}}]
   ```

Allowed actions: `navigate` (payload.route must be an Apollia route) and \
`invoke` (payload.command must be a Tauri command name).

Allowed routes: /dashboard /agents /projects /tasks /chat /automations \
/automations?wizard=open /integrations /inbox /onboarding /llm /triggers \
/pipelines /memory /observability /notifications /settings
";

/// Bundled capability sheet. Kept small enough to fit every local model.
const KNOWLEDGE_CAPABILITIES: &str =
    include_str!("../../../../agents/system/apollia-guide/knowledge/capabilities.md");

/// Bundled walkthroughs with suggested action buttons per intent.
const KNOWLEDGE_TUTORIALS: &str =
    include_str!("../../../../agents/system/apollia-guide/knowledge/tutorials.md");

/// Hard cap on the knowledge base we inject; prevents OOM with 7B-class models.
const MAX_KB_CHARS: usize = 12_000;

/// Hard cap on user history we forward to the LLM (last N turns).
const MAX_HISTORY_TURNS: usize = 12;

// Public types

/// Conversation surface the coach is serving.
///
/// The prompt changes subtly: operator receives pragmatic next-steps, builder
/// receives technical vocabulary (manifest, tool, pipeline, trigger).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoachMode {
    /// Non-technical operator (sidebar "Apollia" entry in operator mode).
    Operator,
    /// Developer / builder (sidebar "Apollia" entry in builder mode).
    Builder,
    /// Contextual onboarding widget: short replies, stage-aware.
    Onboarding,
}

impl Default for CoachMode {
    fn default() -> Self {
        Self::Operator
    }
}

/// Optional extra context passed to the coach (onboarding stage, current route…).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoachContext {
    /// Onboarding stage when [`CoachMode::Onboarding`], e.g. `"ai_setup"`.
    #[serde(default)]
    pub onboarding_stage: Option<String>,
    /// Route the user is currently viewing (`/automations`, `/inbox`, …).
    #[serde(default)]
    pub current_route: Option<String>,
    /// Declared role from onboarding memory (operator / builder / …).
    #[serde(default)]
    pub user_role: Option<String>,
    /// Identifiers of agents the user has already installed; enables
    /// "You already have Linear installed, you can…" guidance.
    #[serde(default)]
    pub installed_agents: Vec<String>,
    /// Integrations connected by the user (OAuth providers, MCP servers).
    #[serde(default)]
    pub connected_integrations: Vec<String>,
}

/// Whitelisted kinds of UI actions the coach may propose.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoachAction {
    /// Navigate to an internal route (whitelisted in the frontend).
    Navigate,
    /// Invoke a Tauri command (whitelisted in the frontend).
    Invoke,
}

/// Single action button rendered in the coach reply bubble.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionButton {
    /// User-visible label (translated by the frontend when needed).
    pub label: String,
    /// Action kind, restricted to safe, read-only UI actions.
    pub action: CoachAction,
    /// Action-specific payload (`route` for `navigate`, `command` for `invoke`).
    pub payload: serde_json::Value,
}

/// Structured output returned to the frontend.
///
/// `text` is rendered as a plain chat bubble; `action_buttons` as inline
/// buttons below the bubble. Always populate `text`; `action_buttons` may
/// be empty when the LLM has nothing concrete to offer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoachResponse {
    pub text: String,
    #[serde(default)]
    pub action_buttons: Vec<ActionButton>,
}

/// Errors surfaced by [`invoke_apollia_coach`].
#[derive(Debug, thiserror::Error)]
pub enum ApolliaCoachError {
    /// No default LLM backend is configured in the router.
    #[error("no default LLM backend configured - open Settings → LLM")]
    NoBackend,
    /// The underlying LLM call failed or timed out.
    #[error("LLM call failed: {0}")]
    Llm(#[from] LlmError),
    /// The LLM returned a response we could not extract text from.
    #[error("LLM returned an empty response")]
    EmptyResponse,
}

// ─────────────────────────────────────────────
// Prompt assembly
// ─────────────────────────────────────────────

fn truncate_kb(raw: &str) -> &str {
    if raw.len() <= MAX_KB_CHARS {
        return raw;
    }
    // Cut on a newline boundary so we do not split a header mid-line.
    match raw[..MAX_KB_CHARS].rfind('\n') {
        Some(idx) => &raw[..idx],
        None => &raw[..MAX_KB_CHARS],
    }
}

fn build_system_prompt(mode: CoachMode, ctx: &CoachContext) -> String {
    let mode_line = match mode {
        CoachMode::Operator => {
            "Audience: operator (non-technical). Prefer pragmatic, jargon-free guidance."
        }
        CoachMode::Builder => {
            "Audience: builder (developer). Technical vocabulary welcome \
(manifest, tool, pipeline, trigger, step budget, HITL, MCP)."
        }
        CoachMode::Onboarding => {
            "Audience: user mid-onboarding. Keep replies under 3 sentences \
and tailor them to the current onboarding stage."
        }
    };

    let mut ctx_lines: Vec<String> = Vec::new();
    if let Some(stage) = &ctx.onboarding_stage {
        ctx_lines.push(format!("Current onboarding stage: {stage}"));
    }
    if let Some(route) = &ctx.current_route {
        ctx_lines.push(format!("User is currently viewing: {route}"));
    }
    if let Some(role) = &ctx.user_role {
        ctx_lines.push(format!("Declared user role: {role}"));
    }
    if !ctx.installed_agents.is_empty() {
        ctx_lines.push(format!(
            "Installed agents: {}",
            ctx.installed_agents.join(", ")
        ));
    }
    if !ctx.connected_integrations.is_empty() {
        ctx_lines.push(format!(
            "Connected integrations: {}",
            ctx.connected_integrations.join(", ")
        ));
    }

    let ctx_block = if ctx_lines.is_empty() {
        String::new()
    } else {
        format!("\n\n## Live context\n{}", ctx_lines.join("\n"))
    };

    let kb_combined = format!(
        "{}\n\n---\n\n{}",
        truncate_kb(KNOWLEDGE_CAPABILITIES),
        truncate_kb(KNOWLEDGE_TUTORIALS),
    );
    let kb = truncate_kb(&kb_combined);

    format!(
        "{header}\n\n{mode_line}{ctx_block}\n\n## Knowledge base\n\n{kb}",
        header = PROMPT_HEADER,
        mode_line = mode_line,
        ctx_block = ctx_block,
        kb = kb,
    )
}

// ─────────────────────────────────────────────
// Parsing (LLM output → CoachResponse)
// ─────────────────────────────────────────────

/// Matches a ```apollia-actions [ ... ] ``` fenced JSON block.
fn extract_action_block(raw: &str) -> Option<&str> {
    let fence = "```apollia-actions";
    let start = raw.find(fence)? + fence.len();
    let rest = raw.get(start..)?.trim_start();
    let end = rest.find("```")?;
    Some(rest[..end].trim())
}

fn parse_action_buttons(raw: &str) -> Vec<ActionButton> {
    let Some(json_str) = extract_action_block(raw) else {
        return Vec::new();
    };
    let parsed: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let arr = match parsed.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter()
        .take(3)
        .filter_map(|item| {
            let label = item.get("label")?.as_str()?.to_string();
            let action_str = item.get("action")?.as_str()?;
            let action = match action_str {
                "navigate" => CoachAction::Navigate,
                "invoke" => CoachAction::Invoke,
                _ => return None,
            };
            let payload = item
                .get("payload")
                .cloned()
                .unwrap_or(serde_json::Value::Object(Default::default()));
            Some(ActionButton {
                label,
                action,
                payload,
            })
        })
        .collect()
}

fn strip_action_block(raw: &str) -> String {
    let fence = "```apollia-actions";
    let Some(start) = raw.find(fence) else {
        return raw.trim().to_string();
    };
    let before = &raw[..start];
    let after_start = start + fence.len();
    let after = raw.get(after_start..).unwrap_or("");
    let cleaned_after = match after.find("```") {
        Some(end) => &after[end + 3..],
        None => "",
    };
    format!("{}{}", before.trim_end(), cleaned_after.trim_start())
        .trim()
        .to_string()
}

// ─────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────

/// Single message in the coach conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoachTurn {
    /// `"user"` or `"assistant"`.
    pub role: String,
    /// Plain text content.
    pub content: String,
}

/// Invoke the Apollia Guide coach with the user's configured LLM.
///
/// Returns a [`CoachResponse`] ready to render. Never spawns a second LLM
/// and never performs a network call the user did not already authorise
/// (because the backend selection is theirs).
///
/// # Errors
///
/// - [`ApolliaCoachError::NoBackend`] if no default LLM backend is set.
/// - [`ApolliaCoachError::Llm`] on transport / inference errors.
/// - [`ApolliaCoachError::EmptyResponse`] if the LLM returned no content.
pub async fn invoke_apollia_coach(
    router: &Arc<LlmRouter>,
    mode: CoachMode,
    context: CoachContext,
    history: Vec<CoachTurn>,
    user_message: String,
) -> Result<CoachResponse, ApolliaCoachError> {
    let backend = router.get(None).ok_or(ApolliaCoachError::NoBackend)?;

    let system = build_system_prompt(mode, &context);

    let mut messages = Vec::with_capacity(history.len() + 2);
    messages.push(ChatMessage::system(system));
    let history_tail = if history.len() > MAX_HISTORY_TURNS {
        &history[history.len() - MAX_HISTORY_TURNS..]
    } else {
        &history[..]
    };
    for turn in history_tail {
        let msg = match turn.role.as_str() {
            "assistant" | "agent" => ChatMessage::assistant(turn.content.clone()),
            _ => ChatMessage::user(turn.content.clone()),
        };
        messages.push(msg);
    }
    messages.push(ChatMessage::user(user_message));

    let req = CompletionRequest {
        messages,
        temperature: Some(0.5),
        max_tokens: Some(512),
        ..Default::default()
    };

    let call = backend.complete(req);
    let response = match tokio::time::timeout(CALL_TIMEOUT, call).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return Err(ApolliaCoachError::Llm(e)),
        Err(_) => return Err(ApolliaCoachError::Llm(LlmError::Cancelled)),
    };

    let raw = response.content.trim();
    if raw.is_empty() {
        return Err(ApolliaCoachError::EmptyResponse);
    }

    let action_buttons = parse_action_buttons(raw);
    let text = strip_action_block(raw);
    let text = if text.is_empty() {
        // LLM emitted only the action block; fall back to a short stock message.
        "Voici ce que je vous propose :".to_string()
    } else {
        text
    };

    Ok(CoachResponse {
        text,
        action_buttons,
    })
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_and_strip_action_block_roundtrip() {
        let raw = "Sure, here is the next step.\n\n```apollia-actions\n[{\"label\":\"Open\",\"action\":\"navigate\",\"payload\":{\"route\":\"/inbox\"}}]\n```";
        let buttons = parse_action_buttons(raw);
        assert_eq!(buttons.len(), 1);
        assert_eq!(buttons[0].label, "Open");
        assert!(matches!(buttons[0].action, CoachAction::Navigate));

        let text = strip_action_block(raw);
        assert_eq!(text, "Sure, here is the next step.");
    }

    #[test]
    fn parse_rejects_unknown_action_kind() {
        let raw =
            "```apollia-actions\n[{\"label\":\"Del\",\"action\":\"delete\",\"payload\":{}}]\n```";
        let buttons = parse_action_buttons(raw);
        assert!(buttons.is_empty(), "unknown action kinds must be dropped");
    }

    #[test]
    fn parse_caps_at_three_buttons() {
        let raw = "```apollia-actions\n[\
            {\"label\":\"a\",\"action\":\"navigate\",\"payload\":{\"route\":\"/inbox\"}},\
            {\"label\":\"b\",\"action\":\"navigate\",\"payload\":{\"route\":\"/agents\"}},\
            {\"label\":\"c\",\"action\":\"navigate\",\"payload\":{\"route\":\"/chat\"}},\
            {\"label\":\"d\",\"action\":\"navigate\",\"payload\":{\"route\":\"/tasks\"}}\
        ]\n```";
        let buttons = parse_action_buttons(raw);
        assert_eq!(buttons.len(), 3);
    }

    #[test]
    fn build_system_prompt_changes_with_mode() {
        let ctx = CoachContext::default();
        let operator = build_system_prompt(CoachMode::Operator, &ctx);
        let builder = build_system_prompt(CoachMode::Builder, &ctx);
        assert_ne!(operator, builder);
        assert!(operator.contains("operator"));
        assert!(builder.contains("builder"));
    }

    #[test]
    fn build_system_prompt_injects_live_context() {
        let ctx = CoachContext {
            onboarding_stage: Some("ai_setup".into()),
            current_route: Some("/automations".into()),
            user_role: Some("operator".into()),
            installed_agents: vec!["linear-assistant".into()],
            connected_integrations: vec!["slack".into()],
        };
        let prompt = build_system_prompt(CoachMode::Onboarding, &ctx);
        assert!(prompt.contains("ai_setup"));
        assert!(prompt.contains("/automations"));
        assert!(prompt.contains("linear-assistant"));
        assert!(prompt.contains("slack"));
    }

    #[test]
    fn truncate_kb_caps_length() {
        let long = "a\n".repeat(MAX_KB_CHARS);
        let trimmed = truncate_kb(&long);
        assert!(trimmed.len() <= MAX_KB_CHARS);
    }
}
