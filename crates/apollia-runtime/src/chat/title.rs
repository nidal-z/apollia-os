//! Automatic naming of a chat session: the prompt sent to the router, the
//! sanitising that turns a model answer into a title short enough for the
//! list, and the rename that stores it. The desktop running the runtime in its
//! own process and `POST /api/v1/sessions/{id}/title/generate` both call
//! [`name_session`], so a conversation is named the same way in both modes.

use apollia_llm::types::ChatMessage as LlmChatMessage;
use apollia_llm::{CompletionRequest, LlmRouter, ObservabilityConfig};

use super::manager::ChatSessionManagerHandle;
use super::types::ChatError;

/// System prompt instructing the LLM to produce a short conversation title.
const TITLE_PROMPT: &str = "Tu génères des titres courts pour des conversations à partir de la \
requête de l'utilisateur. Le titre doit décrire l'intention de l'utilisateur en 3 à 5 mots. \
Exemples : « Aide rédaction CV », « Bug import CSV Pandas », « Idée nom d'agent IA ». \
Réponds UNIQUEMENT avec le titre - pas de guillemets, pas de ponctuation finale, pas \
d'introduction du type « Voici le titre : ».";

/// Maximum tokens the LLM may produce for a session title.
///
/// The request turns reasoning off; a model that reasons anyway still has room
/// to close a `<think>` block, which the post-process step strips.
const TITLE_MAX_TOKENS: u32 = 1024;

/// How the first line of a model that reasons in plain text opens, without
/// any tag (a Qwen model served by MLX writes "Here's a thinking process:").
/// A line that opens so is reasoning, never a title.
const REASONING_OPENINGS: [&str; 8] = [
    "here's a thinking process",
    "here is a thinking process",
    "thinking process",
    "okay, the user",
    "ok, the user",
    "the user wants",
    "let me think",
    "alright, the user",
];

/// Maximum character length of the persisted title.
const TITLE_MAX_CHARS: usize = 60;

/// Maximum character length of a title the user typed.
const USER_TITLE_MAX_CHARS: usize = 100;

/// Why a session could not be named or renamed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TitleError {
    /// The title the user typed is blank.
    #[error("title must not be empty")]
    EmptyTitle,
    /// The first message the title is drawn from is blank.
    #[error("first_message must not be empty")]
    EmptyMessage,
    /// No inference server is configured.
    #[error("no LLM router configured")]
    NoRouter,
    /// The completion failed.
    #[error("LLM call failed: {0}")]
    Llm(String),
    /// The model answered nothing usable as a title.
    #[error("LLM returned an empty title")]
    EmptyAnswer,
    /// The chat subsystem refused the rename.
    #[error(transparent)]
    Chat(#[from] ChatError),
}

/// Renames a session with a title the user typed: trimmed, refused when blank,
/// capped at [`USER_TITLE_MAX_CHARS`] characters.
///
/// # Errors
///
/// [`TitleError::EmptyTitle`] for a blank title, [`TitleError::Chat`] when the
/// chat subsystem refuses the rename (an unknown session among others).
pub async fn rename_session(
    manager: &ChatSessionManagerHandle,
    session_id: String,
    title: &str,
) -> Result<(), TitleError> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err(TitleError::EmptyTitle);
    }
    let cleaned: String = trimmed.chars().take(USER_TITLE_MAX_CHARS).collect();
    Ok(manager.rename_session(session_id, cleaned).await?)
}

/// Generates a short title for a chat session from its first user message,
/// and stores it.
///
/// Calls `llm` with a dedicated prompt, sanitises the response (trim, strip
/// surrounding quotes, drop trailing punctuation, truncate to
/// [`TITLE_MAX_CHARS`]), then persists it through the session rename.
///
/// Returns the generated title.
///
/// # Errors
///
/// [`TitleError::NoRouter`] without a router, [`TitleError::EmptyMessage`]
/// for a blank message, [`TitleError::Llm`] when the completion fails,
/// [`TitleError::EmptyAnswer`] when nothing usable comes back, and
/// [`TitleError::Chat`] when the rename is refused.
pub async fn name_session(
    manager: &ChatSessionManagerHandle,
    llm: Option<&LlmRouter>,
    session_id: String,
    first_message: String,
) -> Result<String, TitleError> {
    let llm = llm.ok_or(TitleError::NoRouter)?;
    if first_message.trim().is_empty() {
        return Err(TitleError::EmptyMessage);
    }

    let request = CompletionRequest {
        messages: vec![
            LlmChatMessage::system(TITLE_PROMPT.to_string()),
            LlmChatMessage::user(first_message),
        ],
        max_tokens: Some(TITLE_MAX_TOKENS),
        // A title is a few words: a model that reasons first wrote its
        // reasoning as the title.
        thinking: Some(false),
        ..CompletionRequest::default()
    };
    let obs = ObservabilityConfig::default();
    let response = llm
        .complete_with_observability(None, request, None, &obs)
        .await
        .map_err(|e| TitleError::Llm(e.to_string()))?;

    let title = sanitize_session_title(&response.content);
    if title.is_empty() {
        return Err(TitleError::EmptyAnswer);
    }

    manager.rename_session(session_id, title.clone()).await?;

    Ok(title)
}

/// Cleans a raw LLM response into a usable session title.
///
/// - Removes `<think>…</think>` and `<reasoning>…</reasoning>` blocks emitted
///   by reasoning models (DeepSeek R1, o1-style, …) - including unterminated
///   blocks when the response was truncated by `max_tokens`.
/// - Drops common preambles like "Voici le titre :" / "Title:".
/// - Keeps only the first non-empty line (titles are single-line).
/// - Trims whitespace.
/// - Strips a single pair of surrounding ASCII or French quotes.
/// - Removes trailing punctuation (`. , ! ? ; :` and French equivalents).
/// - Truncates to [`TITLE_MAX_CHARS`] characters (not bytes).
fn sanitize_session_title(raw: &str) -> String {
    let stripped = strip_reasoning_blocks(raw);
    // Take the first non-empty line - titles never span multiple lines.
    let line = stripped
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let lower = line.to_lowercase();
    if REASONING_OPENINGS
        .iter()
        .any(|opening| lower.starts_with(opening))
    {
        return String::new();
    }
    let mut s = strip_preamble(line).trim().to_string();

    // Strip a single pair of surrounding quotes (ASCII / French / typographic).
    if let Some(rest) = s
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .or_else(|| s.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')))
        .or_else(|| s.strip_prefix('«').and_then(|r| r.strip_suffix('»')))
        .or_else(|| s.strip_prefix('“').and_then(|r| r.strip_suffix('”')))
    {
        s = rest.trim().to_string();
    }

    let trailing: &[char] = &['.', ',', '!', '?', ';', ':', '。', '…'];
    while let Some(c) = s.chars().last() {
        if trailing.contains(&c) || c.is_whitespace() {
            s.pop();
        } else {
            break;
        }
    }
    s.chars().take(TITLE_MAX_CHARS).collect::<String>()
}

/// Removes `<think>…</think>` and `<reasoning>…</reasoning>` blocks.
///
/// Handles three reasoning-model shapes:
/// - Well-formed paired block (`<think>…</think>Title`): the block is dropped.
/// - Truncation: a response cut off inside an unterminated `<think>` block has
///   everything from that tag onward dropped (yielding an empty string - caller
///   rejects it).
/// - Pre-filled opening tag (Qwen3 family): the chat template injects `<think>`
///   into the prompt, so the model's raw output begins with reasoning text
///   terminated by a lone `</think>` with no matching opening tag. Everything up
///   to and including that dangling closing tag is dropped.
fn strip_reasoning_blocks(raw: &str) -> String {
    fn drop_block(text: &str, open: &str, close: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        loop {
            match (rest.find(open), rest.find(close)) {
                // A closing tag precedes any opening tag (or none exists): the
                // reasoning block was opened by the template, not the model.
                // Drop everything up to and including that closing tag.
                (None, Some(c)) => rest = &rest[c + close.len()..],
                (Some(o), Some(c)) if c < o => rest = &rest[c + close.len()..],
                // Well-formed paired block: keep text before the opening tag,
                // resume after the matching closing tag.
                (Some(o), _) => {
                    out.push_str(&rest[..o]);
                    let after_open = &rest[o + open.len()..];
                    match after_open.find(close) {
                        Some(end) => rest = &after_open[end + close.len()..],
                        // Unterminated block: drop everything from the opening tag.
                        None => return out,
                    }
                }
                // No tags left.
                (None, None) => {
                    out.push_str(rest);
                    return out;
                }
            }
        }
    }
    let s = drop_block(raw, "<think>", "</think>");
    drop_block(&s, "<reasoning>", "</reasoning>")
}

/// Strips common preambles models add despite the system prompt.
fn strip_preamble(line: &str) -> &str {
    let lower = line.to_lowercase();
    for prefix in [
        "voici le titre :",
        "voici le titre:",
        "titre :",
        "titre:",
        "title:",
        "title :",
    ] {
        if let Some(stripped) = lower.strip_prefix(prefix) {
            let consumed = line.len() - stripped.len();
            return line[consumed..].trim_start();
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_session_title_strips_quotes_and_punct() {
        // GIVEN a raw LLM response with surrounding quotes and trailing punctuation
        // WHEN sanitised
        // THEN both wrappers are removed
        assert_eq!(
            sanitize_session_title("  \"Plan de migration.\"  "),
            "Plan de migration"
        );
        assert_eq!(
            sanitize_session_title("«Idée d'article!»"),
            "Idée d'article"
        );
        assert_eq!(sanitize_session_title("Refonte sidebar"), "Refonte sidebar");
    }

    #[test]
    fn test_sanitize_session_title_truncates_to_max_chars() {
        // GIVEN a long response with many chars
        let long: String = "a".repeat(120);
        // WHEN sanitised
        let out = sanitize_session_title(&long);
        // THEN it is capped at TITLE_MAX_CHARS chars (not bytes)
        assert_eq!(out.chars().count(), TITLE_MAX_CHARS);
    }

    #[test]
    fn test_sanitize_session_title_empty_input() {
        // GIVEN a blank title, then one made of a single ellipsis
        // WHEN each is sanitised
        // THEN both come back empty, which is what makes the caller keep the previous title
        assert_eq!(sanitize_session_title("   "), "");
        assert_eq!(sanitize_session_title("…"), "");
    }

    #[test]
    fn test_sanitize_session_title_drops_think_block() {
        // GIVEN a reasoning-model response with a closed <think> block
        let raw = "<think>Okay, the user wants me to generate a title for a CV \
                   request, so let me think… Aide rédaction CV is concise.</think>\n\
                   Aide rédaction CV";
        // WHEN sanitised
        // THEN only the post-think title remains
        assert_eq!(sanitize_session_title(raw), "Aide rédaction CV");
    }

    #[test]
    fn test_sanitize_session_title_drops_prefilled_think_block() {
        // GIVEN a Qwen3-style response whose opening <think> tag was pre-filled by
        // the chat template, so the raw output starts with reasoning and ends the
        // block with a lone </think>
        let raw = "Okay, the user wants a short title for a CV request. \
                   Aide rédaction CV is concise.</think>\n\n\
                   Aide rédaction CV";
        // WHEN sanitised
        // THEN the dangling reasoning is dropped and only the title remains
        assert_eq!(sanitize_session_title(raw), "Aide rédaction CV");
    }

    #[test]
    fn test_sanitize_session_title_truncated_unterminated_think() {
        // GIVEN a response cut off inside an unterminated <think> block
        let raw = "<think>Okay, the user wants me to generate a short title fo";
        // WHEN sanitised
        // THEN the result is empty (caller will reject it and fall back)
        assert_eq!(sanitize_session_title(raw), "");
    }

    #[test]
    fn test_sanitize_session_title_refuses_untagged_reasoning() {
        // GIVEN the answer of a model that reasons in plain text, without a tag
        let raw = "Here's a thinking process:\n\n1. Analyze the request: the user wants a title.";
        // WHEN it is sanitised
        // THEN nothing comes back, so the session keeps its previous title
        assert_eq!(sanitize_session_title(raw), "");
        assert_eq!(sanitize_session_title("Okay, the user asks for a CV"), "");
    }

    #[test]
    fn test_sanitize_session_title_drops_preamble() {
        // GIVEN three titles the model prefixed with an announcement, in both languages
        // WHEN each is sanitised
        // THEN only the title itself remains
        assert_eq!(
            sanitize_session_title("Titre : Aide rédaction CV"),
            "Aide rédaction CV"
        );
        assert_eq!(
            sanitize_session_title("Voici le titre : Bug import CSV"),
            "Bug import CSV"
        );
        assert_eq!(
            sanitize_session_title("Title: Idée nom agent"),
            "Idée nom agent"
        );
    }

    #[test]
    fn test_sanitize_session_title_keeps_only_first_line() {
        // GIVEN an answer carrying a reasoning block, the title, and an afterthought
        let raw = "<think>blah</think>\nAide rédaction CV\n\nNote: alternative title.";
        // WHEN it is sanitised
        // THEN only the title line survives
        assert_eq!(sanitize_session_title(raw), "Aide rédaction CV");
    }
}
