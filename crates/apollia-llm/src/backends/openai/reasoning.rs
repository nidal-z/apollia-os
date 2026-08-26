//! The reasoning envelope some OpenAI-compatible servers add to a response.
//!
//! Split out of `openai.rs`: the client stays in the parent, the non-standard
//! `reasoning` and timing fields it reads out of the raw body live here.

/// A response type plus the engine timings the declared type would discard.
///
/// `serde` drops unknown fields, so the crate's own response struct silently
/// loses the `timings` object that `llama-server` attaches. Flattening that
/// struct inside this wrapper preserves its parsing exactly while capturing the
/// extra key. Backends that report nothing simply yield `None`.
#[derive(serde::Deserialize)]
pub(super) struct WithTimings<T> {
    #[serde(flatten)]
    pub(super) inner: T,
    #[serde(default)]
    pub(super) timings: Option<serde_json::Value>,
}
/// The separate reasoning field carried by a non-streaming response.
///
/// See [`RawReasoning`] for why this is captured at all. Parsed from the same
/// body as [`WithTimings`], in a second pass over the raw bytes, because the
/// field sits inside `choices[].message` where a top-level flatten cannot reach.
#[derive(serde::Deserialize)]
pub(super) struct ReasoningEnvelope {
    #[serde(default)]
    pub(super) choices: Vec<ReasoningChoice>,
}
#[derive(serde::Deserialize)]
pub(super) struct ReasoningChoice {
    /// Non-streaming responses carry the field under `message`, streamed ones
    /// under `delta`. One of the two is present; both are optional.
    #[serde(default)]
    pub(super) message: Option<RawReasoning>,
    #[serde(default)]
    pub(super) delta: Option<RawReasoning>,
}
/// Reasoning a server streams beside the content instead of inside it.
///
/// The embedded `llama-server` runs with `--reasoning-format none`, so a
/// reasoning model's thoughts stay inline in `content` as `<think>` tags, which
/// is what the chat pipeline parses. Other OpenAI-compatible servers split them
/// out instead: Ollama uses `reasoning`, vLLM and DeepSeek use
/// `reasoning_content`. A client that reads only `content` drops them entirely,
/// which on a reasoning model means the user watches an empty screen for the
/// whole thinking phase and the pipeline sees no reasoning at all.
///
/// Both spellings are accepted here and re-inlined as `<think>` tags, so every
/// backend reaches the rest of the runtime in the same shape.
#[derive(serde::Deserialize)]
pub(super) struct RawReasoning {
    #[serde(default, alias = "reasoning_content")]
    pub(super) reasoning: Option<String>,
}
/// Opening tag used to re-inline reasoning that arrived in a separate field.
pub(super) const THINK_OPEN: &str = "<think>";
/// Closing counterpart of [`THINK_OPEN`].
pub(super) const THINK_CLOSE: &str = "</think>";
/// Prefix `content` with the separate `reasoning`, wrapped in `<think>` tags.
///
/// A backend that already inlines its reasoning reports no separate field and
/// its content passes through untouched, so the two shapes converge without the
/// caller having to know which server answered.
pub(super) fn inline_reasoning(reasoning: Option<String>, content: String) -> String {
    match reasoning {
        Some(r) if !r.is_empty() => format!("{THINK_OPEN}{r}{THINK_CLOSE}{content}"),
        _ => content,
    }
}
impl ReasoningEnvelope {
    /// The first choice's reasoning delta, when it is present and non-empty.
    pub(super) fn first(&self) -> Option<&str> {
        self.choices
            .first()
            .and_then(|c| c.message.as_ref().or(c.delta.as_ref()))
            .and_then(|r| r.reasoning.as_deref())
            .filter(|s| !s.is_empty())
    }
}
