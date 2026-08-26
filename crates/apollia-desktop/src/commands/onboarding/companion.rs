//! The in-app companion: the per-route help text it opens with, and the chat
//! session it runs against the guide agent.

use apollia_runtime::chat::ChatMode;
use apollia_runtime::embedded::RuntimeHandle;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::memory::{get_repo, load_state_from_memory, persist_state, read_bool, write_bool};
use super::state::OnboardingError;
use super::GUIDE_AGENT_NAME;

// ---------------------------------------------------------------------------
// Companion - context table
// ---------------------------------------------------------------------------

/// Per-route contextual help texts displayed in the Companion panel.
const COMPANION_CONTEXTS: &[(&str, &str)] = &[
    ("dashboard", "You are on the dashboard. It gives you an overview of your active agents, the recent events and the state of the system. Ask me anything about running your agents."),
    ("agents", "You are on the Agents page. You can create, configure and watch your AI agents here. Each agent is a Python module exposing manifest() and run()."),
    ("chat", "You are on the Chat page. You can talk to a local model or work with your agents through the free chat. Sessions are stored on this machine."),
    ("triggers", "You are on the Triggers page. Triggers start agents on their own, on a cron schedule, on an interval, when a file changes, or on an incoming webhook."),
    ("pipelines", "You are on the Pipelines page. Pipelines chain several agents in sequence or in parallel, with a DAG topology, fan-out and fan-in, and human checkpoints."),
    ("memory", "You are on the Memory page. Apollia keeps three kinds of memory: episodic for events, semantic for knowledge, procedural for know-how. Full-text search is built in."),
    ("integrations", "You are on the MCP integrations page. Connect external MCP servers to give your agents new tools over the standard JSON-RPC 2.0 protocol."),
    ("approvals", "You are on the Approvals page. Sensitive agent actions can require your go-ahead before they run, which is the human-in-the-loop safeguard."),
    ("observability", "You are on the Observability page. Follow your agents' execution traces, their performance metrics and the structured logs as they happen."),
    ("notifications", "You are on the Notifications page. Set up desktop alerts and webhooks so you hear about the events that matter on your agents."),
    ("transcriptions", "You are on the Transcriptions page. Read back the audio transcripts produced by the built-in Whisper speech engine."),
    ("llm", "You are on the LLM page. Configure the language model backends your agents use: local llama.cpp, Ollama, or a cloud API such as Anthropic or OpenAI."),
    ("settings", "You are on the Settings page. Configure Apollia's global options: paths, logs, resource limits and your own preferences."),
    ("onboarding", "You are in the middle of onboarding. I am here to walk you through Apollia. Ask me anything at any step."),
];

/// Generic fallback when no route-specific context is available.
const COMPANION_CONTEXT_FALLBACK: &str =
    "I am your Apollia assistant. Ask me anything about the application, your agents, or what it can do.";

/// Returns the contextual help text for the given application route.
///
/// Falls back to a generic message for unknown routes.
pub fn get_companion_context_text(route: &str) -> &'static str {
    COMPANION_CONTEXTS
        .iter()
        .find(|(r, _)| *r == route)
        .map(|(_, ctx)| *ctx)
        .unwrap_or(COMPANION_CONTEXT_FALLBACK)
}

/// Return payload for companion session creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionSessionResult {
    /// Unique identifier of the newly created companion session.
    pub session_id: String,
}

// ---------------------------------------------------------------------------
// Companion - Tauri commands
// ---------------------------------------------------------------------------

/// Returns the contextual help text for the given application route.
///
/// Exposes [`get_companion_context_text`] over IPC so the frontend can
/// pre-load context before opening the panel.
#[tauri::command]
pub async fn get_companion_context(route: String) -> Result<String, String> {
    Ok(get_companion_context_text(&route).to_string())
}

/// Creates a companion session backed by the apollia-guide agent.
///
/// The session runs in Agent mode so the panel is driven by the single
/// product coach (knowledge base, live environment listing, action buttons).
/// The session is persisted normally through the chat subsystem. On success,
/// the `session_id` is stored in the onboarding state so other commands can
/// reference the active companion session.
#[tauri::command]
pub async fn create_companion_session(
    context: Option<String>,
    state: State<'_, RuntimeHandle>,
) -> Result<CompanionSessionResult, String> {
    create_companion_session_inner(context, &state)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "companion.session.create.failed");
            e.to_string()
        })
}

async fn create_companion_session_inner(
    context: Option<String>,
    state: &RuntimeHandle,
) -> Result<CompanionSessionResult, OnboardingError> {
    // The companion panel is powered by the apollia-guide agent: a
    // knowledge-base-grounded coach that inspects the live environment and
    // suggests allowlisted deep-links. Verify it is registered first so the
    // panel can surface a clear install hint instead of an opaque failure.
    let found = state
        .registry_handle
        .find_by_name(GUIDE_AGENT_NAME)
        .await
        .map_err(|_| OnboardingError::GuideAgentNotInstalled)?;
    if found.is_none() {
        return Err(OnboardingError::GuideAgentNotInstalled);
    }

    let manager = state
        .chat_manager
        .as_ref()
        .ok_or(OnboardingError::ChatNotAvailable)?;

    // Agent mode: the agent owns its own system prompt, environment listing,
    // and action-block contract, so no caller prompt is supplied. The
    // `context` route hint is retained on the IPC surface for a future
    // page-aware enhancement but is not injected here.
    let info = manager
        .create_session(apollia_runtime::chat::manager::CreateSessionParams {
            mode: ChatMode::Agent,
            agent_name: Some(GUIDE_AGENT_NAME.to_string()),
            system_prompt: None,
            tools: Vec::new(),
            project_id: None,
        })
        .await
        .map_err(|e| OnboardingError::SessionCreationFailed(e.to_string()))?;

    let session_id = info.id.clone();

    // Best-effort: persist the companion session id in the onboarding state.
    if let Ok(repo) = get_repo(state) {
        let sid = session_id.clone();
        let _ = tokio::task::spawn_blocking(move || {
            if let Ok(repo) = repo.lock() {
                let mut ob_state = load_state_from_memory(&repo).unwrap_or_default();
                ob_state.companion_session_id = Some(sid);
                let _ = persist_state(&repo, &ob_state);
            }
        })
        .await;
    }

    tracing::info!(
        session_id = %session_id,
        agent = %GUIDE_AGENT_NAME,
        context = ?context,
        "companion.session.created"
    );

    Ok(CompanionSessionResult { session_id })
}

/// Persists the companion-enabled preference to UserMemory internal state.
///
/// Stored under the internal-state key `companion_enabled` (auto-prefixed
/// with `__` so it stays out of the user profile listing).
#[tauri::command]
pub async fn set_companion_enabled(
    enabled: bool,
    state: State<'_, RuntimeHandle>,
) -> Result<(), String> {
    let repo = get_repo(&state).map_err(|e| e.to_string())?;
    tokio::task::spawn_blocking(move || {
        let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;
        write_bool(&repo, "companion_enabled", enabled).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

/// Reads the companion-enabled preference from UserMemory internal state.
/// Defaults to `false` when the key has never been written.
#[tauri::command]
pub async fn get_companion_enabled(state: State<'_, RuntimeHandle>) -> Result<bool, String> {
    let repo = get_repo(&state).map_err(|e| e.to_string())?;
    tokio::task::spawn_blocking(move || {
        let repo = repo.lock().map_err(|e| format!("mutex poisoned: {e}"))?;
        read_bool(&repo, "companion_enabled").map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

#[cfg(test)]
mod companion_tests {
    use super::*;

    #[test]
    fn test_companion_context_for_all_routes() {
        // GIVEN the 15 defined routes
        let routes = [
            "dashboard",
            "agents",
            "chat",
            "triggers",
            "pipelines",
            "memory",
            "integrations",
            "approvals",
            "observability",
            "notifications",
            "transcriptions",
            "llm",
            "settings",
            "onboarding",
        ];
        // WHEN getting context for each route
        // THEN all return non-empty strings
        for route in routes {
            let ctx = get_companion_context_text(route);
            assert!(
                !ctx.is_empty(),
                "context for route '{}' should not be empty",
                route
            );
        }
    }

    #[test]
    fn test_companion_context_unknown_route() {
        // GIVEN an unknown route
        let ctx = get_companion_context_text("nonexistent");
        // WHEN the companion context of that route is asked for
        // THEN a non-empty fallback is returned
        assert!(!ctx.is_empty());
        assert_eq!(ctx, COMPANION_CONTEXT_FALLBACK);
    }

    #[test]
    fn test_companion_context_all_routes_distinct() {
        // GIVEN the defined route table
        // WHEN extracting all context strings
        let mut seen = std::collections::HashSet::new();
        for (_, ctx) in COMPANION_CONTEXTS {
            // THEN each route has a unique context text
            assert!(seen.insert(*ctx), "duplicate context text: {ctx}");
        }
    }

    #[test]
    fn test_companion_contexts_are_written_in_one_language() {
        // GIVEN the per-route Companion blurbs
        //
        // WHEN each is read
        //
        // THEN it reads in the codebase language. These strings are not
        // translated anywhere: `get_companion_context` hands them to the
        // frontend, which pushes them into the Companion session's system
        // prompt, so a French blurb makes the model answer in French inside an
        // English window. The assertion is on the opening words rather than on
        // a non-ASCII scan, because the old fallback ("Je suis votre assistant
        // Apollia...") was pure ASCII and a scan would have waved it through.
        for (route, ctx) in COMPANION_CONTEXTS {
            assert!(
                ctx.starts_with("You are "),
                "context for route '{route}' does not open in English: {ctx}"
            );
        }
        assert!(COMPANION_CONTEXT_FALLBACK.starts_with("I am your Apollia assistant"));

        // The negative case the scan would miss, spelled out so this test is
        // known to be able to fail.
        assert!(!"Je suis votre assistant Apollia.".starts_with("I am your Apollia assistant"));
    }
}
