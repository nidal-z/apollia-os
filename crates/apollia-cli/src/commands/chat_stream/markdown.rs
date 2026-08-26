//! Rendu markdown inline et rendu des appels d'outil.

use std::io::{self, Write};

use super::render_loop::RenderState;

// ─── Rendu markdown inline ───────────────────────────────────────────────────

/// Sequence de retour en debut de ligne + effacement de la ligne entiere, pour
/// repeindre la ligne courante en mode couleur.
pub(super) const CLEAR_LINE: &str = "\r\x1b[2K";

/// Streame un token en mode couleur: finalise les lignes completes (rendu
/// markdown fige) et repeint la ligne partielle en cours a chaque token.
pub(super) fn stream_markdown_token(
    tok: &str,
    st: &mut RenderState,
    out: &mut impl Write,
) -> io::Result<()> {
    st.line_buf.push_str(tok);
    // Finaliser chaque ligne complete: on efface la ligne repeinte puis on
    // ecrit la version markdown suivie d'un saut definitif.
    while let Some(nl) = st.line_buf.find('\n') {
        let line: String = st.line_buf.drain(..=nl).collect();
        let content = &line[..line.len() - 1];
        write!(out, "{CLEAR_LINE}{}", render_markdown_line(content, true))?;
        writeln!(out)?;
    }
    // Repeindre la ligne partielle restante (delimiteurs non fermes rendus
    // litteralement jusqu'a leur fermeture par un token ulterieur).
    write!(
        out,
        "{CLEAR_LINE}{}",
        render_markdown_line(&st.line_buf, true)
    )?;
    out.flush()?;
    st.at_line_start = st.line_buf.is_empty();
    Ok(())
}

/// Rend un bloc multi-ligne (fallback `response_completed` sans tokens).
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

/// Rend une ligne markdown: prefixes de bloc (titres, listes) puis style inline.
///
/// En mode couleur, applique des codes SGR ANSI; sinon retire simplement les
/// delimiteurs pour un texte propre. Rendu volontairement leger (pas de tableaux
/// ni de blocs de code multi-lignes), robuste (delimiteurs non apparies laisses
/// tels quels).
pub(super) fn render_markdown_line(line: &str, use_color: bool) -> String {
    // Titre `#`, `##`, `###` -> ligne en gras.
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

/// Applique le style inline: code, gras, italique. Ordre important pour que le
/// gras `**` soit traite avant l'italique `*`.
pub(super) fn render_inline(text: &str, use_color: bool) -> String {
    let s = wrap_pairs(text, "`", "7", use_color); // video inverse pour le code
    let s = wrap_pairs(&s, "**", "1", use_color); // gras
    let s = wrap_pairs(&s, "__", "1", use_color); // gras
    let s = wrap_pairs(&s, "*", "3", use_color); // italique
    wrap_pairs(&s, "_", "3", use_color) // italique
}

/// Enveloppe chaque paire de `delim` dans le code SGR `sgr` (mode couleur) ou
/// retire les delimiteurs (mode brut). Un delimiteur non apparie est conserve
/// litteralement.
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
            // Pas de fermeture: on garde le delimiteur ouvrant tel quel.
            None => {
                out.push_str(&rest[..i + delim.len()]);
                rest = after;
            }
            Some(j) => {
                let inner = &after[..j];
                // Contenu vide (delimiteurs adjacents, p.ex. un `**` non ferme
                // vu par la passe `*`): garder l'ouvrant litteralement.
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

// ─── Rendu inline ────────────────────────────────────────────────────────────

/// Applique un code SGR ANSI si la couleur est active, sinon renvoie le texte nu.
pub(super) fn paint(text: &str, code: &str, use_color: bool) -> String {
    if use_color {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

/// Rend un appel d'outil qui demarre: glyphe, nom, apercu d'entree, intention.
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

/// Vue empruntee d'un appel d'outil termine, pour le rendu.
pub(super) struct ToolResultView<'a> {
    pub(super) tool_name: &'a str,
    pub(super) success: bool,
    pub(super) output_preview: &'a str,
    pub(super) analysis: Option<&'a str>,
}

/// Rend un appel d'outil termine: glyphe succes/echec, nom, apercu, analyse.
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
