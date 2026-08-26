//! Inline markdown rendering, and rendering of tool calls.

use std::io::{self, Write};

use super::render_loop::RenderState;

// ─── Rendu markdown inline ───────────────────────────────────────────────────

/// Carriage return plus erase-whole-line sequence, used to repaint the current
/// line in colour mode.
pub(super) const CLEAR_LINE: &str = "\r\x1b[2K";

/// Streams a token in colour mode: finalises the complete lines (frozen
/// markdown render) and repaints the partial line in progress at every token.
pub(super) fn stream_markdown_token(
    tok: &str,
    st: &mut RenderState,
    out: &mut impl Write,
) -> io::Result<()> {
    st.line_buf.push_str(tok);
    // Finalise every complete line: erase the repainted line, then write the
    // markdown version followed by a definitive break.
    while let Some(nl) = st.line_buf.find('\n') {
        let line: String = st.line_buf.drain(..=nl).collect();
        let content = &line[..line.len() - 1];
        write!(out, "{CLEAR_LINE}{}", render_markdown_line(content, true))?;
        writeln!(out)?;
    }
    // Repaint the remaining partial line (unclosed delimiters are rendered
    // literally until a later token closes them).
    write!(
        out,
        "{CLEAR_LINE}{}",
        render_markdown_line(&st.line_buf, true)
    )?;
    out.flush()?;
    st.at_line_start = st.line_buf.is_empty();
    Ok(())
}

/// Renders a multi-line block (the `response_completed` fallback, no tokens).
pub(super) fn render_block(content: &str, use_color: bool, out: &mut impl Write) -> io::Result<()> {
    let mut first = true;
    for line in content.split('\n') {
        if !first {
            writeln!(out)?;
        }
        first = false;
        write!(out, "{}", render_markdown_line(line, use_color))?;
    }
    Ok(())
}

/// Renders one markdown line: block prefixes (headings, lists) then inline style.
///
/// In colour mode, applies ANSI SGR codes; otherwise it simply strips the
/// delimiters for clean text. Deliberately light (no tables, no multi-line code
/// blocks) and robust (unpaired delimiters are left as they are).
pub(super) fn render_markdown_line(line: &str, use_color: bool) -> String {
    // Heading `#`, `##`, `###` -> line in bold.
    for prefix in ["### ", "## ", "# "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            let inner = render_inline(rest, use_color);
            return if use_color {
                paint(&inner, "1", true)
            } else {
                inner
            };
        }
    }
    // Puce `- `, `* `, `+ ` (indentation preservee) -> `• `.
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);
    for prefix in ["- ", "* ", "+ "] {
        if let Some(item) = rest.strip_prefix(prefix) {
            return format!("{indent}• {}", render_inline(item, use_color));
        }
    }
    render_inline(line, use_color)
}

/// Applies the inline style: code, bold, italic. The order matters, so that
/// bold `**` is handled before italic `*`.
pub(super) fn render_inline(text: &str, use_color: bool) -> String {
    let s = wrap_pairs(text, "`", "7", use_color); // reverse video for code
    let s = wrap_pairs(&s, "**", "1", use_color); // bold
    let s = wrap_pairs(&s, "__", "1", use_color); // bold
    let s = wrap_pairs(&s, "*", "3", use_color); // italique
    wrap_pairs(&s, "_", "3", use_color) // italique
}

/// Wraps every pair of `delim` in the SGR code `sgr` (colour mode), or strips
/// the delimiters (plain mode). An unpaired delimiter is kept literally.
pub(super) fn wrap_pairs(text: &str, delim: &str, sgr: &str, use_color: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let Some(i) = rest.find(delim) else {
            out.push_str(rest);
            break;
        };
        let after = &rest[i + delim.len()..];
        match after.find(delim) {
            // No closer: keep the opening delimiter as it is.
            None => {
                out.push_str(&rest[..i + delim.len()]);
                rest = after;
            }
            Some(j) => {
                let inner = &after[..j];
                // Empty content (adjacent delimiters, for instance a `**` left
                // open seen by the `*` pass): keep the opener literally.
                if inner.is_empty() {
                    out.push_str(&rest[..i + delim.len()]);
                    rest = after;
                    continue;
                }
                out.push_str(&rest[..i]);
                if use_color {
                    out.push_str(&format!("\x1b[{sgr}m{inner}\x1b[0m"));
                } else {
                    out.push_str(inner);
                }
                rest = &after[j + delim.len()..];
            }
        }
    }
    out
}

// ─── Inline rendering ───────────────────────────────────────────────────────

/// Applies an ANSI SGR code when colour is on, otherwise returns the bare text.
pub(super) fn paint(text: &str, code: &str, use_color: bool) -> String {
    if use_color {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

/// Renders a tool call that starts: glyph, name, input preview, intent.
pub(super) fn render_tool_started(
    tool_name: &str,
    input_preview: &str,
    rationale: Option<&str>,
    use_color: bool,
    out: &mut impl Write,
) -> io::Result<()> {
    let name = paint(tool_name, "1", use_color);
    if input_preview.is_empty() {
        writeln!(out, "  ● {name}")?;
    } else {
        writeln!(out, "  ● {name}  {}", paint(input_preview, "2", use_color))?;
    }
    if let Some(r) = rationale {
        writeln!(out, "  {}", paint(&format!("└── {r}"), "2", use_color))?;
    }
    Ok(())
}

/// Borrowed view of a finished tool call, for rendering.
pub(super) struct ToolResultView<'a> {
    pub(super) tool_name: &'a str,
    pub(super) success: bool,
    pub(super) output_preview: &'a str,
    pub(super) analysis: Option<&'a str>,
}

/// Renders a finished tool call: success/failure glyph, name, preview, analysis.
pub(super) fn render_tool_completed(
    view: &ToolResultView<'_>,
    use_color: bool,
    out: &mut impl Write,
) -> io::Result<()> {
    let (glyph, code) = if view.success {
        ("✔", "32")
    } else {
        ("✗", "31")
    };
    let g = paint(glyph, code, use_color);
    let name = paint(view.tool_name, "1", use_color);
    if view.output_preview.is_empty() {
        writeln!(out, "  {g} {name}")?;
    } else {
        writeln!(
            out,
            "  {g} {name}  {}",
            paint(view.output_preview, "2", use_color)
        )?;
    }
    if let Some(a) = view.analysis {
        writeln!(out, "  {}", paint(&format!("└── {a}"), "2", use_color))?;
    }
    Ok(())
}
