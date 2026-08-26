//! Render state, loop control, and the stream render loop.

use std::io::{self, Write};
use std::time::Duration;

use futures::StreamExt;

use crate::client::ClientError;

use super::break_line;
use super::classify::{classify_chat_event, ChatStreamAction};
use super::markdown::{
    render_block, render_tool_completed, render_tool_started, stream_markdown_token, ToolResultView,
};

// ─── State and loop control ─────────────────────────────────────────────────

/// Render state kept across the events of one turn.
pub(super) struct RenderState {
    /// At least one token was streamed (avoids reprinting the final content).
    pub(super) any_token: bool,
    /// The cursor is at the start of a line (to cut cleanly before a block).
    pub(super) at_line_start: bool,
    /// Coloration ANSI active.
    pub(super) use_color: bool,
    /// Line being built, for markdown rendering in colour mode: this line is
    /// repainted at every token so the style is applied once the delimiters
    /// are closed. Empty in plain mode (`--no-color`).
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

/// What to do after handling an event.
#[derive(Debug, PartialEq)]
pub(super) enum LoopControl {
    /// Keep reading the stream.
    Continue,
    /// Turn finished successfully.
    Done,
    /// Turn finished on an error (text to report on stderr).
    Failed(String),
    /// A tool approval is required before going further.
    Approval {
        message_id: String,
        tool_name: String,
        prompt: String,
    },
}

/// Handles one classified event: writes the human rendering into `out` and
/// returns what to do next. Synchronous, testable core, no network I/O, no stdin.
pub(super) fn handle_action(
    action: ChatStreamAction,
    st: &mut RenderState,
    out: &mut impl Write,
) -> io::Result<LoopControl> {
    match action {
        ChatStreamAction::Token(tok) => {
            st.any_token = true;
            if st.use_color {
                // Colour mode (TTY): the current line is repainted at every
                // token so the markdown style applies once the delimiters
                // are closed.
                stream_markdown_token(&tok, st, out)?;
            } else {
                // Plain mode (--no-color / pipe): verbatim stream, proven and safe.
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
            // The tokens were already streamed live. The full content is only
            // reprinted as a fallback, when no token arrived.
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

/// Resolves a tool approval. Abstracted to make the loop testable without
/// stdin and without network (production reads stdin and POSTs the decision).
pub(super) trait ApprovalResolver {
    fn resolve(
        &self,
        message_id: &str,
        tool_name: &str,
        prompt: &str,
    ) -> impl std::future::Future<Output = ()>;
}

/// Render options of [`run_render_loop`].
#[derive(Clone, Copy)]
pub(super) struct LoopOpts {
    /// Inactivity guard armed at every `next`.
    pub(super) idle: Duration,
    /// Coloration ANSI active.
    pub(super) use_color: bool,
}

/// Human render loop over a stream of SSE lines. Generic over the stream, the
/// output and the approval resolver: that is the injection point for the tests.
///
/// The inactivity guard is armed at every `next`. The axum SSE keep-alives
/// (~15s) arrive as lines and reset it, so a slow model load does not trigger
/// a false timeout: the guard only aims at a connection that is really dead.
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
            // Stream closed by the server.
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
                // The prompt printed line breaks: start again from a clean line.
                st.at_line_start = true;
            }
        }
    }
    Ok(())
}
