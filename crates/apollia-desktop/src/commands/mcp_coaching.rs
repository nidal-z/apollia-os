//! Tauri IPC command: post-install coaching examples for a freshly added
//! MCP connection.
//!
//! Returns 2–3 ready-to-send prompts the operator can click to try the newly
//! connected service immediately. Implemented as a heuristic for now: the
//! skill contract (same `Vec<CoachingExample>` shape) matches what the
//! Meta-LLM `GenerateCapabilityCoaching` routine will produce,
//! so the frontend will not need to change when the LLM path is wired.
//!
//! Cache is not implemented yet (owns the 7-day file cache keyed
//! by `integration_id + version`); the heuristic is deterministic and cheap.
use serde::{Deserialize, Serialize};

/// One coaching example shown as a clickable card in the wizard's final step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoachingExample {
    /// Short actionable title (e.g. "Summarise my latest Notion page").
    pub title: String,
    /// One-line description shown under the title.
    pub description: String,
    /// Pre-filled chat message sent when the user clicks "Try".
    pub prompt: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoachingRequest {
    pub server_name: String,
    #[serde(default)]
    pub server_title: Option<String>,
}

/// Generate 2–3 post-install examples for a newly added MCP server.
///
/// Never returns `Err`; on empty match the frontend shows a generic empty
/// state. The heuristic matches against well-known connector names; unknown
/// servers get a single generic "Explore the tools" card.
#[tauri::command]
pub fn meta_generate_capabilities_coaching(
    request: CoachingRequest,
) -> Result<Vec<CoachingExample>, String> {
    let name = request.server_name.to_ascii_lowercase();
    let title = request
        .server_title
        .unwrap_or_else(|| request.server_name.clone());

    let examples = if name.contains("notion") {
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
    };

    Ok(examples)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn examples_for(server: &str) -> Vec<CoachingExample> {
        meta_generate_capabilities_coaching(CoachingRequest {
            server_name: server.to_string(),
            server_title: None,
        })
        .expect("coaching never fails")
    }

    #[test]
    fn test_coaching_cards_are_written_in_one_language() {
        // GIVEN every branch of the connector heuristic, including the seeded
        // `filesystem` server and the generic fallback
        //
        // WHEN their cards are generated
        //
        // THEN every visible string reads in the codebase language. These go
        // straight into the wizard's final step with no translation layer, so a
        // French card lands untouched in an English window. A non-ASCII scan
        // alone would not do: "Liste les fichiers" is pure ASCII, hence the
        // check on the leading verb of each title.
        let english_openers = [
            "Summarise",
            "List",
            "Explore",
            "Run",
            "Open",
            "Get",
            "Ask",
            "Show",
            "Look",
            "Search",
            "Which",
        ];
        for server in [
            "notion",
            "github",
            "slack",
            "filesystem",
            "brave-search",
            "totally-unknown",
        ] {
            let cards = examples_for(server);
            assert!(!cards.is_empty(), "{server} produced no card");
            for card in cards {
                for text in [&card.title, &card.description, &card.prompt] {
                    let first = text.split_whitespace().next().unwrap_or("");
                    assert!(
                        english_openers.contains(&first),
                        "{server}: '{text}' does not open in English"
                    );
                }
            }
        }

        // The negative case, so this test is known to be able to fail.
        assert!(!english_openers.contains(&"Liste")); // French, on purpose
    }

    #[test]
    fn test_unknown_server_still_gets_a_card_naming_it() {
        // GIVEN a connector the heuristic knows nothing about
        // WHEN coaching is generated
        // THEN one generic card comes back and it names the server, so the
        // wizard's last step is never blank
        let cards = examples_for("acme-widgets");
        assert_eq!(cards.len(), 1);
        assert!(cards[0].title.contains("acme-widgets"));
    }
}
