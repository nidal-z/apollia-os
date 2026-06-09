//! The LLM-judge assertion: scores a run output PASS/FAIL against a rubric.
//!
//! Some tasks succeed in ways no exit code, file, or regex can verify. The judge
//! asks the user-configured [`LlmRouter`] (local or cloud, per their config) to
//! rate the output against a rubric, using the fast route to keep evaluation
//! cheap (Principle #1: it is the operator's own backend, no imposed third
//! party). Backend absence is non-blocking: the assertion is skipped, not failed.

use apollia_llm::{ChatMessage, CompletionRequest, LlmRouter};

/// The verdict of an LLM judge on a single run output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JudgeVerdict {
    /// The output satisfies the rubric.
    Pass,
    /// The output fails the rubric, carrying the judge's reason.
    Fail(String),
    /// No backend was available (or the call failed); non-blocking.
    Skipped,
}

/// Judges a run `output` against a `rubric` using the fast route.
///
/// Resolves a backend via the fast route, falling back to the default. When no
/// backend is available, or the completion call fails, returns
/// [`JudgeVerdict::Skipped`] (non-blocking) and emits a `tracing` warning. The
/// prompt is deterministic (temperature 0) and asks for a `PASS` or
/// `FAIL: <reason>` answer.
pub async fn judge(router: &LlmRouter, rubric: &str, output: &str) -> JudgeVerdict {
    let backend = match router.route_fast() {
        Ok(backend) => backend,
        Err(_) => match router.get(None) {
            Some(backend) => backend,
            None => {
                tracing::warn!("eval.judge.no_backend");
                return JudgeVerdict::Skipped;
            }
        },
    };

    let request = CompletionRequest {
        messages: vec![
            ChatMessage::system(format!(
                "You are an evaluation judge. Decide whether the output satisfies the rubric. \
                 Reply with a single line: `PASS` when it satisfies the rubric, or \
                 `FAIL: <reason>` when it does not.\n\nRubric:\n{rubric}"
            )),
            ChatMessage::user(format!("Output to judge:\n{output}")),
        ],
        temperature: Some(0.0),
        max_tokens: Some(256),
        ..Default::default()
    };

    match backend.complete(request).await {
        Ok(response) => parse_verdict(&response.content),
        Err(error) => {
            tracing::warn!(error = %error, "eval.judge.backend_error");
            JudgeVerdict::Skipped
        }
    }
}

/// Parses a judge response into a [`JudgeVerdict`].
///
/// A response starting with `PASS` is a pass; one starting with `FAIL` is a
/// failure carrying the trailing reason; anything else is an inconclusive
/// failure.
fn parse_verdict(content: &str) -> JudgeVerdict {
    let trimmed = content.trim();
    if trimmed.to_uppercase().starts_with("PASS") {
        JudgeVerdict::Pass
    } else if trimmed.to_uppercase().starts_with("FAIL") {
        let reason = trimmed
            .split_once(':')
            .map(|(_, rest)| rest.trim())
            .filter(|rest| !rest.is_empty())
            .unwrap_or("judge returned FAIL without a reason")
            .to_string();
        JudgeVerdict::Fail(reason)
    } else {
        JudgeVerdict::Fail("judge response was inconclusive".to_string())
    }
}

#[cfg(test)]
mod tests {
    use apollia_llm::LlmRouter;

    use super::*;
    use crate::test_support::router_replying;

    #[tokio::test]
    async fn test_judge_pass_when_model_says_pass() {
        // GIVEN a router whose backend replies "PASS"
        let router = router_replying("PASS");

        // WHEN judging an output against a rubric
        let verdict = judge(&router, "must greet the user", "Hello there!").await;

        // THEN the verdict is Pass
        assert_eq!(verdict, JudgeVerdict::Pass);
    }

    // (failure case)
    #[tokio::test]
    async fn test_judge_fail_carries_the_reason() {
        // GIVEN a router whose backend replies "FAIL: <reason>"
        let router = router_replying("FAIL: the answer ignores the rubric");

        // WHEN judging
        let verdict = judge(&router, "must cite a source", "no source here").await;

        // THEN the verdict is Fail carrying the judge reason
        assert_eq!(
            verdict,
            JudgeVerdict::Fail("the answer ignores the rubric".to_string())
        );
    }

    // (no backend -> skipped, non-blocking)
    #[tokio::test]
    async fn test_judge_skipped_when_no_backend() {
        // GIVEN an empty router with no backend
        let router = LlmRouter::empty();

        // WHEN judging
        let verdict = judge(&router, "any rubric", "any output").await;

        // THEN the verdict is Skipped, not a failure
        assert_eq!(verdict, JudgeVerdict::Skipped);
    }

    #[test]
    fn test_parse_verdict_inconclusive_is_failure() {
        // GIVEN a response that is neither PASS nor FAIL
        // WHEN parsing it
        // THEN it is an inconclusive failure, not a pass
        assert!(matches!(
            parse_verdict("maybe, hard to say"),
            JudgeVerdict::Fail(_)
        ));
    }
}
