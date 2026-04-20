//! `GenerateNextSteps` — meta-LLM routine producing up to 3 actionable
//! "next steps" cards for the operator Dashboard and for end-of-session
//! debriefs (US-SP42-059, Chantier 4 P12, finding H.20).
//!
//! # Principles
//!
//! - **Reuses the user's default LLM backend** via [`LlmRouter::get(None)`]
//!   — never allocates a second model nor performs a cloud call the user
//!   has not already authorised (ADR-073).
//! - **Whitelisted action surface**: each suggestion points to either a
//!   known Apollia route (`/memory`, `/automations`, `/templates`, …) or
//!   a known Tauri command (`memory_insert`, …). The parser drops any
//!   action outside the whitelist so the LLM cannot invent invalid links.
//! - **Graceful fallback**: any LLM error yields 3 generic heuristics so
//!   the panel always renders something useful.
//! - **Bounded output**: max 3 cards, each with `{title, description,
//!   action_button{label, action, payload}}`.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::router::LlmRouter;
use crate::types::{ChatMessage, CompletionRequest, LlmError};

/// Upper bound on the LLM call — beyond that we emit the heuristic fallback.
const CALL_TIMEOUT: Duration = Duration::from_secs(20);

/// Max number of cards ever returned, regardless of LLM output.
pub const MAX_NEXT_STEPS: usize = 3;

/// Allowed navigation routes — mirror of the frontend whitelist. Any suggestion
/// referencing a route not in this list is silently dropped.
const ALLOWED_ROUTES: &[&str] = &[
    "/dashboard",
    "/agents",
    "/projects",
    "/tasks",
    "/chat",
    "/automations",
    "/automations?wizard=open",
    "/integrations",
    "/inbox",
    "/onboarding",
    "/llm",
    "/triggers",
    "/pipelines",
    "/memory",
    "/memory?new",
    "/observability",
    "/notifications",
    "/settings",
    "/templates",
    "/templates?filter=agents",
];

/// Allowed Tauri commands the LLM may propose. The frontend also enforces this
/// list, but keeping it here means bogus suggestions never reach the UI.
const ALLOWED_COMMANDS: &[&str] = &[
    "memory_insert",
    "create_trigger",
    "create_pipeline",
    "install_agent",
    "export_session",
];

/// Panel the suggestion targets — lets the prompt steer between "derniers
/// jours" (global) and "cette session" (session end).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NextStepsContext {
    /// Operator Dashboard — rolling context of the last few days.
    GlobalContext,
    /// End of a chat session — contextualised to the session that just closed.
    SessionEnd,
}

impl Default for NextStepsContext {
    fn default() -> Self {
        Self::GlobalContext
    }
}

/// UX persona the panel is rendered for. Controls prompt tone and the type
/// of suggestions the LLM focuses on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NextStepsMode {
    /// Non-technical operator — pragmatic, outcome-oriented suggestions.
    Operator,
    /// Builder/developer — technical vocabulary and observability-flavoured
    /// suggestions ("see logs", "export session").
    Builder,
}

impl Default for NextStepsMode {
    fn default() -> Self {
        Self::Operator
    }
}

/// Facts the frontend already computed — the LLM only narrates, it never
/// invents numbers. All fields are optional; an empty payload yields the
/// heuristic fallback.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NextStepsFacts {
    /// Last messages of the session (for [`NextStepsContext::SessionEnd`]).
    #[serde(default)]
    pub recent_messages: Vec<String>,
    /// Tool names used in the window (dedup + capped frontend-side).
    #[serde(default)]
    pub tools_used: Vec<String>,
    /// Number of memory entries created in the window.
    #[serde(default)]
    pub memories_created: u32,
    /// Number of memory entries recalled in the window.
    #[serde(default)]
    pub memories_recalled: u32,
    /// Count of recent inbox items awaiting action.
    #[serde(default)]
    pub inbox_pending: u32,
    /// Count of tasks that failed in the window.
    #[serde(default)]
    pub tasks_failed: u32,
    /// Count of tasks that completed successfully in the window.
    #[serde(default)]
    pub tasks_completed: u32,
    /// Count of automations currently misfiring (for global trends).
    #[serde(default)]
    pub automations_failing: u32,
    /// Free-form signals frontend wants to surface (e.g. "repeated similar
    /// task twice", "domain blocked: finance"). Capped server-side.
    #[serde(default)]
    pub signals: Vec<String>,
}

/// Action kinds the card button may carry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NextStepAction {
    /// Navigate to a whitelisted Apollia route.
    Navigate,
    /// Invoke a whitelisted Tauri command.
    Invoke,
}

/// Button rendered under each card.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NextStepButton {
    /// User-visible label (translated by the frontend when needed).
    pub label: String,
    /// Action kind — restricted to the whitelist above.
    pub action: NextStepAction,
    /// For `navigate`: `{"route": "/memory?new"}`. For `invoke`:
    /// `{"command": "memory_insert", "args": {...}}`.
    pub payload: serde_json::Value,
}

/// Single card displayed in the Next Steps panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NextStep {
    /// Stable identifier used by the frontend to persist dismiss state.
    /// Derived from `title` when the LLM does not provide one.
    pub id: String,
    /// Short, outcome-oriented card title.
    pub title: String,
    /// 1-2 sentence explanation of why this step is proposed.
    pub description: String,
    /// Single action button — the card has exactly one CTA.
    pub action_button: NextStepButton,
}

/// Request passed to [`generate_next_steps`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NextStepsRequest {
    /// Panel this payload targets.
    #[serde(default)]
    pub context: NextStepsContext,
    /// Persona — operator vs builder.
    #[serde(default)]
    pub mode: NextStepsMode,
    /// Frontend-computed facts.
    #[serde(default)]
    pub facts: NextStepsFacts,
}

/// Result returned to the caller. `from_llm` is `true` when the LLM path
/// succeeded; `false` when we fell back to heuristics.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextStepsResponse {
    pub steps: Vec<NextStep>,
    pub from_llm: bool,
}

/// Errors surfaced by [`generate_next_steps`]. Callers should not abort on
/// these — the helper already returns a heuristic fallback and never panics.
#[derive(Debug, thiserror::Error)]
pub enum NextStepsError {
    #[error("LLM call failed: {0}")]
    Llm(#[from] LlmError),
    #[error("LLM returned an empty response")]
    EmptyResponse,
    #[error("LLM returned malformed JSON")]
    MalformedJson,
}

// ─────────────────────────────────────────────
// Prompt assembly
// ─────────────────────────────────────────────

const PROMPT_HEADER: &str = "\
You are a productivity coach inside Apollia OS. From the provided facts, \
propose 2 to 3 concrete, immediately doable next steps the user can take \
INSIDE Apollia. Each step MUST reference a real Apollia route or Tauri \
command from the whitelist below — never invent routes.

Respond with a single JSON object matching EXACTLY this schema:
{
  \"steps\": [
    {
      \"title\": \"string (<= 60 chars)\",
      \"description\": \"string (<= 140 chars)\",
      \"action_button\": {
        \"label\": \"string (<= 28 chars)\",
        \"action\": \"navigate\" | \"invoke\",
        \"payload\": { /* route or command payload */ }
      }
    }
  ]
}

No prose, no markdown fence — JSON only.
";

fn allowed_routes_line() -> String {
    format!("Allowed routes: {}", ALLOWED_ROUTES.join(" "))
}

fn allowed_commands_line() -> String {
    format!("Allowed commands: {}", ALLOWED_COMMANDS.join(" "))
}

fn mode_line(mode: NextStepsMode) -> &'static str {
    match mode {
        NextStepsMode::Operator => {
            "Audience: operator (non-technical). Prefer pragmatic, outcome \
            oriented suggestions (capture memory, automate a routine, install \
            a helpful assistant)."
        }
        NextStepsMode::Builder => {
            "Audience: builder (developer). Technical suggestions welcome \
            (inspect logs, export session JSON, tune a pipeline)."
        }
    }
}

fn context_line(ctx: NextStepsContext) -> &'static str {
    match ctx {
        NextStepsContext::GlobalContext => {
            "Panel: operator Dashboard — rolling context of the last few days."
        }
        NextStepsContext::SessionEnd => {
            "Panel: session end — suggestions must relate to the session that \
            just closed."
        }
    }
}

fn build_facts_block(f: &NextStepsFacts) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("memories_created: {}", f.memories_created));
    lines.push(format!("memories_recalled: {}", f.memories_recalled));
    lines.push(format!("inbox_pending: {}", f.inbox_pending));
    lines.push(format!("tasks_completed: {}", f.tasks_completed));
    lines.push(format!("tasks_failed: {}", f.tasks_failed));
    lines.push(format!("automations_failing: {}", f.automations_failing));
    if !f.tools_used.is_empty() {
        let mut cap = f.tools_used.clone();
        cap.truncate(12);
        lines.push(format!("tools_used: {}", cap.join(", ")));
    }
    if !f.signals.is_empty() {
        let mut cap = f.signals.clone();
        cap.truncate(8);
        lines.push(format!("signals: {}", cap.join(" | ")));
    }
    if !f.recent_messages.is_empty() {
        // Guard against huge transcripts — take last 8, trim each.
        let tail = if f.recent_messages.len() > 8 {
            &f.recent_messages[f.recent_messages.len() - 8..]
        } else {
            &f.recent_messages[..]
        };
        let joined = tail
            .iter()
            .map(|m| {
                let mut s = m.trim().to_string();
                if s.len() > 240 {
                    s.truncate(240);
                    s.push('…');
                }
                s
            })
            .collect::<Vec<_>>()
            .join("\n- ");
        lines.push(format!("recent_messages:\n- {joined}"));
    }
    lines.join("\n")
}

fn build_system_prompt(req: &NextStepsRequest) -> String {
    format!(
        "{header}\n\n{mode}\n{ctx}\n\n{routes}\n{commands}\n\n## Facts\n{facts}",
        header = PROMPT_HEADER,
        mode = mode_line(req.mode),
        ctx = context_line(req.context),
        routes = allowed_routes_line(),
        commands = allowed_commands_line(),
        facts = build_facts_block(&req.facts),
    )
}

// ─────────────────────────────────────────────
// LLM output parsing + validation
// ─────────────────────────────────────────────

/// Pull a JSON object from a raw completion string. Models often wrap JSON in
/// ```json fences — strip them here.
fn extract_json_object(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if trimmed.starts_with('{') {
        return Some(trimmed);
    }
    // Look for the first `{` and the last matching `}` — good enough when
    // the LLM decorates the JSON with surrounding prose.
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end > start {
        Some(&trimmed[start..=end])
    } else {
        None
    }
}

fn slugify(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() || ch == '-' || ch == '_' {
            out.push('-');
        }
    }
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "next-step".to_string()
    } else {
        out
    }
}

fn payload_is_allowed(action: &NextStepAction, payload: &serde_json::Value) -> bool {
    match action {
        NextStepAction::Navigate => payload
            .get("route")
            .and_then(|v| v.as_str())
            .map(|r| ALLOWED_ROUTES.iter().any(|&allowed| r == allowed))
            .unwrap_or(false),
        NextStepAction::Invoke => payload
            .get("command")
            .and_then(|v| v.as_str())
            .map(|c| ALLOWED_COMMANDS.iter().any(|&allowed| c == allowed))
            .unwrap_or(false),
    }
}

fn parse_steps(raw: &str) -> Result<Vec<NextStep>, NextStepsError> {
    let json_str = extract_json_object(raw).ok_or(NextStepsError::MalformedJson)?;
    let parsed: serde_json::Value =
        serde_json::from_str(json_str).map_err(|_| NextStepsError::MalformedJson)?;
    let arr = parsed
        .get("steps")
        .and_then(|v| v.as_array())
        .ok_or(NextStepsError::MalformedJson)?;

    let mut out: Vec<NextStep> = Vec::new();
    for item in arr.iter().take(MAX_NEXT_STEPS) {
        let Some(title) = item.get("title").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(description) = item.get("description").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(btn) = item.get("action_button") else {
            continue;
        };
        let Some(label) = btn.get("label").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(action_str) = btn.get("action").and_then(|v| v.as_str()) else {
            continue;
        };
        let action = match action_str {
            "navigate" => NextStepAction::Navigate,
            "invoke" => NextStepAction::Invoke,
            _ => continue,
        };
        let payload = btn
            .get("payload")
            .cloned()
            .unwrap_or(serde_json::Value::Object(Default::default()));
        if !payload_is_allowed(&action, &payload) {
            continue;
        }
        out.push(NextStep {
            id: slugify(title),
            title: title.to_string(),
            description: description.to_string(),
            action_button: NextStepButton {
                label: label.to_string(),
                action,
                payload,
            },
        });
    }
    if out.is_empty() {
        return Err(NextStepsError::EmptyResponse);
    }
    Ok(out)
}

// ─────────────────────────────────────────────
// Heuristic fallback
// ─────────────────────────────────────────────

/// Deterministic 3-card fallback used whenever the LLM path fails, times
/// out, or returns invalid JSON. The suggestions are intentionally generic
/// — they always apply and always point to a valid route.
pub fn heuristic_fallback(req: &NextStepsRequest) -> Vec<NextStep> {
    let mut steps: Vec<NextStep> = Vec::new();

    // (1) No memories yet → steer the user to capture one.
    if req.facts.memories_created == 0 {
        steps.push(NextStep {
            id: "capture-note".into(),
            title: "Enregistrer une note".into(),
            description:
                "Gardez une trace utile de ce que vous venez de faire dans votre mémoire Apollia."
                    .into(),
            action_button: NextStepButton {
                label: "Ouvrir Mémoire".into(),
                action: NextStepAction::Navigate,
                payload: serde_json::json!({ "route": "/memory?new" }),
            },
        });
    }

    // (2) Repeated completion → propose an automation.
    if req.facts.tasks_completed >= 2 || !req.facts.signals.is_empty() {
        steps.push(NextStep {
            id: "create-automation".into(),
            title: "Créer une automatisation".into(),
            description:
                "Transformez une routine en automatisation pour que Apollia la relance toute seule."
                    .into(),
            action_button: NextStepButton {
                label: "Ouvrir l'assistant".into(),
                action: NextStepAction::Navigate,
                payload: serde_json::json!({ "route": "/automations?wizard=open" }),
            },
        });
    }

    // (3) Always-on: ask Apollia Guide.
    steps.push(NextStep {
        id: "ask-apollia".into(),
        title: "Demander à Apollia".into(),
        description: "Apollia Guide vous aide à trouver la prochaine étape utile.".into(),
        action_button: NextStepButton {
            label: "Ouvrir Apollia".into(),
            action: NextStepAction::Navigate,
            payload: serde_json::json!({ "route": "/chat" }),
        },
    });

    // Builder-only bonus: keep 3 total, swap in an observability suggestion
    // when we still have room.
    if matches!(req.mode, NextStepsMode::Builder) && steps.len() < MAX_NEXT_STEPS {
        steps.push(NextStep {
            id: "view-logs".into(),
            title: "Voir les logs".into(),
            description: "Inspectez la timeline et les métriques de vos agents.".into(),
            action_button: NextStepButton {
                label: "Observabilité".into(),
                action: NextStepAction::Navigate,
                payload: serde_json::json!({ "route": "/observability" }),
            },
        });
    }

    steps.truncate(MAX_NEXT_STEPS);
    steps
}

// ─────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────

/// Produce up to 3 Next-Step cards for the given context.
///
/// Always returns a non-empty `NextStepsResponse`. On any LLM failure the
/// heuristic fallback is substituted and `from_llm` is set to `false`.
pub async fn generate_next_steps(
    router: &Arc<LlmRouter>,
    request: NextStepsRequest,
) -> NextStepsResponse {
    let Some(backend) = router.get(None) else {
        return NextStepsResponse {
            steps: heuristic_fallback(&request),
            from_llm: false,
        };
    };

    let system = build_system_prompt(&request);
    let user = match request.context {
        NextStepsContext::GlobalContext => {
            "Propose 2-3 Next Steps for the operator Dashboard.".to_string()
        }
        NextStepsContext::SessionEnd => {
            "Propose 2-3 Next Steps for the session that just ended.".to_string()
        }
    };

    let req = CompletionRequest {
        messages: vec![ChatMessage::system(system), ChatMessage::user(user)],
        temperature: Some(0.4),
        max_tokens: Some(420),
        ..Default::default()
    };

    let call = backend.complete(req);
    let response = match tokio::time::timeout(CALL_TIMEOUT, call).await {
        Ok(Ok(r)) => r,
        _ => {
            return NextStepsResponse {
                steps: heuristic_fallback(&request),
                from_llm: false,
            };
        }
    };

    let raw = response.content.trim();
    if raw.is_empty() {
        return NextStepsResponse {
            steps: heuristic_fallback(&request),
            from_llm: false,
        };
    }

    match parse_steps(raw) {
        Ok(steps) => NextStepsResponse {
            steps,
            from_llm: true,
        },
        Err(_) => NextStepsResponse {
            steps: heuristic_fallback(&request),
            from_llm: false,
        },
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_three_valid_steps() {
        let raw = r#"{"steps":[
            {"title":"Enregistrer conclusions","description":"Capture en mémoire.",
             "action_button":{"label":"Mémoire","action":"invoke","payload":{"command":"memory_insert"}}},
            {"title":"Créer une automatisation","description":"Routine récurrente.",
             "action_button":{"label":"Automations","action":"navigate","payload":{"route":"/automations?wizard=open"}}},
            {"title":"Installer Review","description":"Assistant spécialisé.",
             "action_button":{"label":"Templates","action":"navigate","payload":{"route":"/templates?filter=agents"}}}
        ]}"#;
        let steps = parse_steps(raw).expect("parse");
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].id, "enregistrer-conclusions");
        assert!(matches!(steps[0].action_button.action, NextStepAction::Invoke));
    }

    #[test]
    fn parse_rejects_unknown_route() {
        let raw = r#"{"steps":[
            {"title":"Bad","description":"nope",
             "action_button":{"label":"X","action":"navigate","payload":{"route":"/evil"}}}
        ]}"#;
        let err = parse_steps(raw).unwrap_err();
        assert!(matches!(err, NextStepsError::EmptyResponse));
    }

    #[test]
    fn parse_rejects_unknown_command() {
        let raw = r#"{"steps":[
            {"title":"Bad","description":"nope",
             "action_button":{"label":"X","action":"invoke","payload":{"command":"rm_rf"}}}
        ]}"#;
        assert!(parse_steps(raw).is_err());
    }

    #[test]
    fn parse_caps_at_three() {
        let raw = r#"{"steps":[
            {"title":"a","description":"d","action_button":{"label":"l","action":"navigate","payload":{"route":"/inbox"}}},
            {"title":"b","description":"d","action_button":{"label":"l","action":"navigate","payload":{"route":"/agents"}}},
            {"title":"c","description":"d","action_button":{"label":"l","action":"navigate","payload":{"route":"/chat"}}},
            {"title":"d","description":"d","action_button":{"label":"l","action":"navigate","payload":{"route":"/tasks"}}}
        ]}"#;
        let steps = parse_steps(raw).expect("parse");
        assert_eq!(steps.len(), 3);
    }

    #[test]
    fn parse_extracts_json_from_fence_decorated_output() {
        let raw = "```json\n{\"steps\":[\
            {\"title\":\"Ouvrir mémoire\",\"description\":\"…\",\
            \"action_button\":{\"label\":\"Go\",\"action\":\"navigate\",\"payload\":{\"route\":\"/memory\"}}}\
        ]}\n```";
        let steps = parse_steps(raw).expect("parse");
        assert_eq!(steps.len(), 1);
    }

    #[test]
    fn fallback_always_yields_capped_non_empty_steps() {
        let req = NextStepsRequest::default();
        let steps = heuristic_fallback(&req);
        assert!(!steps.is_empty());
        assert!(steps.len() <= MAX_NEXT_STEPS);
    }

    #[test]
    fn fallback_builder_mode_includes_observability_hint() {
        let req = NextStepsRequest {
            mode: NextStepsMode::Builder,
            ..Default::default()
        };
        let steps = heuristic_fallback(&req);
        assert!(steps.iter().any(|s| s.id == "view-logs" || s.id == "ask-apollia"));
    }

    #[test]
    fn slugify_is_ascii_and_hyphenated() {
        assert_eq!(slugify("Enregistrer les conclusions !"), "enregistrer-les-conclusions");
        assert_eq!(slugify(""), "next-step");
    }

    #[test]
    fn prompt_includes_allowed_routes_and_facts() {
        let req = NextStepsRequest {
            mode: NextStepsMode::Operator,
            context: NextStepsContext::GlobalContext,
            facts: NextStepsFacts {
                memories_created: 2,
                tools_used: vec!["web_search".into()],
                ..Default::default()
            },
        };
        let prompt = build_system_prompt(&req);
        assert!(prompt.contains("/automations?wizard=open"));
        assert!(prompt.contains("memories_created: 2"));
        assert!(prompt.contains("web_search"));
    }
}
