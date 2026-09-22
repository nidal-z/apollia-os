//! Reasoning markers other than `<think>`, rewritten into `<think>`.
//!
//! The chat pipeline parses reasoning as `<think>...</think>`. That is Qwen's
//! spelling, and the one every OpenAI-compatible backend here re-inlines a
//! separate reasoning field into. Gemma 4 spells it differently in its own
//! template:
//!
//! ```text
//! <|channel>thought
//! ...the reasoning...
//! <channel|>the answer
//! ```
//!
//! A backend that hands the raw text through (any server running without a
//! reasoning parser for that template) therefore delivered the markers to the
//! chat unparsed, and the operator read the model's private reasoning as its
//! answer, markers included.
//!
//! The embedded engine now splits reasoning with its own template-aware parser,
//! so this is the fallback for everything else. It lives here, below both the
//! chat runtime and the ORIA engine, because both read model output as text:
//! the runtime to separate a reply from its reasoning, ORIA to keep a
//! compaction summary free of the summarizer's own reasoning.
//!
//! After a tool result the Gemma 4 template opens the thought channel itself,
//! in the prompt, so the model's output carries only the closing marker. The
//! same happens with any template that pre-fills `<think>`. A closing tag with
//! no opening before it therefore means everything ahead of it was reasoning,
//! and an opening tag is supplied at the start so every parser downstream reads
//! a balanced block.

use std::borrow::Cow;

/// Gemma 4's opening marker, with the channel name its template always uses.
const GEMMA_THOUGHT_OPEN: &str = "<|channel>thought";
/// The same opening marker without a channel name, accepted defensively.
const GEMMA_OPEN: &str = "<|channel>";
/// Gemma 4's closing marker.
const GEMMA_CLOSE: &str = "<channel|>";

/// Rewrite every known reasoning marker into `<think>` / `</think>`.
///
/// Borrows when the text carries none, which is the common case, so the call
/// costs nothing on a model that already speaks `<think>`.
#[must_use]
pub fn normalise_reasoning_markers(text: &str) -> Cow<'_, str> {
    let foreign = text.contains(GEMMA_OPEN) || text.contains(GEMMA_CLOSE);
    let rewritten: Cow<'_, str> = if foreign {
        Cow::Owned(
            text.replace(GEMMA_THOUGHT_OPEN, "<think>")
                .replace(GEMMA_OPEN, "<think>")
                .replace(GEMMA_CLOSE, "</think>"),
        )
    } else {
        Cow::Borrowed(text)
    };

    // A closing tag with no opening ahead of it: the template opened the block
    // in the prompt, so the output starts inside it.
    match rewritten.find("</think>") {
        Some(close) if !rewritten[..close].contains("<think>") => {
            Cow::Owned(format!("<think>{rewritten}"))
        }
        _ => rewritten,
    }
}

/// The text with every reasoning block removed, markers normalised first.
///
/// An unclosed block is dropped to the end, since a reply cut off inside its
/// reasoning has no answer to keep. The result is trimmed.
#[must_use]
pub fn strip_reasoning(text: &str) -> String {
    let text = normalise_reasoning_markers(text);
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_ref();
    while let Some(open) = rest.find("<think>") {
        out.push_str(&rest[..open]);
        let after = &rest[open + "<think>".len()..];
        match after.find("</think>") {
            Some(close) => rest = &after[close + "</think>".len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gemma_thought_channel_becomes_a_think_block() {
        // GIVEN a Gemma 4 reply with its reasoning channel inline, as reported
        let raw = "<|channel>thought The user typed \"test\". <channel|>I'm here.";

        // WHEN the markers are normalised
        let out = normalise_reasoning_markers(raw);

        // THEN the pipeline's own tags replace them, so the reasoning is parsed
        // as reasoning instead of being shown as the answer
        assert_eq!(out, "<think> The user typed \"test\". </think>I'm here.");
    }

    #[test]
    fn text_without_foreign_markers_is_borrowed_untouched() {
        // GIVEN a reply that already uses `<think>`
        let raw = "<think>weighing</think>the answer";

        // WHEN the markers are normalised
        let out = normalise_reasoning_markers(raw);

        // THEN nothing is copied or changed
        assert!(matches!(out, Cow::Borrowed(_)));
        assert_eq!(out, raw);
    }

    #[test]
    fn an_orphan_closing_marker_never_reaches_the_screen() {
        // GIVEN the continuation after a tool result, whose opening marker the
        // template put in the prompt rather than in the output
        let raw = "the listing shows three files<channel|>You have three files.";

        // WHEN the markers are normalised
        let out = normalise_reasoning_markers(raw);

        // THEN what came before it is recognised as reasoning, and the marker
        // never reaches the screen
        assert_eq!(
            out,
            "<think>the listing shows three files</think>You have three files."
        );
    }

    #[test]
    fn an_orphan_think_close_from_a_prefilled_template_is_balanced() {
        // GIVEN a Qwen-style continuation whose `<think>` the template pre-filled
        let raw = "checking the result</think>Done.";

        // WHEN the markers are normalised
        let out = normalise_reasoning_markers(raw);

        // THEN the block is balanced, so the reasoning is not shown as the answer
        assert_eq!(out, "<think>checking the result</think>Done.");
    }

    #[test]
    fn stripping_keeps_only_the_answer() {
        // GIVEN replies carrying reasoning in each spelling the pipeline meets
        // WHEN the reasoning is stripped
        // THEN only the answer is left
        assert_eq!(
            strip_reasoning("<think>plan</think> The answer."),
            "The answer."
        );
        assert_eq!(
            strip_reasoning("<|channel>thought plan<channel|>The answer."),
            "The answer."
        );
        assert_eq!(strip_reasoning("plan</think>The answer."), "The answer.");
    }

    #[test]
    fn a_reply_cut_off_inside_its_reasoning_strips_to_nothing() {
        // GIVEN a reply whose reasoning never closed
        // WHEN it is stripped
        // THEN nothing is left, which a caller can recognise as "no answer"
        assert_eq!(strip_reasoning("<think>still thinking about"), "");
    }
}
