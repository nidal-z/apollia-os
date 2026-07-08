//! Streaming chat send loop.
//!
//! Consumes the session SSE stream (`GET /api/v1/sessions/:id/stream`) token by
//! token so the assistant reply appears live in the terminal, renders tool
//! calls (intent rationale + result + error analysis), and handles inline tool
//! approval prompts ("l'agent veut utiliser X : autoriser une fois / toujours /
//! refuser"). Replaces the former poll-then-print path.
//!
//! Rendering stays inline (no full-screen TUI): plain stdout, Unicode glyphs,
//! and optional ANSI styling gated on a TTY and `--no-color`.

use std::io::{self, BufRead, IsTerminal, Write};
use std::time::Duration;

use futures::StreamExt;

use crate::client::{AuthorizeToolArgs, ClientError, RuntimeClient};
use crate::exit_codes;

/// Garde d'inactivite: si aucune ligne SSE n'arrive pendant ce delai, la boucle
/// rend la main au REPL plutot que de rester figee sur un stream muet. Le stream
/// de session ne se ferme pas par message, cette garde est donc indispensable.
const STREAM_IDLE_TIMEOUT_SECS: u64 = 120;

// ─── Classification des evenements ──────────────────────────────────────────

/// Action derivee d'une ligne SSE du stream de session chat.
#[derive(Debug, PartialEq)]
enum ChatStreamAction {
    /// Un fragment de texte assistant a afficher en direct.
    Token(String),
    /// Un appel d'outil demarre (nom, apercu d'entree, intention).
    ToolStarted {
        tool_name: String,
        input_preview: String,
        rationale: Option<String>,
    },
    /// Un appel d'outil termine (succes, apercu de sortie, analyse d'erreur).
    ToolCompleted {
        tool_name: String,
        success: bool,
        output_preview: String,
        analysis: Option<String>,
    },
    /// L'agent demande l'autorisation d'utiliser un outil.
    ApprovalRequired {
        message_id: String,
        tool_name: String,
        prompt: String,
    },
    /// Reponse terminee pour ce tour (contenu complet, fallback si zero token).
    Completed { content: String },
    /// Erreur de generation pour ce tour.
    Error(String),
    /// Session fermee cote serveur, terminal global.
    SessionClosed,
    /// Rien a afficher (echo du prompt, autre message, ligne non pertinente).
    Ignore,
}

/// Classe une ligne SSE brute en [`ChatStreamAction`], filtree sur le tour
/// courant `my_id`. Fonction pure et testable, sans I/O ni socket.
fn classify_chat_event(line: &str, my_id: &str) -> ChatStreamAction {
    let Some(data) = line.strip_prefix("data: ") else {
        return ChatStreamAction::Ignore;
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) else {
        return ChatStreamAction::Ignore;
    };
    let event = parsed.get("event").and_then(|v| v.as_str()).unwrap_or("");

    // `session_closed` ne porte pas de message_id: fin de session, terminal.
    if event == "session_closed" {
        return ChatStreamAction::SessionClosed;
    }

    // Tous les autres evenements sont filtres sur le message_id du tour courant.
    // Le 202 renvoie le meme id que celui porte par les tokens et la reponse.
    let mid = parsed
        .get("message_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if mid != my_id {
        return ChatStreamAction::Ignore;
    }

    match event {
        "token" => ChatStreamAction::Token(str_field(&parsed, "token")),
        "tool_call_started" => ChatStreamAction::ToolStarted {
            tool_name: str_field(&parsed, "tool_name"),
            input_preview: str_field(&parsed, "input_preview"),
            rationale: extract_rationale(&parsed),
        },
        "tool_call_completed" => ChatStreamAction::ToolCompleted {
            tool_name: str_field(&parsed, "tool_name"),
            success: parsed
                .get("success")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            output_preview: str_field(&parsed, "output_preview"),
            analysis: extract_analysis(&parsed),
        },
        "approval_required" => ChatStreamAction::ApprovalRequired {
            message_id: mid.to_string(),
            tool_name: str_field(&parsed, "tool_name"),
            prompt: str_field(&parsed, "prompt"),
        },
        "response_completed" => ChatStreamAction::Completed {
            content: str_field(&parsed, "content"),
        },
        "error" => ChatStreamAction::Error(str_field(&parsed, "error")),
        // message_sent (echo du prompt), response_started, approval_resolved:
        // rien a afficher.
        _ => ChatStreamAction::Ignore,
    }
}

/// Lit un champ chaine, chaine vide si absent.
fn str_field(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

/// Extrait le resume d'intention d'un `ToolCallRationale` serialise, si present.
fn extract_rationale(v: &serde_json::Value) -> Option<String> {
    let r = v.get("rationale")?;
    if r.is_null() {
        return None;
    }
    let summary = r.get("summary")?.as_str()?;
    if summary.is_empty() {
        None
    } else {
        Some(summary.to_string())
    }
}

/// Extrait le message humain d'un `ErrorAnalysis` serialise (avec le conseil
/// eventuel), si present.
fn extract_analysis(v: &serde_json::Value) -> Option<String> {
    let a = v.get("analysis")?;
    if a.is_null() {
        return None;
    }
    let human = a.get("human_message")?.as_str()?;
    if human.is_empty() {
        return None;
    }
    match a.get("suggested_action").and_then(|x| x.as_str()) {
        Some(action) if !action.is_empty() => Some(format!("{human}  (hint: {action})")),
        _ => Some(human.to_string()),
    }
}

// ─── Decision d'approbation ──────────────────────────────────────────────────

/// Decision saisie au prompt d'approbation d'outil.
#[derive(Debug, PartialEq)]
enum ToolDecisionInput {
    /// Autoriser une fois.
    Accept,
    /// Toujours autoriser (une portee sera demandee ensuite).
    Always,
    /// Refuser, avec une raison optionnelle.
    Refuse(Option<String>),
    /// Saisie non reconnue.
    Invalid,
}

/// Parse la decision saisie au prompt. Accepte le francais et l'anglais.
///
/// `a`/`autoriser`/`accept`/`oui` -> Accept ; `t`/`toujours`/`always` -> Always ;
/// `r`/`refuser`/`refuse`/`non` [raison] -> Refuse. Le reste est Invalid.
fn parse_tool_decision(input: &str) -> ToolDecisionInput {
    let trimmed = input.trim();
    let (head, rest) = match trimmed.split_once(char::is_whitespace) {
        Some((h, r)) => (h, r.trim()),
        None => (trimmed, ""),
    };
    match head.to_ascii_lowercase().as_str() {
        "a" | "autoriser" | "accept" | "oui" | "y" => ToolDecisionInput::Accept,
        "t" | "toujours" | "always" => ToolDecisionInput::Always,
        "r" | "refuser" | "refuse" | "non" | "n" => {
            let reason = if rest.is_empty() {
                None
            } else {
                Some(rest.to_string())
            };
            ToolDecisionInput::Refuse(reason)
        }
        _ => ToolDecisionInput::Invalid,
    }
}

/// Traduit un choix de portee en valeur wire snake_case attendue par l'API.
///
/// Entree vide ou `1` -> `this_session` (defaut, le moins collant) ; `2` ->
/// `this_tool` ; `3` -> `this_project`. Le reste est invalide.
fn parse_scope_choice(input: &str) -> Option<&'static str> {
    match input.trim() {
        "" | "1" => Some("this_session"),
        "2" => Some("this_tool"),
        "3" => Some("this_project"),
        _ => None,
    }
}

// ─── Etat et controle de boucle ──────────────────────────────────────────────

/// Etat de rendu maintenu au fil des evenements d'un tour.
struct RenderState {
    /// Au moins un token a ete streame (evite de reimprimer le contenu final).
    any_token: bool,
    /// Le curseur est en debut de ligne (pour couper proprement avant un bloc).
    at_line_start: bool,
    /// Coloration ANSI active.
    use_color: bool,
    /// Ligne en cours de construction, pour le rendu markdown en mode couleur:
    /// on repeint cette ligne a chaque token pour appliquer le style une fois
    /// les delimiteurs fermes. Vide en mode brut (`--no-color`).
    line_buf: String,
}

impl RenderState {
    fn new(use_color: bool) -> Self {
        Self {
            any_token: false,
            at_line_start: true,
            use_color,
            line_buf: String::new(),
        }
    }
}

/// Suite a donner apres traitement d'un evenement.
#[derive(Debug, PartialEq)]
enum LoopControl {
    /// Continuer a lire le stream.
    Continue,
    /// Tour termine avec succes.
    Done,
    /// Tour termine en erreur (texte a signaler sur stderr).
    Failed(String),
    /// Une approbation d'outil est requise avant de poursuivre.
    Approval {
        message_id: String,
        tool_name: String,
        prompt: String,
    },
}

/// Traite un evenement classifie: ecrit le rendu humain dans `out` et renvoie
/// la suite a donner. Coeur synchrone et testable, sans I/O reseau ni stdin.
fn handle_action(
    action: ChatStreamAction,
    st: &mut RenderState,
    out: &mut impl Write,
) -> io::Result<LoopControl> {
    match action {
        ChatStreamAction::Token(tok) => {
            st.any_token = true;
            if st.use_color {
                // Mode couleur (TTY): on repeint la ligne en cours a chaque
                // token pour appliquer le style markdown une fois les
                // delimiteurs fermes.
                stream_markdown_token(&tok, st, out)?;
            } else {
                // Mode brut (--no-color / pipe): flux verbatim, prouve et sur.
                write!(out, "{tok}")?;
                out.flush()?;
                st.at_line_start = tok.ends_with('\n');
            }
            Ok(LoopControl::Continue)
        }
        ChatStreamAction::ToolStarted {
            tool_name,
            input_preview,
            rationale,
        } => {
            break_line(st, out)?;
            render_tool_started(
                &tool_name,
                &input_preview,
                rationale.as_deref(),
                st.use_color,
                out,
            )?;
            Ok(LoopControl::Continue)
        }
        ChatStreamAction::ToolCompleted {
            tool_name,
            success,
            output_preview,
            analysis,
        } => {
            break_line(st, out)?;
            let view = ToolResultView {
                tool_name: &tool_name,
                success,
                output_preview: &output_preview,
                analysis: analysis.as_deref(),
            };
            render_tool_completed(&view, st.use_color, out)?;
            Ok(LoopControl::Continue)
        }
        ChatStreamAction::ApprovalRequired {
            message_id,
            tool_name,
            prompt,
        } => {
            break_line(st, out)?;
            Ok(LoopControl::Approval {
                message_id,
                tool_name,
                prompt,
            })
        }
        ChatStreamAction::Completed { content } => {
            // Les tokens ont deja ete streames en direct. On ne reimprime le
            // contenu complet qu'en secours, si aucun token n'est arrive.
            if !st.any_token && !content.is_empty() {
                render_block(&content, st.use_color, out)?;
                st.at_line_start = content.ends_with('\n');
            }
            if !st.at_line_start {
                writeln!(out)?;
                st.at_line_start = true;
            }
            st.line_buf.clear();
            Ok(LoopControl::Done)
        }
        ChatStreamAction::Error(err) => {
            if !st.at_line_start {
                writeln!(out)?;
                st.at_line_start = true;
            }
            Ok(LoopControl::Failed(err))
        }
        ChatStreamAction::SessionClosed => {
            if !st.at_line_start {
                writeln!(out)?;
                st.at_line_start = true;
            }
            Ok(LoopControl::Done)
        }
        ChatStreamAction::Ignore => Ok(LoopControl::Continue),
    }
}

/// Resout une approbation d'outil. Abstrait pour rendre la boucle testable sans
/// stdin ni reseau (la version de production lit stdin et POST la decision).
trait ApprovalResolver {
    fn resolve(
        &self,
        message_id: &str,
        tool_name: &str,
        prompt: &str,
    ) -> impl std::future::Future<Output = ()>;
}

/// Options de rendu de [`run_render_loop`].
#[derive(Clone, Copy)]
struct LoopOpts {
    /// Garde d'inactivite armee a chaque `next`.
    idle: Duration,
    /// Coloration ANSI active.
    use_color: bool,
}

/// Boucle de rendu humain sur un flux de lignes SSE. Generique sur le flux, la
/// sortie et le resolveur d'approbation: c'est le point d'injection des tests.
///
/// La garde d'inactivite est armee a chaque `next`. Les keep-alives SSE d'axum
/// (~15s) arrivent comme des lignes et la resettent, donc un chargement de
/// modele lent ne declenche pas de faux timeout: la garde ne vise qu'une
/// connexion reellement morte.
async fn run_render_loop<S, W, R>(
    mut lines: S,
    my_id: &str,
    out: &mut W,
    resolver: &R,
    opts: LoopOpts,
) -> io::Result<()>
where
    S: futures::Stream<Item = Result<String, ClientError>> + Unpin,
    W: Write,
    R: ApprovalResolver,
{
    let mut st = RenderState::new(opts.use_color);
    let idle = opts.idle;

    loop {
        let line = match tokio::time::timeout(idle, lines.next()).await {
            Ok(Some(Ok(l))) => l,
            Ok(Some(Err(e))) => {
                if !st.at_line_start {
                    writeln!(out)?;
                }
                eprintln!("[stream error: {e}]");
                break;
            }
            // Stream ferme par le serveur.
            Ok(None) => break,
            Err(_) => {
                if !st.at_line_start {
                    writeln!(out)?;
                }
                eprintln!("[no response received within the timeout]");
                break;
            }
        };

        match handle_action(classify_chat_event(&line, my_id), &mut st, out)? {
            LoopControl::Continue => {}
            LoopControl::Done => break,
            LoopControl::Failed(err) => {
                eprintln!("[error: {err}]");
                break;
            }
            LoopControl::Approval {
                message_id,
                tool_name,
                prompt,
            } => {
                resolver.resolve(&message_id, &tool_name, &prompt).await;
                // Le prompt a imprime des sauts de ligne: on repart au propre.
                st.at_line_start = true;
            }
        }
    }
    Ok(())
}

// ─── Boucle d'envoi streamee ─────────────────────────────────────────────────

/// Envoie `message` puis rend la reponse en streaming token par token.
///
/// Conserve le contrat `Result<(), i32>`: `Err(code)` seulement sur erreur
/// fatale (runtime injoignable). Une erreur cote serveur (session occupee,
/// generation en echec) laisse le REPL vivant (`Ok`).
pub async fn stream_send(
    client: &RuntimeClient,
    session_id: &str,
    message: &str,
    json: bool,
    no_color: bool,
) -> Result<(), i32> {
    let use_color = !no_color && io::stdout().is_terminal();

    // Ouvrir le SSE AVANT le POST. L'envoi est fire-and-forget et le bus ne
    // rejoue rien: s'abonner d'abord garantit qu'aucun token precoce n'est
    // perdu entre l'enqueue du travail et l'attachement de l'abonnement.
    let uri = format!("/api/v1/sessions/{session_id}/stream");
    let line_stream = match client.stream_sse_lines(&uri).await {
        Ok(s) => s,
        // Runtime injoignable: fatal, on quitte le REPL.
        Err(ClientError::ConnectionRefused) => {
            eprintln!("runtime not started");
            return Err(exit_codes::GENERAL_ERROR);
        }
        // Autre erreur de transport: non fatale, le REPL continue.
        Err(e) => {
            eprintln!("Error: {e}");
            return Ok(());
        }
    };

    // Declencher le travail: le 202 renvoie le message_id du tour courant, qui
    // correle aussi les tokens et evenements de la reponse.
    let my_id = match client.send_chat_message(session_id, message).await {
        Ok(resp) => resp["message_id"].as_str().unwrap_or("").to_string(),
        Err(ClientError::ConnectionRefused) => {
            eprintln!("runtime not started");
            return Err(exit_codes::GENERAL_ERROR);
        }
        // Session occupee, introuvable, etc.: non fatal, on garde le REPL.
        Err(e) => {
            eprintln!("Error: {e}");
            return Ok(());
        }
    };

    // Un 202 sans identifiant viole le contrat serveur: sans correlation on ne
    // peut pas filtrer le flux. On signale et on garde le REPL vivant.
    if my_id.is_empty() {
        eprintln!("[cannot stream: message id missing from the server response]");
        return Ok(());
    }

    let idle = Duration::from_secs(STREAM_IDLE_TIMEOUT_SECS);

    if json {
        stream_json(client, session_id, line_stream, &my_id, idle).await;
        return Ok(());
    }

    let resolver = InteractiveResolver { client, session_id };
    let mut stdout = io::stdout();
    let opts = LoopOpts { idle, use_color };
    let _ = run_render_loop(line_stream, &my_id, &mut stdout, &resolver, opts).await;
    Ok(())
}

/// Mode machine: emet les trames `data` brutes, aucun rendu humain. Pause sur
/// approbation pour lire une decision JSON sur stdin.
async fn stream_json<S>(
    client: &RuntimeClient,
    session_id: &str,
    mut lines: S,
    my_id: &str,
    idle: Duration,
) where
    S: futures::Stream<Item = Result<String, ClientError>> + Unpin,
{
    loop {
        let line = match tokio::time::timeout(idle, lines.next()).await {
            Ok(Some(Ok(l))) => l,
            Ok(Some(Err(_))) | Ok(None) | Err(_) => break,
        };
        if let Some(data) = line.strip_prefix("data: ") {
            println!("{data}");
        }
        match classify_chat_event(&line, my_id) {
            ChatStreamAction::ApprovalRequired {
                message_id,
                tool_name,
                ..
            } => {
                handle_chat_approval_json(client, session_id, &message_id, &tool_name).await;
            }
            ChatStreamAction::Completed { .. }
            | ChatStreamAction::Error(_)
            | ChatStreamAction::SessionClosed => break,
            _ => {}
        }
    }
}

/// Resolveur d'approbation de production: lit stdin et POST la decision.
struct InteractiveResolver<'a> {
    client: &'a RuntimeClient,
    session_id: &'a str,
}

impl ApprovalResolver for InteractiveResolver<'_> {
    async fn resolve(&self, message_id: &str, tool_name: &str, prompt: &str) {
        handle_chat_approval(self.client, self.session_id, message_id, tool_name, prompt).await;
    }
}

/// Insere un saut de ligne dans `out` si le curseur n'est pas deja en debut de
/// ligne, puis marque la ligne comme fraiche. Vide le tampon de ligne markdown:
/// la ligne partielle deja repeinte est ainsi validee avant un bloc outil.
fn break_line(st: &mut RenderState, out: &mut impl Write) -> io::Result<()> {
    if !st.at_line_start {
        writeln!(out)?;
    }
    st.at_line_start = true;
    st.line_buf.clear();
    Ok(())
}

// ─── Rendu markdown inline ───────────────────────────────────────────────────

/// Sequence de retour en debut de ligne + effacement de la ligne entiere, pour
/// repeindre la ligne courante en mode couleur.
const CLEAR_LINE: &str = "\r\x1b[2K";

/// Streame un token en mode couleur: finalise les lignes completes (rendu
/// markdown fige) et repeint la ligne partielle en cours a chaque token.
fn stream_markdown_token(tok: &str, st: &mut RenderState, out: &mut impl Write) -> io::Result<()> {
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
fn render_block(content: &str, use_color: bool, out: &mut impl Write) -> io::Result<()> {
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
fn render_markdown_line(line: &str, use_color: bool) -> String {
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
fn render_inline(text: &str, use_color: bool) -> String {
    let s = wrap_pairs(text, "`", "7", use_color); // video inverse pour le code
    let s = wrap_pairs(&s, "**", "1", use_color); // gras
    let s = wrap_pairs(&s, "__", "1", use_color); // gras
    let s = wrap_pairs(&s, "*", "3", use_color); // italique
    wrap_pairs(&s, "_", "3", use_color) // italique
}

/// Enveloppe chaque paire de `delim` dans le code SGR `sgr` (mode couleur) ou
/// retire les delimiteurs (mode brut). Un delimiteur non apparie est conserve
/// litteralement.
fn wrap_pairs(text: &str, delim: &str, sgr: &str, use_color: bool) -> String {
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
fn paint(text: &str, code: &str, use_color: bool) -> String {
    if use_color {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

/// Rend un appel d'outil qui demarre: glyphe, nom, apercu d'entree, intention.
fn render_tool_started(
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
struct ToolResultView<'a> {
    tool_name: &'a str,
    success: bool,
    output_preview: &'a str,
    analysis: Option<&'a str>,
}

/// Rend un appel d'outil termine: glyphe succes/echec, nom, apercu, analyse.
fn render_tool_completed(
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

// ─── Approbation en ligne ────────────────────────────────────────────────────

/// Lit une ligne de stdin, `None` si le flux est ferme. Bloque le thread: a
/// appeler sous `block_in_place` depuis un contexte async.
fn read_stdin_line() -> Option<String> {
    io::stdin().lock().lines().next().and_then(|r| r.ok())
}

/// Prompt d'approbation TTY: affiche la demande, lit la decision, la soumet.
///
/// Le stream est en pause pendant la saisie (le pump SSE est une tache separee
/// bufferisee, une pause breve est sans risque).
async fn handle_chat_approval(
    client: &RuntimeClient,
    session_id: &str,
    message_id: &str,
    tool_name: &str,
    prompt: &str,
) {
    if !prompt.is_empty() {
        println!("{prompt}");
    }
    loop {
        print!(
            "the agent wants to use {tool_name}: [A]llow once / [T]always / [R]efuse [reason] > "
        );
        let _ = io::stdout().flush();
        let Some(line) = tokio::task::block_in_place(read_stdin_line) else {
            // stdin ferme: refuser par securite.
            submit_authorize(
                client,
                &refuse_args(session_id, message_id, tool_name, Some("stdin closed")),
            )
            .await;
            return;
        };
        match parse_tool_decision(&line) {
            ToolDecisionInput::Accept => {
                submit_authorize(
                    client,
                    &AuthorizeToolArgs {
                        session_id,
                        message_id,
                        tool_name,
                        decision: "accept",
                        reason: None,
                        scope: None,
                    },
                )
                .await;
                return;
            }
            ToolDecisionInput::Refuse(reason) => {
                submit_authorize(
                    client,
                    &refuse_args(session_id, message_id, tool_name, reason.as_deref()),
                )
                .await;
                return;
            }
            ToolDecisionInput::Always => {
                let scope = prompt_scope();
                submit_authorize(
                    client,
                    &AuthorizeToolArgs {
                        session_id,
                        message_id,
                        tool_name,
                        decision: "always_accept",
                        reason: None,
                        scope: Some(scope),
                    },
                )
                .await;
                return;
            }
            ToolDecisionInput::Invalid => {
                println!("Invalid input. [A]llow / [T]always / [R]efuse");
            }
        }
    }
}

/// Construit les arguments d'un refus.
fn refuse_args<'a>(
    session_id: &'a str,
    message_id: &'a str,
    tool_name: &'a str,
    reason: Option<&'a str>,
) -> AuthorizeToolArgs<'a> {
    AuthorizeToolArgs {
        session_id,
        message_id,
        tool_name,
        decision: "refuse",
        reason,
        scope: None,
    }
}

/// Sous-prompt de portee pour "toujours autoriser". Renvoie la valeur wire.
fn prompt_scope() -> &'static str {
    loop {
        print!("  Always scope: [1] this session (default) / [2] this tool / [3] this project > ");
        let _ = io::stdout().flush();
        let Some(line) = tokio::task::block_in_place(read_stdin_line) else {
            return "this_session";
        };
        match parse_scope_choice(&line) {
            Some(s) => return s,
            None => println!("  Invalid choice."),
        }
    }
}

/// Prompt d'approbation en mode machine: lit une decision JSON sur stdin.
async fn handle_chat_approval_json(
    client: &RuntimeClient,
    session_id: &str,
    message_id: &str,
    tool_name: &str,
) {
    let Some(line) = tokio::task::block_in_place(read_stdin_line) else {
        submit_authorize(
            client,
            &refuse_args(session_id, message_id, tool_name, Some("stdin closed")),
        )
        .await;
        return;
    };
    let parsed =
        serde_json::from_str::<serde_json::Value>(&line).unwrap_or(serde_json::Value::Null);
    submit_authorize(
        client,
        &AuthorizeToolArgs {
            session_id,
            message_id,
            tool_name,
            decision: parsed["decision"].as_str().unwrap_or("refuse"),
            reason: parsed["reason"].as_str(),
            scope: parsed["scope"].as_str(),
        },
    )
    .await;
}

/// Soumet la decision d'approbation a l'API. Une erreur de transport est
/// signalee mais ne tue pas le REPL.
async fn submit_authorize(client: &RuntimeClient, args: &AuthorizeToolArgs<'_>) {
    if let Err(e) = client.authorize_tool(args).await {
        eprintln!("  x Failed to submit the decision: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN un token pour le tour courant WHEN classifie THEN Token.
    #[test]
    fn test_classify_token_for_current_message() {
        let line = r#"data: {"event":"token","message_id":"m1","token":"Bonjour"}"#;
        let action = classify_chat_event(line, "m1");
        assert_eq!(action, ChatStreamAction::Token("Bonjour".to_string()));
    }

    // GIVEN un token pour un autre message WHEN classifie THEN Ignore.
    #[test]
    fn test_classify_token_other_message_ignored() {
        let line = r#"data: {"event":"token","message_id":"other","token":"x"}"#;
        assert_eq!(classify_chat_event(line, "m1"), ChatStreamAction::Ignore);
    }

    // GIVEN response_completed WHEN classifie THEN Completed avec le contenu.
    #[test]
    fn test_classify_completed() {
        let line = r#"data: {"event":"response_completed","message_id":"m1","content":"fini"}"#;
        assert_eq!(
            classify_chat_event(line, "m1"),
            ChatStreamAction::Completed {
                content: "fini".to_string()
            }
        );
    }

    // GIVEN error WHEN classifie THEN Error.
    #[test]
    fn test_classify_error() {
        let line = r#"data: {"event":"error","message_id":"m1","error":"boom"}"#;
        assert_eq!(
            classify_chat_event(line, "m1"),
            ChatStreamAction::Error("boom".to_string())
        );
    }

    // GIVEN tool_call_started avec rationale WHEN classifie THEN ToolStarted
    // avec le resume d'intention extrait.
    #[test]
    fn test_classify_tool_started_with_rationale() {
        let line = r#"data: {"event":"tool_call_started","message_id":"m1","tool_name":"read_file","input_preview":"src/main.rs","rationale":{"summary":"je lis le fichier","inputs_recap":[],"expected_outcome":"contenu"}}"#;
        assert_eq!(
            classify_chat_event(line, "m1"),
            ChatStreamAction::ToolStarted {
                tool_name: "read_file".to_string(),
                input_preview: "src/main.rs".to_string(),
                rationale: Some("je lis le fichier".to_string()),
            }
        );
    }

    // GIVEN tool_call_completed en echec avec analyse WHEN classifie THEN
    // ToolCompleted avec success=false et le message humain.
    #[test]
    fn test_classify_tool_completed_with_analysis() {
        let line = r#"data: {"event":"tool_call_completed","message_id":"m1","tool_name":"http_get","success":false,"output_preview":"","analysis":{"category":"NetworkError","human_message":"réseau indisponible","technical_details":"timeout"}}"#;
        assert_eq!(
            classify_chat_event(line, "m1"),
            ChatStreamAction::ToolCompleted {
                tool_name: "http_get".to_string(),
                success: false,
                output_preview: String::new(),
                analysis: Some("réseau indisponible".to_string()),
            }
        );
    }

    // GIVEN une analyse avec conseil WHEN extraite THEN le conseil est annexe.
    #[test]
    fn test_extract_analysis_with_suggested_action() {
        let v = serde_json::json!({
            "analysis": {
                "category": "Timeout",
                "human_message": "trop long",
                "suggested_action": "réessayer",
                "technical_details": "t"
            }
        });
        assert_eq!(
            extract_analysis(&v),
            Some("trop long  (hint: réessayer)".to_string())
        );
    }

    // GIVEN approval_required WHEN classifie THEN ApprovalRequired complet.
    #[test]
    fn test_classify_approval_required() {
        let line = r#"data: {"event":"approval_required","message_id":"m1","tool_name":"shell","prompt":"Autoriser shell ?"}"#;
        assert_eq!(
            classify_chat_event(line, "m1"),
            ChatStreamAction::ApprovalRequired {
                message_id: "m1".to_string(),
                tool_name: "shell".to_string(),
                prompt: "Autoriser shell ?".to_string(),
            }
        );
    }

    // GIVEN session_closed WHEN classifie THEN SessionClosed, sans filtre d'id.
    #[test]
    fn test_classify_session_closed_ignores_id() {
        let line = r#"data: {"event":"session_closed"}"#;
        assert_eq!(
            classify_chat_event(line, "m1"),
            ChatStreamAction::SessionClosed
        );
    }

    // GIVEN message_sent (echo du prompt) WHEN classifie THEN Ignore.
    #[test]
    fn test_classify_message_sent_ignored() {
        let line = r#"data: {"event":"message_sent","message_id":"m1"}"#;
        assert_eq!(classify_chat_event(line, "m1"), ChatStreamAction::Ignore);
    }

    // GIVEN une ligne non-data WHEN classifie THEN Ignore.
    #[test]
    fn test_classify_non_data_line_ignored() {
        assert_eq!(
            classify_chat_event(": keep-alive", "m1"),
            ChatStreamAction::Ignore
        );
        assert_eq!(classify_chat_event("", "m1"), ChatStreamAction::Ignore);
    }

    // GIVEN un JSON malforme WHEN classifie THEN Ignore, pas de panique.
    #[test]
    fn test_classify_malformed_json_ignored() {
        let line = "data: {not json";
        assert_eq!(classify_chat_event(line, "m1"), ChatStreamAction::Ignore);
    }

    // GIVEN les variantes de decision WHEN parsees THEN la bonne action.
    #[test]
    fn test_parse_tool_decision_variants() {
        assert_eq!(parse_tool_decision("a"), ToolDecisionInput::Accept);
        assert_eq!(parse_tool_decision("Autoriser"), ToolDecisionInput::Accept);
        assert_eq!(parse_tool_decision("oui"), ToolDecisionInput::Accept);
        assert_eq!(parse_tool_decision("t"), ToolDecisionInput::Always);
        assert_eq!(parse_tool_decision("toujours"), ToolDecisionInput::Always);
        assert_eq!(parse_tool_decision("r"), ToolDecisionInput::Refuse(None));
        assert_eq!(
            parse_tool_decision("refuser trop risqué"),
            ToolDecisionInput::Refuse(Some("trop risqué".to_string()))
        );
        assert_eq!(parse_tool_decision("xyz"), ToolDecisionInput::Invalid);
        assert_eq!(parse_tool_decision(""), ToolDecisionInput::Invalid);
    }

    // GIVEN les choix de portee WHEN parses THEN la valeur wire snake_case.
    #[test]
    fn test_parse_scope_choice() {
        assert_eq!(parse_scope_choice(""), Some("this_session"));
        assert_eq!(parse_scope_choice("1"), Some("this_session"));
        assert_eq!(parse_scope_choice("2"), Some("this_tool"));
        assert_eq!(parse_scope_choice("3"), Some("this_project"));
        assert_eq!(parse_scope_choice("9"), None);
    }

    // ─── handle_action (coeur synchrone) ─────────────────────────────────────

    /// Applique une suite d'actions et renvoie la sortie ecrite + le dernier
    /// controle. `use_color` desactive pour des assertions stables.
    fn drive(actions: Vec<ChatStreamAction>) -> (String, LoopControl) {
        let mut st = RenderState::new(false);
        let mut out: Vec<u8> = Vec::new();
        let mut last = LoopControl::Continue;
        for a in actions {
            last = handle_action(a, &mut st, &mut out).expect("write to Vec ne peut pas echouer");
        }
        (String::from_utf8(out).unwrap(), last)
    }

    // GIVEN des tokens WHEN traites THEN ecrits verbatim, sans saut ajoute.
    #[test]
    fn test_handle_tokens_written_verbatim() {
        let (out, ctrl) = drive(vec![
            ChatStreamAction::Token("Bon".into()),
            ChatStreamAction::Token("jour".into()),
        ]);
        assert_eq!(out, "Bonjour");
        assert_eq!(ctrl, LoopControl::Continue);
    }

    // GIVEN un token puis un tool_call WHEN traites THEN un saut coupe la ligne
    // de tokens avant le bloc outil (interleaving propre).
    #[test]
    fn test_handle_token_then_tool_breaks_line() {
        let (out, _) = drive(vec![
            ChatStreamAction::Token("texte".into()),
            ChatStreamAction::ToolStarted {
                tool_name: "read_file".into(),
                input_preview: "src/main.rs".into(),
                rationale: Some("je lis".into()),
            },
        ]);
        assert_eq!(out, "texte\n  ● read_file  src/main.rs\n  └── je lis\n");
    }

    // GIVEN un tool termine en echec avec analyse WHEN traite THEN glyphe ✗ +
    // analyse en continuation.
    #[test]
    fn test_handle_tool_completed_failure() {
        let (out, _) = drive(vec![ChatStreamAction::ToolCompleted {
            tool_name: "http_get".into(),
            success: false,
            output_preview: String::new(),
            analysis: Some("reseau indisponible".into()),
        }]);
        assert_eq!(out, "  ✗ http_get\n  └── reseau indisponible\n");
    }

    // GIVEN des tokens deja streames puis response_completed WHEN traite THEN le
    // contenu n'est PAS reimprime (anti-doublon), juste un saut final.
    #[test]
    fn test_handle_completed_after_tokens_no_duplicate() {
        let (out, ctrl) = drive(vec![
            ChatStreamAction::Token("deja".into()),
            ChatStreamAction::Completed {
                content: "deja streame".into(),
            },
        ]);
        assert_eq!(out, "deja\n");
        assert_eq!(ctrl, LoopControl::Done);
    }

    // GIVEN aucun token puis response_completed WHEN traite THEN le contenu
    // complet est imprime en secours.
    #[test]
    fn test_handle_completed_without_tokens_prints_content() {
        let (out, ctrl) = drive(vec![ChatStreamAction::Completed {
            content: "reponse en bloc".into(),
        }]);
        assert_eq!(out, "reponse en bloc\n");
        assert_eq!(ctrl, LoopControl::Done);
    }

    // GIVEN approval_required WHEN traite THEN LoopControl::Approval, sans rendu.
    #[test]
    fn test_handle_approval_returns_control() {
        let (out, ctrl) = drive(vec![ChatStreamAction::ApprovalRequired {
            message_id: "m1".into(),
            tool_name: "shell".into(),
            prompt: "Autoriser ?".into(),
        }]);
        assert_eq!(out, "");
        assert_eq!(
            ctrl,
            LoopControl::Approval {
                message_id: "m1".into(),
                tool_name: "shell".into(),
                prompt: "Autoriser ?".into(),
            }
        );
    }

    // GIVEN error WHEN traite THEN LoopControl::Failed portant le texte.
    #[test]
    fn test_handle_error_returns_failed() {
        let (_, ctrl) = drive(vec![ChatStreamAction::Error("boom".into())]);
        assert_eq!(ctrl, LoopControl::Failed("boom".into()));
    }

    // ─── run_render_loop (integration sur flux SSE injecte) ──────────────────

    /// Resolveur d'approbation qui enregistre les demandes au lieu d'agir.
    struct RecordingResolver {
        calls: std::cell::RefCell<Vec<(String, String, String)>>,
    }

    impl ApprovalResolver for RecordingResolver {
        async fn resolve(&self, message_id: &str, tool_name: &str, prompt: &str) {
            self.calls.borrow_mut().push((
                message_id.to_string(),
                tool_name.to_string(),
                prompt.to_string(),
            ));
        }
    }

    /// Construit un flux de lignes SSE a partir de chaines brutes. Les chaines
    /// sont materialisees (owned) pour ne pas capturer le lifetime des `&str`.
    fn sse_stream(
        lines: Vec<&str>,
    ) -> impl futures::Stream<Item = Result<String, ClientError>> + Unpin {
        let owned: Vec<Result<String, ClientError>> =
            lines.into_iter().map(|l| Ok(l.to_string())).collect();
        futures::stream::iter(owned)
    }

    async fn run_case(lines: Vec<&str>, my_id: &str) -> (String, Vec<(String, String, String)>) {
        let resolver = RecordingResolver {
            calls: std::cell::RefCell::new(Vec::new()),
        };
        let mut out: Vec<u8> = Vec::new();
        let opts = LoopOpts {
            idle: Duration::from_secs(5),
            use_color: false,
        };
        run_render_loop(sse_stream(lines), my_id, &mut out, &resolver, opts)
            .await
            .unwrap();
        (String::from_utf8(out).unwrap(), resolver.calls.into_inner())
    }

    // GIVEN une session de tokens terminee WHEN parcourue THEN texte concatene,
    // les lignes hors tour (autre id, keep-alive) ignorees.
    #[tokio::test]
    async fn test_loop_tokens_session() {
        let lines = vec![
            r#"data: {"event":"message_sent","message_id":"m1"}"#,
            r#"data: {"event":"response_started","message_id":"m1"}"#,
            ": keep-alive",
            r#"data: {"event":"token","message_id":"m1","token":"Bon"}"#,
            r#"data: {"event":"token","message_id":"other","token":"XXX"}"#,
            r#"data: {"event":"token","message_id":"m1","token":"jour"}"#,
            r#"data: {"event":"response_completed","message_id":"m1","content":"Bonjour"}"#,
        ];
        let (out, calls) = run_case(lines, "m1").await;
        assert_eq!(out, "Bonjour\n");
        assert!(calls.is_empty());
    }

    // GIVEN un tour avec tool_call puis reponse WHEN parcouru THEN bloc outil
    // rendu entre les tokens, resultat affiche.
    #[tokio::test]
    async fn test_loop_tool_call_then_answer() {
        let lines = vec![
            r#"data: {"event":"tool_call_started","message_id":"m1","tool_name":"read_file","input_preview":"a.rs","rationale":{"summary":"je lis","inputs_recap":[],"expected_outcome":"x"}}"#,
            r#"data: {"event":"tool_call_completed","message_id":"m1","tool_name":"read_file","success":true,"output_preview":"12 lignes"}"#,
            r#"data: {"event":"token","message_id":"m1","token":"Fait."}"#,
            r#"data: {"event":"response_completed","message_id":"m1","content":"Fait."}"#,
        ];
        let (out, calls) = run_case(lines, "m1").await;
        assert_eq!(
            out,
            "  ● read_file  a.rs\n  └── je lis\n  ✔ read_file  12 lignes\nFait.\n"
        );
        assert!(calls.is_empty());
    }

    // GIVEN une approbation au milieu WHEN parcourue THEN le resolveur est
    // appele une fois, puis les tokens suivants sont rendus.
    #[tokio::test]
    async fn test_loop_approval_then_resumes() {
        let lines = vec![
            r#"data: {"event":"approval_required","message_id":"m1","tool_name":"shell","prompt":"Autoriser shell ?"}"#,
            r#"data: {"event":"approval_resolved","message_id":"m1","tool_name":"shell","decision":"accept"}"#,
            r#"data: {"event":"token","message_id":"m1","token":"ok"}"#,
            r#"data: {"event":"response_completed","message_id":"m1","content":"ok"}"#,
        ];
        let (out, calls) = run_case(lines, "m1").await;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, "shell");
        assert_eq!(out, "ok\n");
    }

    // GIVEN une erreur en cours de flux WHEN parcourue THEN la boucle termine
    // sans paniquer (le message d'erreur part sur stderr).
    #[tokio::test]
    async fn test_loop_error_mid_stream_terminates() {
        let lines = vec![
            r#"data: {"event":"token","message_id":"m1","token":"deb"}"#,
            r#"data: {"event":"error","message_id":"m1","error":"backend indisponible"}"#,
            r#"data: {"event":"token","message_id":"m1","token":"apres"}"#,
        ];
        let (out, _) = run_case(lines, "m1").await;
        // Le token d'avant est rendu, puis un saut de ligne clot la ligne; le
        // token d'apres n'est jamais atteint (boucle terminee sur l'erreur).
        assert_eq!(out, "deb\n");
    }

    // ─── Rendu markdown (fonctions pures) ────────────────────────────────────

    // GIVEN du gras/italique/code inline en mode brut WHEN rendu THEN les
    // delimiteurs sont retires, pas d'ANSI.
    #[test]
    fn test_markdown_inline_no_color_strips_markers() {
        assert_eq!(
            render_markdown_line("un **gras** ici", false),
            "un gras ici"
        );
        assert_eq!(render_markdown_line("un *ital* la", false), "un ital la");
        assert_eq!(
            render_markdown_line("du `code` inline", false),
            "du code inline"
        );
    }

    // GIVEN du gras en mode couleur WHEN rendu THEN enveloppe SGR bold.
    #[test]
    fn test_markdown_bold_color_wraps_sgr() {
        assert_eq!(
            render_markdown_line("a **b** c", true),
            "a \x1b[1mb\x1b[0m c"
        );
    }

    // GIVEN un titre WHEN rendu THEN ligne entiere en gras (couleur), texte nu
    // sinon.
    #[test]
    fn test_markdown_heading() {
        assert_eq!(
            render_markdown_line("## Titre", true),
            "\x1b[1mTitre\x1b[0m"
        );
        assert_eq!(render_markdown_line("## Titre", false), "Titre");
    }

    // GIVEN une puce WHEN rendue THEN glyphe • en preservant l'indentation.
    #[test]
    fn test_markdown_bullet() {
        assert_eq!(render_markdown_line("- item", false), "• item");
        assert_eq!(render_markdown_line("  * sous", false), "  • sous");
    }

    // GIVEN un delimiteur non apparie WHEN rendu THEN conserve litteralement.
    #[test]
    fn test_markdown_unpaired_marker_kept() {
        assert_eq!(
            render_markdown_line("deux ** sans fin", true),
            "deux ** sans fin"
        );
    }

    // GIVEN un token markdown streame en couleur WHEN traite THEN la ligne est
    // repeinte (efface-ligne + rendu) via handle_action.
    #[test]
    fn test_stream_markdown_repaints_line() {
        let mut st = RenderState::new(true);
        let mut out: Vec<u8> = Vec::new();
        handle_action(ChatStreamAction::Token("**ok**".into()), &mut st, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        // Efface-ligne puis gras applique (delimiteurs fermes dans le token).
        assert_eq!(s, "\r\x1b[2K\x1b[1mok\x1b[0m");
        assert!(!st.at_line_start);
    }

    // GIVEN un token markdown couleur finissant par un saut WHEN traite THEN la
    // ligne est figee avec un newline et le curseur revient en debut de ligne.
    #[test]
    fn test_stream_markdown_finalizes_on_newline() {
        let mut st = RenderState::new(true);
        let mut out: Vec<u8> = Vec::new();
        handle_action(ChatStreamAction::Token("# T\n".into()), &mut st, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        // Ligne finalisee (titre en gras) + newline, puis repeinte vide.
        assert_eq!(s, "\r\x1b[2K\x1b[1mT\x1b[0m\n\r\x1b[2K");
        assert!(st.at_line_start);
    }
}
