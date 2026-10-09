//! The request sampling settings an agent passes to `ctx.llm.complete`.
//!
//! Read off the Python keywords once, then turned into a [`CompletionRequest`]
//! by a plain function, so what the bridge builds can be asserted without an
//! interpreter.

use apollia_llm::request_defaults::RequestDefaults;
use apollia_llm::{ChatMessage, CompletionRequest};

/// Cap applied when the agent sets no `max_tokens`: an unbounded completion
/// lets a local model run to context-fill and appear to hang forever. An agent
/// that needs a longer answer passes `max_tokens` explicitly.
pub(super) const DEFAULT_COMPLETE_MAX_TOKENS: u32 = 4096;

/// Per-request sampling settings, each one `None` when the agent left it to
/// the engine.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct RequestSampling {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub seed: Option<u64>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub min_p: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub repeat_penalty: Option<f32>,
    pub stop: Option<Vec<String>>,
    /// Whether the model reasons before it answers; `None` leaves the choice
    /// to the chat template of the model.
    pub thinking: Option<bool>,
}

/// Build the request `ctx.llm.complete` sends, from its messages, its sampling
/// settings and its optional response schema. Each setting the call leaves
/// unset takes the agent's resolved `defaults`; the token cap applies last.
pub(super) fn complete_request(
    messages: Vec<ChatMessage>,
    sampling: RequestSampling,
    response_schema: Option<serde_json::Value>,
    defaults: &RequestDefaults,
) -> CompletionRequest {
    let mut req = CompletionRequest {
        messages,
        temperature: sampling.temperature,
        max_tokens: sampling.max_tokens,
        seed: sampling.seed,
        response_schema,
        top_p: sampling.top_p,
        top_k: sampling.top_k,
        min_p: sampling.min_p,
        presence_penalty: sampling.presence_penalty,
        frequency_penalty: sampling.frequency_penalty,
        repeat_penalty: sampling.repeat_penalty,
        stop: sampling.stop,
        thinking: sampling.thinking,
        ..Default::default()
    };
    defaults.apply_to(&mut req);
    req.max_tokens = req.max_tokens.or(Some(DEFAULT_COMPLETE_MAX_TOKENS));
    req
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sampling_keyword_lands_on_the_built_request() {
        // GIVEN the keywords of `ctx.llm.complete(..., top_p=0.5, top_k=40,
        // min_p=0.25, presence_penalty=0.75, frequency_penalty=-0.5,
        // repeat_penalty=1.125, stop=["END"], seed=3, temperature=0.2,
        // max_tokens=64)`
        let sampling = RequestSampling {
            temperature: Some(0.2),
            max_tokens: Some(64),
            seed: Some(3),
            top_p: Some(0.5),
            top_k: Some(40),
            min_p: Some(0.25),
            presence_penalty: Some(0.75),
            frequency_penalty: Some(-0.5),
            repeat_penalty: Some(1.125),
            stop: Some(vec!["END".to_string()]),
            thinking: Some(false),
        };

        // WHEN the bridge builds the request it hands to the router
        let req = complete_request(
            vec![ChatMessage::user("hi")],
            sampling,
            None,
            &RequestDefaults::default(),
        );

        // THEN each setting is carried as given
        assert_eq!(req.temperature, Some(0.2));
        assert_eq!(req.max_tokens, Some(64));
        assert_eq!(req.seed, Some(3));
        assert_eq!(req.top_p, Some(0.5));
        assert_eq!(req.top_k, Some(40));
        assert_eq!(req.min_p, Some(0.25));
        assert_eq!(req.presence_penalty, Some(0.75));
        assert_eq!(req.frequency_penalty, Some(-0.5));
        assert_eq!(req.repeat_penalty, Some(1.125));
        assert_eq!(req.stop, Some(vec!["END".to_string()]));
        assert_eq!(req.thinking, Some(false));
        assert!(req.response_schema.is_none());
    }

    #[test]
    fn a_call_without_sampling_keywords_leaves_them_to_the_engine() {
        // GIVEN a bare `ctx.llm.complete(messages)`
        let sampling = RequestSampling::default();

        // WHEN the bridge builds the request
        let req = complete_request(
            vec![ChatMessage::user("hi")],
            sampling,
            None,
            &RequestDefaults::default(),
        );

        // THEN every sampling field stays unset, and only the token cap
        // takes its bridge default
        assert_eq!(req.top_p, None);
        assert_eq!(req.top_k, None);
        assert_eq!(req.min_p, None);
        assert_eq!(req.presence_penalty, None);
        assert_eq!(req.frequency_penalty, None);
        assert_eq!(req.repeat_penalty, None);
        assert_eq!(req.stop, None);
        assert_eq!(req.thinking, None);
        assert_eq!(req.max_tokens, Some(DEFAULT_COMPLETE_MAX_TOKENS));
    }

    #[test]
    fn the_agents_resolved_settings_fill_what_the_call_leaves_unset() {
        // GIVEN an agent whose three levels resolve to a temperature, a top-p
        //       and a token cap, and a call that sets its own temperature
        let defaults = RequestDefaults {
            temperature: Some(0.7),
            top_p: Some(0.8),
            max_tokens: Some(512),
            ..RequestDefaults::default()
        };
        let sampling = RequestSampling {
            temperature: Some(0.1),
            ..RequestSampling::default()
        };

        // WHEN the bridge builds the request
        let req = complete_request(vec![ChatMessage::user("hi")], sampling, None, &defaults);

        // THEN the call's temperature stays, the agent's top-p and token cap
        //      fill the rest, and the bridge cap does not override the agent's
        assert_eq!(req.temperature, Some(0.1));
        assert_eq!(req.top_p, Some(0.8));
        assert_eq!(req.max_tokens, Some(512));
    }
}
