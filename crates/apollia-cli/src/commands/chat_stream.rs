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

use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use futures::StreamExt;

use crate::client::{ClientError, RuntimeClient};
use crate::exit_codes;

mod approval;
mod classify;
mod markdown;
mod render_loop;

use approval::{handle_chat_approval, handle_chat_approval_json};
use classify::{classify_chat_event, ChatStreamAction};
use render_loop::{run_render_loop, ApprovalResolver, LoopOpts, RenderState};

/// Garde d'inactivite: si aucune ligne SSE n'arrive pendant ce delai, la boucle
/// rend la main au REPL plutot que de rester figee sur un stream muet. Le stream
/// de session ne se ferme pas par message, cette garde est donc indispensable.
const STREAM_IDLE_TIMEOUT_SECS: u64 = 120;
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
#[cfg(test)]
mod tests {
    use super::classify::{
        extract_analysis, parse_scope_choice, parse_tool_decision, ToolDecisionInput,
    };
    use super::markdown::render_markdown_line;
    use super::render_loop::{handle_action, LoopControl};
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
