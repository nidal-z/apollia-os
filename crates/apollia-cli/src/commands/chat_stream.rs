//! Streaming chat send loop.
//!
//! Consumes the session SSE stream (`GET /api/v1/sessions/:id/stream`) token by
//! token so the assistant reply appears live in the terminal, renders tool
//! calls (intent rationale + result + error analysis), and handles inline tool
//! approval prompts ("the agent wants to use X: allow once / always /
//! refuse"). Replaces the former poll-then-print path.
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

/// Inactivity guard: when no SSE line arrives within this delay, the loop
/// hands control back to the REPL instead of hanging on a silent stream. The
/// session stream is not closed per message, so this guard is indispensable.
const STREAM_IDLE_TIMEOUT_SECS: u64 = 120;
// ─── Boucle d'envoi streamee ─────────────────────────────────────────────────

/// Sends `message`, then renders the reply as it streams, token by token.
///
/// Keeps the `Result<(), i32>` contract: `Err(code)` only on a fatal error
/// (runtime unreachable). A server-side error (session busy, generation
/// failed) leaves the REPL alive (`Ok`).
pub async fn stream_send(
    client: &RuntimeClient,
    session_id: &str,
    message: &str,
    json: bool,
    no_color: bool,
) -> Result<(), i32> {
    let use_color = !no_color && io::stdout().is_terminal();

    // Open the SSE stream BEFORE the POST. The send is fire-and-forget and the
    // bus replays nothing: subscribing first guarantees that no early token is
    // lost between enqueueing the work and attaching the subscription.
    let uri = format!("/api/v1/sessions/{session_id}/stream");
    let line_stream = match client.stream_sse_lines(&uri).await {
        Ok(s) => s,
        // Runtime unreachable: fatal, leave the REPL.
        Err(ClientError::ConnectionRefused) => {
            eprintln!("runtime not started");
            return Err(exit_codes::GENERAL_ERROR);
        }
        // Any other transport error: not fatal, the REPL carries on.
        Err(e) => {
            eprintln!("Error: {e}");
            return Ok(());
        }
    };

    // Trigger the work: the 202 carries the message_id of the current turn,
    // which also correlates the tokens and events of the reply.
    let my_id = match client.send_chat_message(session_id, message).await {
        Ok(resp) => resp["message_id"].as_str().unwrap_or("").to_string(),
        Err(ClientError::ConnectionRefused) => {
            eprintln!("runtime not started");
            return Err(exit_codes::GENERAL_ERROR);
        }
        // Session busy, not found, and so on: not fatal, keep the REPL.
        Err(e) => {
            eprintln!("Error: {e}");
            return Ok(());
        }
    };

    // A 202 with no identifier breaks the server contract: without a correlation
    // the stream cannot be filtered. Report it and keep the REPL alive.
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

/// Machine mode: emits the raw `data` frames, no human rendering. Pauses on an
/// approval to read a JSON decision from stdin.
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

/// Production approval resolver: reads stdin and POSTs the decision.
struct InteractiveResolver<'a> {
    client: &'a RuntimeClient,
    session_id: &'a str,
}

impl ApprovalResolver for InteractiveResolver<'_> {
    async fn resolve(&self, message_id: &str, tool_name: &str, prompt: &str) {
        handle_chat_approval(self.client, self.session_id, message_id, tool_name, prompt).await;
    }
}

/// Inserts a line break into `out` when the cursor is not already at the start
/// of a line, then marks the line as fresh. Empties the markdown line buffer:
/// the partial line already repainted is committed before a tool block.
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

    // GIVEN a token for the current turn WHEN classified THEN Token.
    #[test]
    fn test_classify_token_for_current_message() {
        let line = r#"data: {"event":"token","message_id":"m1","token":"Bonjour"}"#;
        let action = classify_chat_event(line, "m1");
        assert_eq!(action, ChatStreamAction::Token("Bonjour".to_string()));
    }

    // GIVEN a token for another message WHEN classified THEN Ignore.
    #[test]
    fn test_classify_token_other_message_ignored() {
        let line = r#"data: {"event":"token","message_id":"other","token":"x"}"#;
        assert_eq!(classify_chat_event(line, "m1"), ChatStreamAction::Ignore);
    }

    // GIVEN response_completed WHEN classified THEN Completed with the content.
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

    // GIVEN tool_call_started with a rationale WHEN classified THEN ToolStarted
    // carrying the extracted intent summary.
    #[test]
    fn test_classify_tool_started_with_rationale() {
        let line = r#"data: {"event":"tool_call_started","message_id":"m1","tool_name":"read_file","input_preview":"src/main.rs","rationale":{"summary":"reading the file","inputs_recap":[],"expected_outcome":"contents"}}"#;
        assert_eq!(
            classify_chat_event(line, "m1"),
            ChatStreamAction::ToolStarted {
                tool_name: "read_file".to_string(),
                input_preview: "src/main.rs".to_string(),
                rationale: Some("reading the file".to_string()),
            }
        );
    }

    // GIVEN tool_call_completed failed with an analysis WHEN classified THEN
    // ToolCompleted with success=false and the human message.
    #[test]
    fn test_classify_tool_completed_with_analysis() {
        let line = r#"data: {"event":"tool_call_completed","message_id":"m1","tool_name":"http_get","success":false,"output_preview":"","analysis":{"category":"NetworkError","human_message":"network unavailable","technical_details":"timeout"}}"#;
        assert_eq!(
            classify_chat_event(line, "m1"),
            ChatStreamAction::ToolCompleted {
                tool_name: "http_get".to_string(),
                success: false,
                output_preview: String::new(),
                analysis: Some("network unavailable".to_string()),
            }
        );
    }

    // GIVEN an analysis with an advice WHEN extracted THEN the advice is appended.
    #[test]
    fn test_extract_analysis_with_suggested_action() {
        let v = serde_json::json!({
            "analysis": {
                "category": "Timeout",
                "human_message": "too long",
                "suggested_action": "retry",
                "technical_details": "t"
            }
        });
        assert_eq!(
            extract_analysis(&v),
            Some("too long  (hint: retry)".to_string())
        );
    }

    // GIVEN approval_required WHEN classified THEN a complete ApprovalRequired.
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

    // GIVEN session_closed WHEN classified THEN SessionClosed, with no id filter.
    #[test]
    fn test_classify_session_closed_ignores_id() {
        let line = r#"data: {"event":"session_closed"}"#;
        assert_eq!(
            classify_chat_event(line, "m1"),
            ChatStreamAction::SessionClosed
        );
    }

    // GIVEN message_sent (prompt echo) WHEN classified THEN Ignore.
    #[test]
    fn test_classify_message_sent_ignored() {
        let line = r#"data: {"event":"message_sent","message_id":"m1"}"#;
        assert_eq!(classify_chat_event(line, "m1"), ChatStreamAction::Ignore);
    }

    // GIVEN a non-data line WHEN classified THEN Ignore.
    #[test]
    fn test_classify_non_data_line_ignored() {
        assert_eq!(
            classify_chat_event(": keep-alive", "m1"),
            ChatStreamAction::Ignore
        );
        assert_eq!(classify_chat_event("", "m1"), ChatStreamAction::Ignore);
    }

    // GIVEN malformed JSON WHEN classified THEN Ignore, and no panic.
    #[test]
    fn test_classify_malformed_json_ignored() {
        let line = "data: {not json";
        assert_eq!(classify_chat_event(line, "m1"), ChatStreamAction::Ignore);
    }

    // GIVEN the decision variants WHEN parsed THEN the right action.
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

    // GIVEN the scope choices WHEN parsed THEN the snake_case wire value.
    #[test]
    fn test_parse_scope_choice() {
        assert_eq!(parse_scope_choice(""), Some("this_session"));
        assert_eq!(parse_scope_choice("1"), Some("this_session"));
        assert_eq!(parse_scope_choice("2"), Some("this_tool"));
        assert_eq!(parse_scope_choice("3"), Some("this_project"));
        assert_eq!(parse_scope_choice("9"), None);
    }

    // ─── handle_action (coeur synchrone) ─────────────────────────────────────

    /// Applies a sequence of actions and returns the written output plus the
    /// last control. `use_color` is off for stable assertions.
    fn drive(actions: Vec<ChatStreamAction>) -> (String, LoopControl) {
        let mut st = RenderState::new(false);
        let mut out: Vec<u8> = Vec::new();
        let mut last = LoopControl::Continue;
        for a in actions {
            last = handle_action(a, &mut st, &mut out).expect("writing to a Vec cannot fail");
        }
        (String::from_utf8(out).unwrap(), last)
    }

    // GIVEN tokens WHEN handled THEN written verbatim, with no added break.
    #[test]
    fn test_handle_tokens_written_verbatim() {
        let (out, ctrl) = drive(vec![
            ChatStreamAction::Token("Bon".into()),
            ChatStreamAction::Token("jour".into()),
        ]);
        assert_eq!(out, "Bonjour");
        assert_eq!(ctrl, LoopControl::Continue);
    }

    // GIVEN a token then a tool_call WHEN handled THEN a break cuts the token
    // line before the tool block (clean interleaving).
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

    // GIVEN a tool finished on a failure with an analysis WHEN handled THEN a
    // ✗ glyph plus the analysis on a continuation line.
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

    // GIVEN tokens already streamed then response_completed WHEN handled THEN
    // the content is NOT reprinted (no duplicate), just a final break.
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

    // GIVEN no token then response_completed WHEN handled THEN the whole
    // content is printed as a fallback.
    #[test]
    fn test_handle_completed_without_tokens_prints_content() {
        let (out, ctrl) = drive(vec![ChatStreamAction::Completed {
            content: "block reply".into(),
        }]);
        assert_eq!(out, "block reply\n");
        assert_eq!(ctrl, LoopControl::Done);
    }

    // GIVEN approval_required WHEN handled THEN LoopControl::Approval, no render.
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

    // GIVEN error WHEN handled THEN LoopControl::Failed carrying the text.
    #[test]
    fn test_handle_error_returns_failed() {
        let (_, ctrl) = drive(vec![ChatStreamAction::Error("boom".into())]);
        assert_eq!(ctrl, LoopControl::Failed("boom".into()));
    }

    // ─── run_render_loop (integration over an injected SSE stream) ───────

    /// Approval resolver that records the requests instead of acting on them.
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

    /// Builds a stream of SSE lines from raw strings. The strings are
    /// materialised (owned) so as not to capture the lifetime of the `&str`.
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

    // GIVEN a finished token session WHEN walked THEN the text is concatenated,
    // and the out-of-turn lines (other id, keep-alive) are ignored.
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

    // GIVEN a turn with a tool_call then a reply WHEN walked THEN the tool
    // block is rendered between the tokens, and the result is displayed.
    #[tokio::test]
    async fn test_loop_tool_call_then_answer() {
        let lines = vec![
            r#"data: {"event":"tool_call_started","message_id":"m1","tool_name":"read_file","input_preview":"a.rs","rationale":{"summary":"reading","inputs_recap":[],"expected_outcome":"x"}}"#,
            r#"data: {"event":"tool_call_completed","message_id":"m1","tool_name":"read_file","success":true,"output_preview":"12 lines"}"#,
            r#"data: {"event":"token","message_id":"m1","token":"Done."}"#,
            r#"data: {"event":"response_completed","message_id":"m1","content":"Done."}"#,
        ];
        let (out, calls) = run_case(lines, "m1").await;
        assert_eq!(
            out,
            "  ● read_file  a.rs\n  └── reading\n  ✔ read_file  12 lines\nDone.\n"
        );
        assert!(calls.is_empty());
    }

    // GIVEN an approval midway WHEN walked THEN the resolver is called once,
    // then the following tokens are rendered.
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

    // GIVEN an error mid-stream WHEN walked THEN the loop ends without
    // panicking (the error message goes to stderr).
    #[tokio::test]
    async fn test_loop_error_mid_stream_terminates() {
        let lines = vec![
            r#"data: {"event":"token","message_id":"m1","token":"deb"}"#,
            r#"data: {"event":"error","message_id":"m1","error":"backend indisponible"}"#,
            r#"data: {"event":"token","message_id":"m1","token":"after"}"#,
        ];
        let (out, _) = run_case(lines, "m1").await;
        // The token before is rendered, then a line break closes the line; the
        // token after is never reached (the loop ended on the error).
        assert_eq!(out, "deb\n");
    }

    // ─── Markdown rendering (pure functions) ─────────────────────────────

    // GIVEN inline bold/italic/code in plain mode WHEN rendered THEN the
    // delimiters are stripped, and no ANSI is emitted.
    #[test]
    fn test_markdown_inline_no_color_strips_markers() {
        assert_eq!(
            render_markdown_line("a **bold** here", false),
            "a bold here"
        );
        assert_eq!(
            render_markdown_line("an *ital* there", false),
            "an ital there"
        );
        assert_eq!(
            render_markdown_line("some `code` inline", false),
            "some code inline"
        );
    }

    // GIVEN bold in colour mode WHEN rendered THEN an SGR bold wrapper.
    #[test]
    fn test_markdown_bold_color_wraps_sgr() {
        assert_eq!(
            render_markdown_line("a **b** c", true),
            "a \x1b[1mb\x1b[0m c"
        );
    }

    // GIVEN a heading WHEN rendered THEN the whole line in bold (colour), bare
    // text otherwise.
    #[test]
    fn test_markdown_heading() {
        assert_eq!(
            render_markdown_line("## Heading", true),
            "\x1b[1mHeading\x1b[0m"
        );
        assert_eq!(render_markdown_line("## Heading", false), "Heading");
    }

    // GIVEN a bullet WHEN rendered THEN a • glyph, preserving the indentation.
    #[test]
    fn test_markdown_bullet() {
        assert_eq!(render_markdown_line("- item", false), "• item");
        assert_eq!(render_markdown_line("  * sub", false), "  • sub");
    }

    // GIVEN an unpaired delimiter WHEN rendered THEN kept literally.
    #[test]
    fn test_markdown_unpaired_marker_kept() {
        assert_eq!(
            render_markdown_line("two ** without end", true),
            "two ** without end"
        );
    }

    // GIVEN a markdown token streamed in colour WHEN handled THEN the line is
    // repainted (erase-line + render) through handle_action.
    #[test]
    fn test_stream_markdown_repaints_line() {
        let mut st = RenderState::new(true);
        let mut out: Vec<u8> = Vec::new();
        handle_action(ChatStreamAction::Token("**ok**".into()), &mut st, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        // Erase-line then bold applied (delimiters closed within the token).
        assert_eq!(s, "\r\x1b[2K\x1b[1mok\x1b[0m");
        assert!(!st.at_line_start);
    }

    // GIVEN a colour markdown token ending on a break WHEN handled THEN the
    // line is frozen with a newline and the cursor returns to the line start.
    #[test]
    fn test_stream_markdown_finalizes_on_newline() {
        let mut st = RenderState::new(true);
        let mut out: Vec<u8> = Vec::new();
        handle_action(ChatStreamAction::Token("# T\n".into()), &mut st, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        // Line finalised (heading in bold) + newline, then repainted empty.
        assert_eq!(s, "\r\x1b[2K\x1b[1mT\x1b[0m\n\r\x1b[2K");
        assert!(st.at_line_start);
    }
}
