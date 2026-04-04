//! File path extraction from bash command output via the fast LLM backend.
//!
//! [`FilePathExtractor::extract_detached`] spawns a detached [`tokio::spawn`] task so
//! that `BashExecutor::run` is never blocked (Principe #5 — Un acteur, une responsabilité).
//! LLM failures are logged as `tracing::warn` and never propagated.

use std::path::PathBuf;
use std::sync::Arc;

use apollia_core::{EventBusSender, RuntimeEvent};
use apollia_llm::types::{ChatMessage, CompletionRequest, LlmError};
use apollia_llm::LlmRouter;
use regex::Regex;

/// Default path extraction pattern (POSIX.1-2017 §3.265 + Windows UNC RFC 8089).
///
/// Matches:
/// - Unix absolute paths  : `/path/to/file.rs`
/// - Unix relative paths  : `./path`, `../path`
/// - Relative with slash  : `src/main.rs`, `a/b/c.txt`
/// - Windows absolute     : `C:\path\to\file`
/// - Windows UNC          : `\\server\share`
const DEFAULT_PATH_PATTERN: &str = r#"(?:/[^\s\n\r<>"'|*?]+|\.{1,2}/[^\s\n\r<>"'|*?]+|(?:[a-zA-Z0-9_.-]+/)+[a-zA-Z0-9_.-]+(?:\.[a-zA-Z0-9]+)?|[a-zA-Z]:\\[^\s\n\r<>"'|*?]+|\\\\[^\s\n\r<>"'|*?]+)"#;

/// Extracts file paths from bash command output via the fast LLM backend.
///
/// Uses [`LlmRouter::route_fast`] (light extraction backend) when routing is configured,
/// falling back to the default backend otherwise. The extraction is always non-blocking:
/// [`extract_detached`](Self::extract_detached) spawns a detached task and returns
/// immediately to the caller.
pub struct FilePathExtractor {
    llm_router: Arc<LlmRouter>,
    path_pattern: Regex,
}

impl FilePathExtractor {
    /// Creates a `FilePathExtractor` with the built-in default path pattern.
    ///
    /// The default pattern covers Unix absolute/relative paths (POSIX.1-2017 §3.265)
    /// and Windows UNC paths (RFC 8089).
    pub fn new(llm_router: Arc<LlmRouter>) -> Self {
        // DEFAULT_PATH_PATTERN is a compile-time constant — an error here is a bug.
        let path_pattern =
            Regex::new(DEFAULT_PATH_PATTERN).expect("DEFAULT_PATH_PATTERN is a valid regex");
        Self {
            llm_router,
            path_pattern,
        }
    }

    /// Creates a `FilePathExtractor` with a custom path extraction pattern.
    ///
    /// The custom pattern replaces the built-in default for this instance.
    /// Matches are applied to the LLM response to collect [`PathBuf`] values.
    ///
    /// Returns [`regex::Error`] if `pattern` is not a valid regular expression.
    pub fn with_pattern(llm_router: Arc<LlmRouter>, pattern: &str) -> Result<Self, regex::Error> {
        let path_pattern = Regex::new(pattern)?;
        Ok(Self {
            llm_router,
            path_pattern,
        })
    }

    /// Spawns a detached task that extracts paths from `output` and emits
    /// [`RuntimeEvent::BashFilePathsExtracted`] when paths are found.
    ///
    /// Returns immediately — the caller is never blocked. Failures are logged
    /// at `WARN` level and silently discarded (best-effort extraction).
    pub fn extract_detached(&self, command: String, output: String, event_tx: EventBusSender) {
        let router = Arc::clone(&self.llm_router);
        let pattern = self.path_pattern.clone();
        tokio::spawn(async move {
            match Self::extract_paths(&router, &pattern, &command, &output).await {
                Ok(paths) if !paths.is_empty() => {
                    // Fire-and-forget: a missing receiver (no subscribers) is not an error.
                    let _ = event_tx.send(RuntimeEvent::BashFilePathsExtracted { paths });
                }
                Ok(_) => {
                    // No paths found — nothing to emit.
                }
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        command = %command,
                        "file_path_extractor: LLM extraction failed"
                    );
                }
            }
        });
    }

    /// Calls the LLM backend to extract file paths from `output`.
    ///
    /// Prefers the fast backend via [`LlmRouter::route_fast`]; falls back to the
    /// default backend when routing is not configured (e.g. in test environments).
    async fn extract_paths(
        router: &LlmRouter,
        pattern: &Regex,
        command: &str,
        output: &str,
    ) -> Result<Vec<PathBuf>, LlmError> {
        // route_fast selects the cheap extraction backend (e.g. Haiku, Mistral-small).
        // Fall back to route(None) (default) when routing is not configured.
        let backend = match router.route_fast() {
            Ok(b) => b,
            Err(_) => router.route(None),
        };

        let req = CompletionRequest {
            messages: vec![
                ChatMessage::system(
                    "You are a file-path extraction assistant. \
                     Given the output of a bash command, list all file paths you find, \
                     one per line. If there are no file paths, respond with an empty string. \
                     Output only the paths — no explanations, no bullet points.",
                ),
                ChatMessage::user(format!("Command: {command}\nOutput:\n{output}")),
            ],
            max_tokens: Some(512),
            temperature: Some(0.0),
            ..Default::default()
        };

        let response = backend.complete(req).await?;
        Ok(parse_paths(pattern, &response.content))
    }
}

/// Applies `pattern` to `text` and converts each match to a [`PathBuf`].
fn parse_paths(pattern: &Regex, text: &str) -> Vec<PathBuf> {
    pattern
        .find_iter(text)
        .map(|m| PathBuf::from(m.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::pin::Pin;
    use std::sync::Arc;

    use apollia_llm::types::{
        CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError,
        StreamChunk, TokenUsage,
    };
    use apollia_llm::LlmRouter;
    use async_trait::async_trait;
    use futures::Stream;
    use regex::Regex;

    use super::*;

    // ── Mock LLM backend ──────────────────────────────────────────────────────

    /// Test double that always returns a fixed string as the LLM response.
    struct FixedResponseBackend {
        response: String,
    }

    impl FixedResponseBackend {
        fn returning(response: impl Into<String>) -> Arc<dyn CompletionModel> {
            Arc::new(Self {
                response: response.into(),
            })
        }
    }

    #[async_trait]
    impl CompletionModel for FixedResponseBackend {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                content: self.response.clone(),
                tool_calls: vec![],
                usage: TokenUsage::default(),
                finish_reason: FinishReason::Stop,
                latency_ms: 1,
                ttft_ms: None,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            unimplemented!("streaming not used by FilePathExtractor")
        }

        fn is_available(&self) -> bool {
            true
        }

        fn backend_name(&self) -> &str {
            "fixed-response"
        }

        fn model_id(&self) -> &str {
            "fixed-response-v1"
        }
    }

    fn make_router(response: &str) -> Arc<LlmRouter> {
        let mut backends = HashMap::new();
        backends.insert("fast".to_owned(), FixedResponseBackend::returning(response));
        Arc::new(LlmRouter::with_backends(backends, "fast"))
    }

    // ── Unit tests ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn file_path_extractor_parses_find_output() {
        // GIVEN — output de find = "src/main.rs\nsrc/lib.rs"
        let router = make_router("src/main.rs\nsrc/lib.rs");
        let extractor = FilePathExtractor::new(router);

        // WHEN
        let paths = FilePathExtractor::extract_paths(
            &extractor.llm_router,
            &extractor.path_pattern,
            "find . -name '*.rs'",
            "src/main.rs\nsrc/lib.rs",
        )
        .await
        .expect("extraction must succeed");

        // THEN
        assert_eq!(
            paths,
            vec![PathBuf::from("src/main.rs"), PathBuf::from("src/lib.rs")]
        );
    }

    #[tokio::test]
    async fn file_path_extractor_returns_empty_for_no_paths() {
        // GIVEN — output sans paths, LLM retourne une chaîne vide
        let router = make_router("");
        let extractor = FilePathExtractor::new(router);

        // WHEN
        let paths = FilePathExtractor::extract_paths(
            &extractor.llm_router,
            &extractor.path_pattern,
            "echo done",
            "Process exited with code 0\n2 tasks done",
        )
        .await
        .expect("extraction must succeed");

        // THEN
        assert!(paths.is_empty(), "expected no paths, got: {paths:?}");
    }

    #[test]
    fn parse_paths_extracts_unix_absolute_paths() {
        // GIVEN
        let pattern = Regex::new(DEFAULT_PATH_PATTERN).unwrap();

        // WHEN
        let paths = parse_paths(&pattern, "/home/user/file.txt /etc/config.toml");

        // THEN
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/home/user/file.txt"),
                PathBuf::from("/etc/config.toml"),
            ]
        );
    }

    #[test]
    fn parse_paths_extracts_relative_paths_with_slash() {
        // GIVEN
        let pattern = Regex::new(DEFAULT_PATH_PATTERN).unwrap();

        // WHEN
        let paths = parse_paths(&pattern, "src/main.rs\n./lib/utils.rs");

        // THEN
        assert!(
            paths.contains(&PathBuf::from("src/main.rs")),
            "src/main.rs must be extracted"
        );
        assert!(
            paths.contains(&PathBuf::from("./lib/utils.rs")),
            "./lib/utils.rs must be extracted"
        );
    }

    #[test]
    fn parse_paths_returns_empty_for_plain_text() {
        // GIVEN — text with no path-like tokens
        let pattern = Regex::new(DEFAULT_PATH_PATTERN).unwrap();

        // WHEN
        let paths = parse_paths(&pattern, "Process exited with code 0\n2 tasks done");

        // THEN
        assert!(paths.is_empty(), "expected no paths, got: {paths:?}");
    }

    #[test]
    fn with_pattern_rejects_invalid_regex() {
        // GIVEN — invalid regex pattern
        let router = make_router("");

        // WHEN / THEN
        assert!(
            FilePathExtractor::with_pattern(router, "[invalid").is_err(),
            "invalid regex must produce an error"
        );
    }

    #[tokio::test]
    async fn extract_detached_emits_event_when_paths_found() {
        // GIVEN
        use tokio::sync::broadcast;

        let router = make_router("src/main.rs");
        let extractor = FilePathExtractor::new(router);
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);

        // WHEN
        extractor.extract_detached("find . -name '*.rs'".into(), "src/main.rs".into(), tx);

        // THEN — event must arrive
        let event = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                match rx.recv().await {
                    Ok(RuntimeEvent::BashFilePathsExtracted { paths }) => break paths,
                    Ok(_) => continue,
                    Err(_) => panic!("channel closed without BashFilePathsExtracted"),
                }
            }
        })
        .await
        .expect("BashFilePathsExtracted must be emitted within 2 seconds");

        assert!(!event.is_empty());
        assert!(event.contains(&PathBuf::from("src/main.rs")));
    }

    #[tokio::test]
    async fn extract_detached_emits_no_event_when_llm_returns_empty() {
        // GIVEN — LLM returns empty → no event emitted
        use tokio::sync::broadcast;

        let router = make_router("");
        let extractor = FilePathExtractor::new(router);
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);

        // WHEN
        extractor.extract_detached("echo done".into(), "all done".into(), tx);

        // THEN — no BashFilePathsExtracted within a short window
        let result = tokio::time::timeout(std::time::Duration::from_millis(300), async {
            loop {
                match rx.recv().await {
                    Ok(RuntimeEvent::BashFilePathsExtracted { .. }) => return true,
                    Ok(_) => continue,
                    Err(_) => return false,
                }
            }
        })
        .await;

        assert!(
            result.is_err() || !result.unwrap(),
            "BashFilePathsExtracted must NOT be emitted when LLM returns no paths"
        );
    }
}
