//! The in-app companion: the per-route help text it opens with, whether it is
//! on, and the chat session it runs against the guide agent.
//!
//! The help text is a table compiled into this window, so it is answered here
//! in both modes. The preference and the session belong to the runtime:
//! embedded, the commands call [`apollia_runtime::companion`] directly;
//! attached, they reach the same functions through `/api/v1/companion/...` on
//! the daemon.

use apollia_runtime::api::routes_companion::{
    CompanionEnabled, CompanionSession, CreateCompanionSessionBody,
};
use apollia_runtime::companion;
use apollia_runtime::profile::ProfileStore;
use tauri::State;

use crate::attached::{AttachedClient, AttachedError, RuntimeLink};

// ---------------------------------------------------------------------------
// Companion - context table
// ---------------------------------------------------------------------------

/// Per-route contextual help texts displayed in the Companion panel.
const COMPANION_CONTEXTS: &[(&str, &str)] = &[
    ("dashboard", "You are on the dashboard. It gives you an overview of your active agents, the recent events and the state of the system. Ask me anything about running your agents."),
    ("agents", "You are on the Agents page. Each agent has a card: its mission, the requests it handles, the model it uses and what it may access. Install a package here to add agents and workflows."),
    ("chat", "You are on the Chat page. You can talk to a local model or work with your agents through the free chat. Sessions are stored on this machine."),
    ("triggers", "You are on the Triggers page. Triggers start agents on their own, on a cron schedule, on an interval, when a file changes, or on an incoming webhook."),
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

/// Reads whether the companion is on, from the daemon.
pub(crate) async fn enabled_on(remote: &AttachedClient) -> Result<bool, AttachedError> {
    let answer: CompanionEnabled = remote.get("/api/v1/companion/enabled").await?;
    Ok(answer.enabled)
}

/// Records on the daemon whether the companion is on.
pub(crate) async fn set_enabled_on(
    remote: &AttachedClient,
    enabled: bool,
) -> Result<(), AttachedError> {
    let _: CompanionEnabled = remote
        .put("/api/v1/companion/enabled", &CompanionEnabled { enabled })
        .await?;
    Ok(())
}

/// Opens a guide session on the daemon.
pub(crate) async fn create_session_on(
    remote: &AttachedClient,
    context: Option<String>,
) -> Result<CompanionSession, AttachedError> {
    remote
        .post(
            "/api/v1/companion/sessions",
            &CreateCompanionSessionBody { context },
        )
        .await
}

/// Creates a companion session backed by the apollia-guide agent.
///
/// The session runs in Agent mode so the panel is driven by the single
/// product coach (knowledge base and action buttons), and its identifier is
/// recorded in the onboarding state. A guide that is not installed is
/// reported as such, so the panel shows an install hint instead of an opaque
/// failure.
#[tauri::command]
pub async fn create_companion_session(
    context: Option<String>,
    link: State<'_, RuntimeLink>,
) -> Result<CompanionSession, String> {
    let created = match link.inner() {
        RuntimeLink::Embedded(state) => companion::create_session(
            state.chat_manager.as_ref(),
            state.user_memory.as_ref().map(ProfileStore),
            context,
        )
        .await
        .map(|session_id| CompanionSession { session_id })
        .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => create_session_on(remote, context)
            .await
            .map_err(String::from),
    };
    created.inspect_err(|error| {
        tracing::event!(
            tracing::Level::ERROR,
            error = %error,
            "companion.session.create.failed"
        );
    })
}

/// Persists the companion-enabled preference to UserMemory internal state.
///
/// Stored under the internal-state key `companion_enabled` (auto-prefixed
/// with `__` so it stays out of the user profile listing).
#[tauri::command]
pub async fn set_companion_enabled(
    enabled: bool,
    link: State<'_, RuntimeLink>,
) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => {
            companion::write_enabled(state.user_memory.as_ref().map(ProfileStore), enabled)
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeLink::Attached(remote) => Ok(set_enabled_on(remote, enabled).await?),
    }
}

/// Reads the companion-enabled preference from UserMemory internal state.
/// Defaults to `false` when the key has never been written.
#[tauri::command]
pub async fn get_companion_enabled(link: State<'_, RuntimeLink>) -> Result<bool, String> {
    match link.inner() {
        RuntimeLink::Embedded(state) => {
            companion::read_enabled(state.user_memory.as_ref().map(ProfileStore))
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeLink::Attached(remote) => Ok(enabled_on(remote).await?),
    }
}

#[cfg(test)]
mod attached_tests;

#[cfg(test)]
mod companion_tests {
    use super::*;

    #[test]
    fn test_companion_context_for_all_routes() {
        // GIVEN the 14 defined routes
        let routes = [
            "dashboard",
            "agents",
            "chat",
            "triggers",
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
    fn test_companion_contexts_describe_no_retired_contract_or_missing_page() {
        // GIVEN the per-route Companion blurbs
        // WHEN they are searched for the agent contract the bridge refuses and
        // for the Pipelines page the application does not have
        // THEN neither is there
        assert!(COMPANION_CONTEXTS
            .iter()
            .all(|(route, _)| *route != "pipelines"));
        for (route, ctx) in COMPANION_CONTEXTS {
            assert!(!ctx.contains("manifest()"), "{route}: {ctx}");
            assert!(!ctx.contains("Pipelines"), "{route}: {ctx}");
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
