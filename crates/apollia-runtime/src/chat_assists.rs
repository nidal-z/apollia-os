//! The assists around the chat: the rewrite of a terse prompt, the next steps
//! cards, the parse of an automation described in words, and the examples
//! offered once a connector is added.
//!
//! The desktop running the runtime in its own process calls these functions
//! directly; the daemon serves them under `/api/v1/assists` so an attached
//! window gets the same answers. None of them fails: each degrades to the
//! original text, a heuristic, or a generic card, and says so.

use std::sync::Arc;

use apollia_llm::meta::next_steps::{
    generate_next_steps, heuristic_fallback, NextStep, NextStepsRequest,
};
use apollia_llm::meta::parse_automation::{parse_automation, ParsedAutomation};
use apollia_llm::meta::rewrite_input::{rewrite_input, RewriteFallback, RewriteInputRequest};
use apollia_llm::LlmRouter;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Why no rewrite happened, as the composer receives it.
///
/// Mirrors [`RewriteFallback`] and adds the one situation this layer owns:
/// no router is mounted at all, which the crate below cannot observe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum RewriteFallbackReason {
    /// No LLM router is mounted.
    NoRouter,
    /// A router is mounted and holds no backend under its default name.
    NoBackend,
    /// The backend answered with an error, or the call timed out.
    CallFailed,
    /// The backend answered, and nothing survived the reasoning strip.
    EmptyAnswer,
}

impl From<RewriteFallback> for RewriteFallbackReason {
    fn from(value: RewriteFallback) -> Self {
        match value {
            RewriteFallback::NoBackend => Self::NoBackend,
            RewriteFallback::CallFailed => Self::CallFailed,
            RewriteFallback::EmptyAnswer => Self::EmptyAnswer,
        }
    }
}

/// The rewritten prompt, or the original one with the reason it came back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RewriteReply {
    /// The text to put back in the composer.
    pub rewritten_text: String,
    /// `None` when the model rewrote the text, the cause otherwise.
    pub fallback: Option<RewriteFallbackReason>,
}

/// Rewrites a prompt with the default model of `router`. Without a router the
/// original text comes back with [`RewriteFallbackReason::NoRouter`].
pub async fn rewrite(
    router: Option<&Arc<LlmRouter>>,
    request: RewriteInputRequest,
) -> RewriteReply {
    let Some(router) = router else {
        return RewriteReply {
            rewritten_text: request.original_text,
            fallback: Some(RewriteFallbackReason::NoRouter),
        };
    };
    let response = rewrite_input(router, request).await;
    RewriteReply {
        rewritten_text: response.rewritten_text,
        fallback: response.fallback.map(RewriteFallbackReason::from),
    }
}

/// Up to three next steps cards, and whether a model wrote them.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NextStepsReply {
    /// The cards, in display order.
    #[schema(value_type = Vec<Object>)]
    pub steps: Vec<NextStep>,
    /// `false` when the heuristic wrote them.
    pub from_llm: bool,
}

/// Generates the next steps cards with the default model of `router`, or by
/// the heuristic when there is no router or the model fails.
pub async fn next_steps(
    router: Option<&Arc<LlmRouter>>,
    request: NextStepsRequest,
) -> NextStepsReply {
    match router {
        Some(router) => {
            let response = generate_next_steps(router, request).await;
            NextStepsReply {
                steps: response.steps,
                from_llm: response.from_llm,
            }
        }
        None => NextStepsReply {
            steps: heuristic_fallback(&request),
            from_llm: false,
        },
    }
}

/// An automation described in words, and the agents the window knows.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ParseAutomationRequest {
    /// Free-form text entered by the operator.
    pub input: String,
    /// Identifiers or names of the agents known on the window side.
    #[serde(default)]
    pub known_agents: Vec<String>,
}

/// Parses a description (French or English) into an automation, relative to
/// `now`. An empty input gives a low confidence and one ambiguity.
pub fn parse(request: &ParseAutomationRequest, now: DateTime<Utc>) -> ParsedAutomation {
    parse_automation(&request.input, now, &request.known_agents)
}

/// One coaching example shown as a clickable card in the wizard's last step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CoachingExample {
    /// Short actionable title (e.g. "Summarise my latest Notion page").
    pub title: String,
    /// One-line description shown under the title.
    pub description: String,
    /// Pre-filled chat message sent when the user clicks "Try".
    pub prompt: String,
}

/// The connector just added.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CoachingRequest {
    /// The server name, matched against well-known connectors.
    pub server_name: String,
    /// The title shown to the operator, the name when absent.
    #[serde(default)]
    pub server_title: Option<String>,
}

/// Two or three examples to try a newly added MCP server at once.
///
/// A heuristic on well-known connector names; an unknown server gets one
/// generic card that names it, so the wizard's last step is never blank.
pub fn coaching(request: CoachingRequest) -> Vec<CoachingExample> {
    let name = request.server_name.to_ascii_lowercase();
    let title = request
        .server_title
        .unwrap_or_else(|| request.server_name.clone());

    if name.contains("notion") {
        vec![
            CoachingExample {
                title: format!("Summarise my latest {title} page"),
                description: "Open a chat and ask for a summary of your latest Notion page.".into(),
                prompt: "Summarise my latest Notion page in five bullet points.".into(),
            },
            CoachingExample {
                title: "List my databases".into(),
                description: "Explore the databases this connector can reach.".into(),
                prompt: "List the Notion databases you have access to.".into(),
            },
        ]
    } else if name.contains("github") {
        vec![
            CoachingExample {
                title: "List my open issues".into(),
                description: "Show the issues assigned to you.".into(),
                prompt: "List my ten most recent open GitHub issues.".into(),
            },
            CoachingExample {
                title: "Summarise a repository".into(),
                description: "Ask for a summary of a repository README.".into(),
                prompt: "Open my most recent repository and summarise its README.".into(),
            },
        ]
    } else if name.contains("slack") {
        vec![CoachingExample {
            title: "Summarise a channel".into(),
            description: "Get a summary of the latest messages in a channel.".into(),
            prompt: "Summarise the last thirty messages in my main Slack channel.".into(),
        }]
    } else if name.contains("filesystem") || name.contains("file") {
        vec![CoachingExample {
            title: "Explore a folder".into(),
            description: "List the files in an allowed folder.".into(),
            prompt: "List the files at the root of the workspace.".into(),
        }]
    } else if name.contains("brave") || name.contains("search") {
        vec![CoachingExample {
            title: "Run a web search".into(),
            description: "Look something up on the web.".into(),
            prompt: "Search the web for the latest Apollia OS news.".into(),
        }]
    } else {
        vec![CoachingExample {
            title: format!("Explore what {title} offers"),
            description: "Ask an assistant what this connector can do.".into(),
            prompt: format!(
                "Which tools does the {title} connection expose, and what can you do with them?"
            ),
        }]
    }
}

#[cfg(test)]
mod tests;
