//! Confidence + citation markup parser (US-SP42-046 — Pattern P10, always-on).
//!
//! Parses markers inline in agent output and returns a structured
//! [`ParsedMessage`] with assertions, citations and the cleaned text. No LLM
//! is involved — coût : 0.
//!
//! # Supported markers
//!
//! - `[conf:high]...[/conf]`, `[conf:medium]...[/conf]`, `[conf:low]...[/conf]`
//!   wrap a text span and tag it with a confidence level. Nested `conf`
//!   spans use the *innermost* level.
//! - `[cite:<id>]` is an inline pointer to a [`Citation`] in the message's
//!   citation list. Multiple citations inside the same `conf` span are
//!   attached to the enclosing assertion. `[cite:a,b]` shorthand is
//!   accepted and split.
//! - Citations themselves are declared out-of-band by the agent (in
//!   `AIPResult.metadata["citations"]`) or via the SDK helper.
//!
//! Malformed markers (unknown level, missing closing tag, orphan cite outside
//! a `conf` span) are left untouched in the cleaned text — the renderer
//! simply falls back to classic prose.

use serde::{Deserialize, Serialize};
use std::ops::Range;

/// Confidence level attached to an [`Assertion`].
///
/// Serialised in lower-case kebab form (`"high"`, `"medium"`, `"low"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfidenceLevel {
    High,
    Medium,
    Low,
}

impl ConfidenceLevel {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "high" => Some(Self::High),
            "medium" | "med" => Some(Self::Medium),
            "low" => Some(Self::Low),
            _ => None,
        }
    }
}

/// Broad classification of a citation's origin.
///
/// Drives the rendering of the footnote badge (icon, colour). Unknown
/// values serialise to `"other"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    Web,
    Document,
    Tool,
    Memory,
    #[default]
    Other,
}

/// A bibliographic entry referenced by one or more [`Assertion`]s.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    /// Stable identifier used in `[cite:<id>]` markers.
    pub id: String,
    /// Human-readable title shown in the footnote.
    pub title: String,
    /// Optional URL — rendered as a clickable link when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Optional excerpt surfaced in the tooltip / side-panel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    /// Source classification.
    #[serde(default)]
    pub source_type: SourceType,
}

/// A span of the cleaned text tagged with a confidence level and citations.
///
/// `text_range` indexes into the cleaned (marker-stripped) text returned by
/// [`parse`]. Ranges are half-open byte ranges, following Rust conventions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assertion {
    pub text_range: AssertionRange,
    pub confidence: ConfidenceLevel,
    #[serde(default)]
    pub citation_ids: Vec<String>,
}

/// Serializable counterpart of `std::ops::Range<usize>` (serde-friendly).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssertionRange {
    pub start: usize,
    pub end: usize,
}

impl From<Range<usize>> for AssertionRange {
    fn from(r: Range<usize>) -> Self {
        Self {
            start: r.start,
            end: r.end,
        }
    }
}

impl From<AssertionRange> for Range<usize> {
    fn from(r: AssertionRange) -> Self {
        r.start..r.end
    }
}

/// Output of [`parse`].
///
/// `text` is the cleaned message (markers removed). `assertions` reference
/// ranges in `text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedMessage {
    pub text: String,
    #[serde(default)]
    pub assertions: Vec<Assertion>,
}

/// Parse confidence/citation markers out of *raw* agent text.
///
/// Unknown, malformed, or unbalanced markers are preserved verbatim so the
/// renderer can still show the underlying prose. Returns a
/// [`ParsedMessage`] with the cleaned text and recognised assertions.
pub fn parse(raw: &str) -> ParsedMessage {
    let mut out = String::with_capacity(raw.len());
    let mut assertions: Vec<Assertion> = Vec::new();
    let mut stack: Vec<OpenSpan> = Vec::new();
    let bytes = raw.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some((token, consumed)) = read_token(&raw[i..]) {
                match token {
                    Token::OpenConf(level) => {
                        stack.push(OpenSpan {
                            level,
                            start: out.len(),
                            citation_ids: Vec::new(),
                        });
                        i += consumed;
                        continue;
                    }
                    Token::CloseConf => {
                        if let Some(open) = stack.pop() {
                            let end = out.len();
                            if end > open.start {
                                assertions.push(Assertion {
                                    text_range: (open.start..end).into(),
                                    confidence: open.level,
                                    citation_ids: open.citation_ids,
                                });
                            }
                            i += consumed;
                            continue;
                        }
                        // Orphan [/conf] → fall through as literal.
                    }
                    Token::Cite(ids) => {
                        if let Some(top) = stack.last_mut() {
                            for id in ids {
                                if !top.citation_ids.contains(&id) {
                                    top.citation_ids.push(id);
                                }
                            }
                            i += consumed;
                            continue;
                        }
                        // Orphan [cite:...] → keep as literal.
                    }
                }
            }
        }
        // Default: copy one UTF-8 character verbatim.
        let ch_len = utf8_char_len(bytes[i]);
        out.push_str(&raw[i..i + ch_len]);
        i += ch_len;
    }

    // Any still-open spans are malformed — we keep whatever text was emitted
    // but do NOT record an assertion (the closing marker was missing).

    ParsedMessage {
        text: out,
        assertions,
    }
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

struct OpenSpan {
    level: ConfidenceLevel,
    start: usize,
    citation_ids: Vec<String>,
}

enum Token {
    OpenConf(ConfidenceLevel),
    CloseConf,
    Cite(Vec<String>),
}

/// Attempt to read a marker starting at the `[` character.
///
/// Returns the parsed token and the number of bytes consumed, or `None`
/// when the bracketed content is not a recognised marker.
fn read_token(input: &str) -> Option<(Token, usize)> {
    debug_assert!(input.starts_with('['));
    let end = input.find(']')?;
    let inner = &input[1..end];
    let consumed = end + 1;

    if let Some(rest) = inner.strip_prefix("conf:") {
        let level = ConfidenceLevel::parse(rest)?;
        return Some((Token::OpenConf(level), consumed));
    }
    if inner == "/conf" {
        return Some((Token::CloseConf, consumed));
    }
    if let Some(rest) = inner.strip_prefix("cite:") {
        let ids: Vec<String> = rest
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        if ids.is_empty() {
            return None;
        }
        return Some((Token::Cite(ids), consumed));
    }
    None
}

/// Byte-length of the UTF-8 character beginning at `lead`.
fn utf8_char_len(lead: u8) -> usize {
    if lead < 0x80 {
        1
    } else if lead < 0xC0 {
        1 // continuation byte, treat defensively
    } else if lead < 0xE0 {
        2
    } else if lead < 0xF0 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(a: &Assertion) -> Range<usize> {
        a.text_range.into()
    }

    /// GIVEN text with a single high-confidence span
    /// WHEN parse
    /// THEN one assertion is produced and markers are stripped.
    #[test]
    fn parses_single_high_confidence_span() {
        let p = parse("Hello [conf:high]world[/conf] !");
        assert_eq!(p.text, "Hello world !");
        assert_eq!(p.assertions.len(), 1);
        assert_eq!(p.assertions[0].confidence, ConfidenceLevel::High);
        assert_eq!(&p.text[range(&p.assertions[0])], "world");
    }

    /// GIVEN nested conf spans
    /// WHEN parse
    /// THEN both assertions are emitted in post-order with correct ranges.
    #[test]
    fn handles_nested_conf_spans() {
        let p = parse("[conf:low]A [conf:high]B[/conf] C[/conf]");
        assert_eq!(p.text, "A B C");
        assert_eq!(p.assertions.len(), 2);
        // Inner closes first → pushed first.
        assert_eq!(p.assertions[0].confidence, ConfidenceLevel::High);
        assert_eq!(&p.text[range(&p.assertions[0])], "B");
        assert_eq!(p.assertions[1].confidence, ConfidenceLevel::Low);
        assert_eq!(&p.text[range(&p.assertions[1])], "A B C");
    }

    /// GIVEN a conf span with an inline cite
    /// WHEN parse
    /// THEN the cite id is attached to the assertion and removed from text.
    #[test]
    fn attaches_citations_to_enclosing_span() {
        let p = parse("Earth is round [conf:high]per NASA[cite:nasa-2024][/conf].");
        assert_eq!(p.text, "Earth is round per NASA.");
        let a = &p.assertions[0];
        assert_eq!(a.citation_ids, vec!["nasa-2024"]);
    }

    /// GIVEN multi-id cite shorthand
    /// WHEN parse
    /// THEN ids are split and deduplicated.
    #[test]
    fn parses_comma_separated_citation_ids() {
        let p = parse("[conf:medium]claim[cite:a, b,a][/conf]");
        assert_eq!(p.text, "claim");
        assert_eq!(p.assertions[0].citation_ids, vec!["a", "b"]);
    }

    /// GIVEN text with no markers
    /// WHEN parse
    /// THEN the output is identical and no assertions are produced.
    #[test]
    fn leaves_unmarked_text_unchanged() {
        let p = parse("Just a plain sentence.");
        assert_eq!(p.text, "Just a plain sentence.");
        assert!(p.assertions.is_empty());
    }

    /// GIVEN an unknown confidence level
    /// WHEN parse
    /// THEN the marker is preserved verbatim.
    #[test]
    fn unknown_level_is_left_verbatim() {
        let p = parse("[conf:ultra]foo[/conf]");
        assert_eq!(p.text, "[conf:ultra]foo[/conf]");
        assert!(p.assertions.is_empty());
    }

    /// GIVEN an orphan `[/conf]`
    /// WHEN parse
    /// THEN it is left verbatim and no assertion is emitted.
    #[test]
    fn orphan_close_is_left_verbatim() {
        let p = parse("foo[/conf] bar");
        assert_eq!(p.text, "foo[/conf] bar");
        assert!(p.assertions.is_empty());
    }

    /// GIVEN a conf span that is never closed
    /// WHEN parse
    /// THEN no assertion is emitted, inner text survives.
    #[test]
    fn unclosed_span_drops_assertion() {
        let p = parse("[conf:high]dangling");
        assert_eq!(p.text, "dangling");
        assert!(p.assertions.is_empty());
    }

    /// GIVEN an orphan `[cite:x]` outside any conf span
    /// WHEN parse
    /// THEN it is preserved as literal text.
    #[test]
    fn orphan_cite_is_literal() {
        let p = parse("see [cite:x]");
        assert_eq!(p.text, "see [cite:x]");
        assert!(p.assertions.is_empty());
    }

    /// GIVEN malformed brackets (missing `]`)
    /// WHEN parse
    /// THEN the `[` is copied verbatim and parsing continues.
    #[test]
    fn malformed_bracket_is_passthrough() {
        let p = parse("an [unbounded marker");
        assert_eq!(p.text, "an [unbounded marker");
    }

    /// GIVEN a conf span around multi-byte UTF-8 text
    /// WHEN parse
    /// THEN the range slices back into valid UTF-8.
    #[test]
    fn preserves_utf8_ranges() {
        let p = parse("[conf:high]éléphant 🐘[/conf]");
        let a = &p.assertions[0];
        let slice = &p.text[range(a)];
        assert_eq!(slice, "éléphant 🐘");
    }

    /// GIVEN `med` alias
    /// WHEN parse
    /// THEN it maps to Medium.
    #[test]
    fn accepts_med_alias() {
        let p = parse("[conf:med]x[/conf]");
        assert_eq!(p.assertions[0].confidence, ConfidenceLevel::Medium);
    }

    /// GIVEN multiple cites inside a single conf span
    /// WHEN parse
    /// THEN all ids accumulate on the assertion.
    #[test]
    fn multiple_cites_accumulate() {
        let p = parse("[conf:high]A[cite:a] B[cite:b][/conf]");
        assert_eq!(p.text, "A B");
        assert_eq!(p.assertions[0].citation_ids, vec!["a", "b"]);
    }
}
