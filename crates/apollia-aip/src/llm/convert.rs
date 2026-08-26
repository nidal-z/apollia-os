//! Conversion between the Python payloads and the LLM crate's types.
//!
//! Split out of `llm.rs`: the proxy stays in the parent, the readers that turn
//! a Python dict into a `ChatMessage` or a `ToolSpec`, and the context blocks
//! prepended to a system prompt, live here.

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

use apollia_llm::types::{ChatMessage, LlmError, MessageContent, Role, ToolSpec};

/// Render the host-environment block for the bridge layer.
///
/// Gathering lives in `apollia-tools`, rendering in `apollia-prompts`; the
/// workspace root is not known at this layer, so the block reports the
/// generic session-root wording.
pub(super) fn host_environment_block() -> String {
    let env = apollia_tools::host_env::gather_host_environment(None);
    let shell = env.posix_shell.as_ref().map(|p| p.display().to_string());
    apollia_prompts::blocks::environment_block(
        env.os,
        env.os_version.as_deref(),
        env.arch,
        shell.as_deref(),
        None,
    )
}
/// Prepend the temporal block and the host-environment block to `system`.
///
/// Both are authoritative "here and now" facts and sit at the top, in that
/// order, mirroring Chat Libre's `build_system_prompt`.
pub(super) fn prepend_context_blocks(system: &str) -> String {
    apollia_core::temporal_context::prepend_temporal_context(&format!(
        "{}\n\n{}",
        host_environment_block(),
        system
    ))
}
/// Inject the authoritative temporal + host-environment blocks into a
/// multi-turn message list. If the first message is a `system` role, prepend
/// the blocks to its text. Otherwise insert a brand-new `system` message at
/// index 0.
///
/// Keeps Python agents on the real wall clock for "today"/"now"/relative
/// dates, and on the real host OS for shell and path decisions, without
/// requiring the agent author to remember anything.
pub(super) fn inject_temporal_context_into_messages(
    mut messages: Vec<ChatMessage>,
) -> Vec<ChatMessage> {
    use apollia_llm::types::{MessageContent, Role};

    let is_first_system = messages
        .first()
        .map(|m| matches!(m.role, Role::System))
        .unwrap_or(false);

    if is_first_system {
        if let Some(first) = messages.first_mut() {
            if let MessageContent::Text(ref text) = first.content {
                let merged = prepend_context_blocks(text);
                first.content = MessageContent::Text(merged);
            }
            // Non-text system content (rare): leave untouched and prepend
            // a fresh system block ahead of it as a safety net.
        }
    } else {
        messages.insert(
            0,
            ChatMessage::system(format!(
                "{}\n{}",
                apollia_core::temporal_context::temporal_context_block(),
                host_environment_block()
            )),
        );
    }
    messages
}
/// Converts a Python dict `{"role": "...", "content": ...}` into a `ChatMessage`.
///
/// `content` can be:
/// - a `str` (text fast path);
/// - a `list[dict]` of typed blocks (vision typing), each block being either
///   `{"type": "text", "text": "..."}` or
///   `{"type": "image", "source": {"type": "base64"|"url", ...}}`.
///   Text blocks are concatenated (joined with `\n\n`). Image blocks are
///   annotated as `[image: <media_type|url>]` because no LLM backend reachable
///   via `LlmProxy` supports vision today (the local engine is text-only).
///   The JSON shape is preserved as received so a future cloud vision backend
///   can be wired in without breaking the API.
///
/// Returns `PyValueError` if `role` is missing or matches no known role
/// (`system` / `user` / `assistant` / `tool`).
pub(super) fn py_dict_to_chat_message(py: Python<'_>, obj: &PyObject) -> PyResult<ChatMessage> {
    let bound = obj.bind(py);

    let role_str: String = bound
        .get_item("role")
        .map_err(|_| PyValueError::new_err("message dict missing 'role' key"))?
        .extract()
        .map_err(|e| PyValueError::new_err(format!("'role' must be a str: {e}")))?;

    let content_obj = bound
        .get_item("content")
        .map_err(|_| PyValueError::new_err("message dict missing 'content' key"))?;

    // Fast path: plain string content.
    let content_str: String = match content_obj.extract::<String>() {
        Ok(s) => s,
        Err(_) => {
            // Slow path: list[MessageContent], flatten to text representation.
            flatten_multimodal_content(py, &content_obj)?
        }
    };

    let role = match role_str.as_str() {
        "system" => Role::System,
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "tool" => Role::Tool,
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown role '{other}' - expected system/user/assistant/tool"
            )))
        }
    };

    Ok(ChatMessage {
        role,
        content: MessageContent::Text(content_str),
        cache_control: None,
    })
}
/// Flattens a list of multimodal content blocks (TextContent / ImageContent)
/// to a textual representation suitable for text-only backends.
///
/// - `{"type": "text", "text": "..."}` ⇒ raw text fragment.
/// - `{"type": "image", "source": {...}}` ⇒ `[image: <descriptor>]` marker
///   where descriptor is the `media_type` for base64 sources or the truncated
///   URL for URL sources.
///
/// Returns `PyValueError` if a block is malformed (missing `type`, unknown
/// kind, or non-iterable input).
pub(super) fn flatten_multimodal_content(
    py: Python<'_>,
    content: &Bound<'_, PyAny>,
) -> PyResult<String> {
    use pyo3::types::PyList;

    if !content.is_instance_of::<PyList>() {
        return Err(PyValueError::new_err(
            "'content' must be either a str or a list of content blocks",
        ));
    }
    let list = content
        .downcast::<PyList>()
        .map_err(|e| PyValueError::new_err(format!("'content' is not a valid list: {e}")))?;

    let json_mod = py
        .import("json")
        .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;

    let mut fragments: Vec<String> = Vec::with_capacity(list.len());
    for item in list.iter() {
        // Each block must be a dict with a 'type' key.
        let type_str: String = item
            .get_item("type")
            .map_err(|_| PyValueError::new_err("content block missing 'type' key"))?
            .extract()
            .map_err(|e| PyValueError::new_err(format!("'type' must be a str: {e}")))?;

        match type_str.as_str() {
            "text" => {
                let text: String = item
                    .get_item("text")
                    .map_err(|_| PyValueError::new_err("text block missing 'text' key"))?
                    .extract()
                    .map_err(|e| PyValueError::new_err(format!("'text' must be a str: {e}")))?;
                fragments.push(text);
            }
            "image" => {
                // Pull a short descriptor from `source` for the placeholder. The
                // full JSON is also captured (truncated) so a future vision-capable
                // backend can reconstruct the payload if we expose a richer
                // MessageContent variant later.
                let source = item
                    .get_item("source")
                    .map_err(|_| PyValueError::new_err("image block missing 'source' key"))?;
                let descriptor = source
                    .get_item("media_type")
                    .ok()
                    .and_then(|v| v.extract::<String>().ok())
                    .or_else(|| {
                        source
                            .get_item("url")
                            .ok()
                            .and_then(|v| v.extract::<String>().ok())
                            .map(|u| truncate(&u, 80))
                    })
                    .unwrap_or_else(|| "unknown".to_string());
                fragments.push(format!("[image: {descriptor}]"));
                // Tracing: let operators discover that an image was sent to a
                // text-only backend so they can switch to a cloud vision model.
                tracing::warn!(
                    media = %descriptor,
                    detail = "image flattened to a placeholder, the current backends are text-only",
                    "llm.proxy.vision.unsupported"
                );
                // Drop the dumped JSON if we ever need it for debugging.
                let _ = json_mod;
            }
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown content block type '{other}' - expected 'text' or 'image'"
                )));
            }
        }
    }

    Ok(fragments.join("\n\n"))
}
/// Truncates a string to at most `max_chars` characters with an ellipsis.
pub(super) fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push('…');
    out
}
/// Converts a Python dict `{"name": "...", "description": "...", "parameters": {...}}`
/// into a `ToolSpec`.
///
/// `parameters` is optional; defaults to `{}` when absent.
/// Returns `PyValueError` if `name` or `description` is missing.
pub(super) fn py_dict_to_tool_spec(py: Python<'_>, obj: &PyObject) -> PyResult<ToolSpec> {
    let bound = obj.bind(py);

    let name: String = bound
        .get_item("name")
        .map_err(|_| PyValueError::new_err("tool spec dict missing 'name' key"))?
        .extract()
        .map_err(|e| PyValueError::new_err(format!("'name' must be a str: {e}")))?;

    let description: String = bound
        .get_item("description")
        .map_err(|_| PyValueError::new_err("tool spec dict missing 'description' key"))?
        .extract()
        .map_err(|e| PyValueError::new_err(format!("'description' must be a str: {e}")))?;

    // `parameters` is optional; serialize via json.dumps to handle any
    // Python type (dict, list, etc.)
    let parameters: serde_json::Value = match bound.get_item("parameters") {
        Ok(params_obj) => {
            let json_mod = py
                .import("json")
                .map_err(|e| PyRuntimeError::new_err(format!("import json: {e}")))?;
            let json_str: String = json_mod
                .call_method1("dumps", (params_obj,))
                .map_err(|e| PyRuntimeError::new_err(format!("json.dumps failed: {e}")))?
                .extract()
                .map_err(|e| PyRuntimeError::new_err(format!("extract failed: {e}")))?;
            serde_json::from_str(&json_str)
                .map_err(|e| PyValueError::new_err(format!("parameters JSON parse: {e}")))?
        }
        Err(_) => serde_json::json!({}),
    };

    Ok(ToolSpec {
        name,
        description,
        parameters,
    })
}
/// Maps an [`LlmError`] to a `PyRuntimeError`.
pub(super) fn llm_err_to_py(e: LlmError) -> PyErr {
    PyRuntimeError::new_err(e.to_string())
}
