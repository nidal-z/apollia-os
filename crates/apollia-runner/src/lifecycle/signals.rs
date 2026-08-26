//! Cross-platform signal handling for graceful shutdown.
//!
//! The daemon typically sends `SIGTERM` (Unix) or `CTRL_C_EVENT` (Windows)
//! when it wants to stop the runner. The runner intercepts it and triggers a
//! clean shutdown of the axum server.

/// Future that completes when a shutdown signal is received.
///
/// On Unix: `SIGINT` (Ctrl+C) or `SIGTERM`.
/// On Windows: `Ctrl+C` or `Ctrl+Break`.
/// A handler that could not be installed leaves its signal the default
/// disposition, and the operating system terminates the process on its own.
/// So a stream that is absent waits for ever here rather than completing:
/// completing would shut a healthy runner down the moment it started, on the
/// grounds that one handler out of two failed to install.
pub async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, Signal, SignalKind};

        async fn next(stream: &mut Option<Signal>) {
            match stream {
                Some(stream) => {
                    stream.recv().await;
                }
                None => std::future::pending().await,
            }
        }

        fn install(kind: SignalKind, name: &'static str) -> Option<Signal> {
            match signal(kind) {
                Ok(stream) => Some(stream),
                Err(error) => {
                    tracing::error!(
                        error = %error,
                        signal = %name,
                        "runner.signal_handler_not_installed"
                    );
                    None
                }
            }
        }

        let mut sigterm = install(SignalKind::terminate(), "SIGTERM");
        let mut sigint = install(SignalKind::interrupt(), "SIGINT");

        tokio::select! {
            () = next(&mut sigterm) => tracing::info!(reason = "SIGTERM", "runner.signal.received"),
            () = next(&mut sigint) => tracing::info!(reason = "SIGINT", "runner.signal.received"),
        }
    }

    #[cfg(windows)]
    {
        use tokio::signal::windows::{ctrl_break, ctrl_c, CtrlBreak, CtrlC};

        async fn next_ctrl_c(stream: &mut Option<CtrlC>) {
            match stream {
                Some(stream) => {
                    stream.recv().await;
                }
                None => std::future::pending().await,
            }
        }

        async fn next_ctrl_break(stream: &mut Option<CtrlBreak>) {
            match stream {
                Some(stream) => {
                    stream.recv().await;
                }
                None => std::future::pending().await,
            }
        }

        fn report(name: &'static str, error: &std::io::Error) {
            tracing::error!(
                error = %error,
                signal = %name,
                "runner.signal_handler_not_installed"
            );
        }

        let mut ctrl_c_stream = ctrl_c().inspect_err(|error| report("Ctrl-C", error)).ok();
        let mut ctrl_break_stream = ctrl_break()
            .inspect_err(|error| report("Ctrl-Break", error))
            .ok();

        tokio::select! {
            () = next_ctrl_c(&mut ctrl_c_stream) => {
                tracing::info!(reason = "Ctrl-C", "runner.signal.received")
            }
            () = next_ctrl_break(&mut ctrl_break_stream) => {
                tracing::info!(reason = "Ctrl-Break", "runner.signal.received")
            }
        }
    }
}
