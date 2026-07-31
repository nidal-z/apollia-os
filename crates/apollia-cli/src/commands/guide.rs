//! `apollia-os guide <topic>`: short, task-oriented help by theme.
//!
//! Complements clap's reference `--help` with a "how do I..." view. Content is
//! hand-written and tracks the v2 taxonomy.

use crate::commands::suggest;
use crate::exit_codes;

/// Guide topics, in listing order, with their body.
const TOPICS: &[(&str, &str)] = &[
    (
        "chat",
        "Chat with an agent, streamed live in the terminal.\n\n\
         apollia-os chat                 start the streaming REPL\n\
         apollia-os chat --resume <id>   resume a session\n\
         apollia-os chat --list          list recent sessions\n\
         apollia-os chat config show     inspect the Chat Libre config\n\n\
         In the REPL: /fork, /list-commands, and any .md command in \
         ~/.apollia/commands/. Tool-approval prompts appear inline.",
    ),
    (
        "governance",
        "Control what agents may do.\n\n\
         apollia-os permissions list             show permission rules\n\
         apollia-os tools list                   native tool governance\n\
         apollia-os tools show <tool>            inspect one tool\n\
         apollia-os resilience list              circuit breaker state\n\
         apollia-os task list --pending-approval pending HITL approvals",
    ),
    (
        "audit",
        "Inspect and verify what happened.\n\n\
         apollia-os audit list           recent audited runs\n\
         apollia-os audit verify <run>   check the signed hash chain\n\
         apollia-os trace <task_id>      event-sourced trace of a task\n\
         apollia-os digest --since 7d    activity + cost overview",
    ),
    (
        "agents",
        "Install, run, and manage agents.\n\n\
         apollia-os agent list           installed agents\n\
         apollia-os agent create <name>  scaffold from a template\n\
         apollia-os agent install <path> install an agent or bundle\n\
         apollia-os agent show <id>      details for one agent\n\
         apollia-os run <agent> <input>  submit a task (add --stream)",
    ),
    (
        "models",
        "Manage local models and LLM backends.\n\n\
         apollia-os model list           local model files\n\
         apollia-os llm status           backend health\n\
         apollia-os llm backends         configured backends\n\
         apollia-os plan cache stats     plan cache statistics",
    ),
    (
        "integrations",
        "Connect external services and MCP servers.\n\n\
         (desktop app)                      connect a SaaS account\n\
         apollia-os connector list          native connectors\n\
         apollia-os mcp list                MCP servers\n\
         apollia-os mcp add <name> <url>    register an MCP server\n\
         apollia-os mcp server              expose Apollia as an MCP server",
    ),
];

/// Run `guide`. With no topic, list the topics; with a topic, print its body.
pub fn run(topic: Option<&str>, json: bool) -> i32 {
    match topic {
        None => list_topics(json),
        Some(t) => match TOPICS.iter().find(|(k, _)| *k == t) {
            Some((_, body)) => {
                println!("{body}");
                exit_codes::SUCCESS
            }
            None => {
                let names: Vec<&str> = TOPICS.iter().map(|(k, _)| *k).collect();
                eprint!("unknown guide topic: {t}");
                if let Some(sugg) = suggest::nearest(t, &names, 3) {
                    eprint!(" (did you mean `{sugg}`?)");
                }
                eprintln!();
                eprintln!("topics: {}", names.join(", "));
                exit_codes::GENERAL_ERROR
            }
        },
    }
}

/// List the available topics (JSON array in machine mode).
fn list_topics(json: bool) -> i32 {
    let names: Vec<&str> = TOPICS.iter().map(|(k, _)| *k).collect();
    if json {
        let arr = serde_json::to_string(&names).unwrap_or_else(|_| "[]".into());
        println!("{arr}");
    } else {
        println!("Guide topics (apollia-os guide <topic>):");
        for (name, _) in TOPICS {
            println!("  {name}");
        }
    }
    exit_codes::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_topics_present() {
        for t in [
            "chat",
            "governance",
            "audit",
            "agents",
            "models",
            "integrations",
        ] {
            assert!(TOPICS.iter().any(|(k, _)| *k == t), "missing topic {t}");
        }
    }

    // GIVEN un sujet inconnu proche WHEN on suggere THEN un sujet valide.
    #[test]
    fn test_unknown_topic_has_suggestion() {
        let names: Vec<&str> = TOPICS.iter().map(|(k, _)| *k).collect();
        assert_eq!(suggest::nearest("agent", &names, 3), Some("agents"));
    }
}
