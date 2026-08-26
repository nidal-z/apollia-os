//! Router-facing verbs: reload, status, ping and the one-shot chat probe.

use crate::client::{ClientError, RuntimeClient};
use crate::exit_codes;

use super::{handle_error, handle_server_error};

/// `apollia-os llm reload`: rebuild the in-memory router from `system.db`.
///
/// The mutating sub-commands (`create`, `update`, `delete`, `set-default`)
/// invoke this automatically. The standalone command is useful when the
/// operator edited `system.db` directly, restored a backup, or wants to
/// retry after a transient model load failure.
pub(super) async fn run_reload(client: &RuntimeClient, json: bool) -> i32 {
    match client.reload_llm_router().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
                return exit_codes::SUCCESS;
            }
            let default = resp.get("default").and_then(|v| v.as_str()).unwrap_or("");
            let count = resp
                .get("backends")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            println!("OK LLM router reloaded ({count} backend(s) active, default: {default})");
            let reaches = resp
                .get("reaches_running_agents")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            if !reaches {
                eprintln!(
                    "Note: agents already running keep the router they started with. \
                     Restart the daemon for them to pick this up."
                );
            }
            exit_codes::SUCCESS
        }
        Err(ClientError::ConnectionRefused) => {
            let code = crate::output::emit_error(
                json,
                exit_codes::RUNTIME_ERROR,
                "runtime not started (connection refused)",
            );
            if !json {
                eprintln!("Hint: run `apollia-os start` first.");
            }
            code
        }
        Err(ClientError::ServerError { status, body }) => {
            let code = crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("{body} (status {status})"),
            );
            if !json && status == 503 {
                eprintln!("Hint: configure at least one backend with");
                eprintln!("      `apollia-os llm backends create ... --default`");
            }
            code
        }
        Err(e) => handle_error(e, json),
    }
}

/// `apollia-os llm status`: display all LLM backends with their current state.
pub(super) async fn run_status(client: &RuntimeClient, json: bool) -> i32 {
    let resp = match client.get("/api/v1/llm/status").await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("invalid JSON response: {e}"),
            );
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        format_llm_status(&parsed);
    }
    exit_codes::SUCCESS
}

/// `apollia-os llm ping [backend]`: measure the latency of a backend.
///
/// Returns exit code `0` if the backend is available, `1` otherwise.
pub(super) async fn run_ping(client: &RuntimeClient, backend: Option<&str>, json: bool) -> i32 {
    let body = serde_json::json!({ "backend": backend });
    let resp = match client.post("/api/v1/llm/ping", Some(&body)).await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("invalid JSON response: {e}"),
            );
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        format_ping_result(&parsed);
    }

    let available = parsed
        .get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if available {
        exit_codes::SUCCESS
    } else {
        exit_codes::GENERAL_ERROR
    }
}

/// `apollia-os llm chat "prompt"`: send a prompt to an LLM backend.
pub(super) async fn run_chat(
    client: &RuntimeClient,
    prompt: &str,
    backend: Option<&str>,
    json: bool,
) -> i32 {
    let body = serde_json::json!({ "prompt": prompt, "backend": backend });
    let resp = match client.post("/api/v1/llm/chat", Some(&body)).await {
        Ok(r) => r,
        Err(e) => return handle_error(e, json),
    };

    if resp.status >= 400 {
        return handle_server_error(resp.status, &resp.body, json);
    }

    let parsed: serde_json::Value = match serde_json::from_str(&resp.body) {
        Ok(v) => v,
        Err(e) => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("invalid JSON response: {e}"),
            );
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        );
    } else {
        let content = parsed.get("content").and_then(|v| v.as_str()).unwrap_or("");
        println!("{content}");
    }
    exit_codes::SUCCESS
}

// ─────────────────────────────────────────────
// Formatting helpers
// ─────────────────────────────────────────────

/// Render `GET /api/v1/llm/status` response as a human-readable table.
pub(super) fn format_llm_status(resp: &serde_json::Value) {
    let backends = resp
        .get("backends")
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default();

    println!("  {:<24} {:<32} STATUS", "BACKEND", "MODEL");
    if backends.is_empty() {
        println!("  (no LLM backends configured)");
    } else {
        for b in &backends {
            let name = b.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let model = b.get("model_id").and_then(|v| v.as_str()).unwrap_or("?");
            let available = b
                .get("available")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let status = if available { "ready" } else { "unavailable" };
            println!("  {name:<24} {model:<32} {status}");
        }
    }

    // Cost ceiling line: shown only when the runtime reports a session cost.
    // `ceiling_usd` is absent without hybrid routing, in which case the line
    // states that no ceiling is configured rather than printing a phantom value.
    if let Some(cost) = resp.get("cost_usd").and_then(serde_json::Value::as_f64) {
        match resp.get("ceiling_usd").and_then(serde_json::Value::as_f64) {
            Some(ceiling) => {
                println!("  session cost: {cost:.2} USD / {ceiling:.2} USD ceiling");
                let reached = resp
                    .get("ceiling_reached")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                if reached {
                    println!(
                        "  cost ceiling reached: the run stops cleanly when ceiling_action = hard_stop"
                    );
                }
            }
            None => {
                println!("  session cost: {cost:.2} USD (no hybrid cost ceiling configured)");
            }
        }
    }
}

/// Render `POST /api/v1/llm/ping` response as a human-readable line.
pub(super) fn format_ping_result(resp: &serde_json::Value) {
    let backend = resp.get("backend").and_then(|v| v.as_str()).unwrap_or("?");
    let available = resp
        .get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if available {
        let latency = resp.get("latency_ms").and_then(|v| v.as_u64()).unwrap_or(0);
        println!("{backend}: OK ({latency}ms)");
    } else {
        let error = resp
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        println!("{backend}: UNAVAILABLE ({error})");
    }
}
