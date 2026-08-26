//! Etat de rendu, controle de boucle et boucle de rendu du stream.

use std::io::{self, Write};
use std::time::Duration;

use futures::StreamExt;

use crate::client::ClientError;

use super::break_line;
use super::classify::{classify_chat_event, ChatStreamAction};
use super::markdown::{
    render_block, render_tool_completed, render_tool_started, stream_markdown_token, ToolResultView,
};

// ─── Etat et controle de boucle ──────────────────────────────────────────────

/// Etat de rendu maintenu au fil des evenements d'un tour.
pub(super) struct RenderState {
    /// Au moins un token a ete streame (evite de reimprimer le contenu final).
    pub(super) any_token: bool,
    /// Le curseur est en debut de ligne (pour couper proprement avant un bloc).
    pub(super) at_line_start: bool,
    /// Coloration ANSI active.
    pub(super) use_color: bool,
    /// Ligne en cours de construction, pour le rendu markdown en mode couleur:
    /// on repeint cette ligne a chaque token pour appliquer le style une fois
    /// les delimiteurs fermes. Vide en mode brut (`--no-color`).
    pub(super) line_buf: String,
}

impl RenderState {
    pub(super) fn new(use_color: bool) -> Self {
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
pub(super) enum LoopControl {
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
pub(super) fn handle_action(
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
pub(super) trait ApprovalResolver {
    fn resolve(
        &self,
        message_id: &str,
        tool_name: &str,
        prompt: &str,
    ) -> impl std::future::Future<Output = ()>;
}

/// Options de rendu de [`run_render_loop`].
#[derive(Clone, Copy)]
pub(super) struct LoopOpts {
    /// Garde d'inactivite armee a chaque `next`.
    pub(super) idle: Duration,
    /// Coloration ANSI active.
    pub(super) use_color: bool,
}

/// Boucle de rendu humain sur un flux de lignes SSE. Generique sur le flux, la
/// sortie et le resolveur d'approbation: c'est le point d'injection des tests.
///
/// La garde d'inactivite est armee a chaque `next`. Les keep-alives SSE d'axum
/// (~15s) arrivent comme des lignes et la resettent, donc un chargement de
/// modele lent ne declenche pas de faux timeout: la garde ne vise qu'une
/// connexion reellement morte.
pub(super) async fn run_render_loop<S, W, R>(
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
