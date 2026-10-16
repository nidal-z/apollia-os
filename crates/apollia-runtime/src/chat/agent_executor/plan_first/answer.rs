//! The answer of a plan-first run, written by the agent's model from what the
//! steps of its plan returned.
//!
//! Without it, the engine joined the outputs of the steps, and a plan whose
//! last step was a tool answered with that tool's raw output: a batch item
//! read a whole file of conditions back as its result. The model is given the
//! agent's mission, the request and each step's result, and writes the answer
//! the request asks for. Its tokens go out under the run's session, so the
//! interface shows the answer being written.

use std::collections::HashMap;
use std::sync::Arc;

use apollia_core::{AIPResult, RuntimeEvent};
use apollia_llm::types::{ChatMessage, CompletionRequest, StreamChunk};
use apollia_llm::CompletionModel;
use futures::StreamExt;

use crate::eventbus::EventBusSender;

/// The most the answer may generate, reasoning included.
const ANSWER_MAX_TOKENS: u32 = 8_192;
/// The longest result of one step handed to the model, in characters.
const STEP_RESULT_MAX_CHARS: usize = 12_000;

/// What the final answer is written from, kept by the agent of the run.
pub(super) struct FinalAnswer {
    pub model: Arc<dyn CompletionModel>,
    pub mission: String,
    pub request: String,
    pub event_bus: EventBusSender,
    pub session_id: String,
    pub message_id: String,
}

impl FinalAnswer {
    /// The answer to the request, from the results of the steps. A model that
    /// cannot be reached leaves the joined results, as before.
    pub(super) async fn write(&self, step_results: &HashMap<String, String>) -> AIPResult {
        let request = CompletionRequest {
            messages: vec![
                ChatMessage::system(self.mission.clone()),
                ChatMessage::user(prompt_of(&self.request, step_results)),
            ],
            max_tokens: Some(ANSWER_MAX_TOKENS),
            ..Default::default()
        };
        let written = match self.model.stream(request.clone()).await {
            Ok(mut stream) => {
                let mut text = String::new();
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(StreamChunk::Text(token)) => {
                            let _ = self.event_bus.send(RuntimeEvent::ChatToken {
                                session_id: self.session_id.clone(),
                                message_id: self.message_id.clone(),
                                token: token.clone(),
                            });
                            text.push_str(&token);
                        }
                        Ok(_) => {}
                        Err(e) => {
                            tracing::warn!(error = %e, "plan.answer.stream_interrupted");
                            break;
                        }
                    }
                }
                text
            }
            Err(e) => match self.model.complete(request).await {
                Ok(response) => response.content,
                Err(_) => {
                    tracing::warn!(error = %e, "plan.answer.unavailable");
                    String::new()
                }
            },
        };
        let answer = without_reasoning(&written);
        if answer.is_empty() {
            return joined(step_results);
        }
        AIPResult::completed(&answer)
    }
}

/// The turn that asks for the answer: the request, then each step's result.
fn prompt_of(request: &str, step_results: &HashMap<String, String>) -> String {
    let mut steps: Vec<(&String, &String)> = step_results.iter().collect();
    steps.sort_by(|a, b| step_rank(a.0).cmp(&step_rank(b.0)).then(a.0.cmp(b.0)));
    let mut text = format!("REQUEST:\n{request}\n\nRESULTS OF THE STEPS OF YOUR PLAN:\n");
    for (step, result) in steps {
        let shown: String = result.chars().take(STEP_RESULT_MAX_CHARS).collect();
        text.push_str(&format!("\n[{step}]\n{shown}\n"));
    }
    text.push_str(
        "\nWrite the final answer to the request from these results, in the language of \
         the request and in the form it asks for. Do not describe the steps; give the answer.",
    );
    text
}

/// `s12` ranks after `s2`.
fn step_rank(step: &str) -> u64 {
    step.trim_start_matches(|c: char| !c.is_ascii_digit())
        .parse()
        .unwrap_or(u64::MAX)
}

/// The written text without the reasoning a model writes between markers.
fn without_reasoning(text: &str) -> String {
    let normalised = apollia_llm::reasoning_markers::normalise_reasoning_markers(text);
    let text = normalised.as_ref();
    let after = match text.rfind("</think>") {
        Some(end) => &text[end + "</think>".len()..],
        None if text.contains("<think>") => "",
        None => text,
    };
    after.trim().to_string()
}

fn joined(step_results: &HashMap<String, String>) -> AIPResult {
    let mut steps: Vec<(&String, &String)> = step_results.iter().collect();
    steps.sort_by_key(|a| step_rank(a.0));
    let text = steps
        .iter()
        .map(|(_, v)| v.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    AIPResult::completed(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_answer_keeps_what_follows_the_reasoning() {
        // GIVEN texts with a closed reasoning, an unclosed one, and none
        // WHEN the reasoning is removed
        // THEN only the answer is kept
        assert_eq!(
            without_reasoning("<think>Compare</think>\nCompliant."),
            "Compliant."
        );
        assert_eq!(without_reasoning("<think>Still comparing"), "");
        assert_eq!(without_reasoning("Compliant."), "Compliant.");
    }

    #[test]
    fn test_the_prompt_gives_the_results_in_the_order_of_the_plan() {
        // GIVEN the results of steps s2, s10 and s1
        let results = HashMap::from([
            ("s2".to_string(), "two".to_string()),
            ("s10".to_string(), "ten".to_string()),
            ("s1".to_string(), "one".to_string()),
        ]);

        // WHEN the prompt is written
        let prompt = prompt_of("Audit this quote", &results);

        // THEN the request comes first and the steps follow in their order
        let one = prompt.find("[s1]").expect("s1");
        let two = prompt.find("[s2]").expect("s2");
        let ten = prompt.find("[s10]").expect("s10");
        assert!(prompt.starts_with("REQUEST:\nAudit this quote"));
        assert!(one < two && two < ten);
    }
}
