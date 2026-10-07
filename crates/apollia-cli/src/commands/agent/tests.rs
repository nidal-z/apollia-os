use super::*;
use std::path::Path;

/// A stored agent named `name`, enabled.
fn test_stored_agent(name: &str) -> StoredAgent {
    StoredAgent {
        name: name.to_string(),
        origin: apollia_tools::AgentOrigin::Package,
        package: None,
        version: "0.1.0".to_string(),
        definition: serde_json::json!({
            "id": name, "name": name, "mission": "", "defaults": {},
        }),
        enabled: true,
        installed_at: "2026-03-17T10:00:00Z".to_string(),
        updated_at: "2026-03-17T10:00:00Z".to_string(),
    }
}

// uninstall command output format (JSON)
#[test]
fn test_uninstall_command_output() {
    // GIVEN an agent name
    let name = "mon-agent";
    // WHEN formatting JSON output
    let output = serde_json::json!({
        "name": name,
        "status": "uninstalled",
    });
    // THEN JSON contains expected fields
    assert_eq!(output["name"], "mon-agent");
    assert_eq!(output["status"], "uninstalled");
}

// enable/disable output
#[test]
fn test_enable_disable_output() {
    // GIVEN an agent name
    let name = "mon-agent";
    // WHEN formatting enable/disable JSON output
    let enable_output = serde_json::json!({ "name": name, "enabled": true });
    let disable_output = serde_json::json!({ "name": name, "enabled": false });
    // THEN JSON contains expected values
    assert_eq!(enable_output["enabled"], true);
    assert_eq!(disable_output["enabled"], false);
}

// list shows installed agents merged with runtime
#[test]
fn test_list_shows_installed_agents() {
    // GIVEN 2 installed agents (1 enabled, 1 disabled) and runtime data
    let installed = vec![test_stored_agent("agent-active"), {
        let mut a = test_stored_agent("agent-disabled");
        a.enabled = false;
        a
    }];
    let runtime = Some(serde_json::json!({
        "agents": [
            {
                "agent_id": "uuid-1",
                "name": "agent-active",
                "state": "Active",
                "manifest": { "name": "agent-active", "version": "0.1.0" },
            },
            {
                "agent_id": "uuid-3",
                "name": "runtime-only",
                "state": "Active",
                "manifest": { "name": "runtime-only", "version": "1.0.0" },
            }
        ]
    }));

    // WHEN building JSON list
    let result = build_list_json(&installed, &runtime);
    let agents = result["agents"].as_array().expect("should be array");

    // THEN all agents are present with the runtime `state` key + agent_id
    assert_eq!(agents.len(), 3);

    // agent-active: installed, enabled, runtime Active, id surfaced
    assert_eq!(agents[0]["name"], "agent-active");
    assert_eq!(agents[0]["state"], "Active");
    assert_eq!(agents[0]["agent_id"], "uuid-1");
    assert_eq!(agents[0]["enabled"], true);
    assert_eq!(agents[0]["installed"], true);
    assert!(
        agents[0].get("status").is_none(),
        "legacy `status` key must be gone"
    );

    // agent-disabled: installed, disabled, not in runtime -> no agent_id
    assert_eq!(agents[1]["name"], "agent-disabled");
    assert_eq!(agents[1]["state"], "-");
    assert_eq!(agents[1]["agent_id"], serde_json::Value::Null);
    assert_eq!(agents[1]["enabled"], false);
    assert_eq!(agents[1]["installed"], true);

    // runtime-only: not installed, id surfaced
    assert_eq!(agents[2]["name"], "runtime-only");
    assert_eq!(agents[2]["state"], "Active");
    assert_eq!(agents[2]["agent_id"], "uuid-3");
    assert_eq!(agents[2]["installed"], false);
}

// error for uninstall of nonexistent agent
#[test]
fn test_uninstall_not_found_error() {
    // GIVEN a repository with no agents
    let repo = AgentRepository::open(Path::new(":memory:")).expect("open in-memory repo");
    // WHEN checking for a nonexistent agent
    let result = repo.stored("inexistant").expect("stored should not error");
    // THEN the agent is not found
    assert!(result.is_none());
}

// helper functions work without runtime
#[test]
fn test_data_dir_resolution() {
    // GIVEN the HOME environment variable
    // WHEN resolving the data dir
    let dir = apollia_data_dir();
    // THEN it ends with .apollia
    assert!(dir.to_string_lossy().ends_with(".apollia"));
}

#[test]
fn test_looks_like_file_path_detection() {
    // GIVEN various arguments
    // WHEN each is tested against the file-path heuristic
    // THEN file-like args are detected
    assert!(looks_like_file_path("agents/foo.py"));
    assert!(looks_like_file_path("./agent.py"));
    assert!(looks_like_file_path("/abs/path/agent.py"));
    assert!(!looks_like_file_path("my-agent"));
    assert!(!looks_like_file_path("uuid-1234"));
}

// The name-conflict branch of `run_new` has no test: it sits behind the SDK
// probe, so a test that called `run_new` would read GENERAL_ERROR whether the
// name clashed or python3 simply had no `apollia` module. The test that stood
// here created a directory and asserted the directory existed, which is the
// assertion the tempdir crate owns.

#[test]
fn test_a2a_skill_id_field_name() {
    // GIVEN a skill DTO JSON as returned by GET /api/v1/a2a/agents
    let skill = serde_json::json!({
        "id": "read-excel",
        "name": "Read Excel",
        "description": "Read an Excel workbook.",
        "input_modes": ["text"],
        "output_modes": ["text"]
    });

    // WHEN reading the skill identifier
    let id = skill.get("id").and_then(|v| v.as_str()).unwrap_or("?");
    let legacy = skill
        .get("skill_id")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    // THEN "id" resolves correctly and "skill_id" is absent
    assert_eq!(id, "read-excel");
    assert_eq!(legacy, "?");
}

#[test]
fn test_format_a2a_agent_list_empty_no_agents_key() {
    // GIVEN a response with no "agents" key
    let resp = serde_json::json!({});

    // WHEN extracting the agents array
    let agents = resp.get("agents").and_then(|v| v.as_array());

    // THEN agents is None
    assert!(agents.is_none());
}

#[test]
fn test_format_a2a_agent_list_empty_array() {
    // GIVEN a response with an empty agents array
    let resp = serde_json::json!({ "agents": [] });

    // WHEN extracting the agents array
    let agents = resp
        .get("agents")
        .and_then(|v| v.as_array())
        .expect("agents array");

    // THEN the array is empty
    assert!(agents.is_empty());
}

// ── agent logs parsing ────────────────────────────────────────────────────

#[test]
fn test_agent_logs_parses_defaults() {
    // GIVEN "apollia-os agent logs devis-generator"
    use clap::Parser;

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: AgentCommand,
    }

    let cli = TestCli::parse_from(["test", "logs", "devis-generator"]);
    // WHEN clap parses the argument line
    // THEN AgentCommand::Logs with default last=50, follow=false
    match cli.cmd {
        AgentCommand::Logs {
            agent_id,
            last,
            follow,
        } => {
            assert_eq!(agent_id, "devis-generator");
            assert_eq!(last, 50);
            assert!(!follow);
        }
        other => panic!("expected AgentCommand::Logs, got {other:?}"),
    }
}

#[test]
fn test_agent_logs_parses_last_flag() {
    // GIVEN "agent logs devis-generator --last 20"
    use clap::Parser;

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: AgentCommand,
    }

    let cli = TestCli::parse_from(["test", "logs", "devis-generator", "--last", "20"]);
    // WHEN clap parses the argument line
    // THEN last=20
    match cli.cmd {
        AgentCommand::Logs { last, .. } => assert_eq!(last, 20),
        other => panic!("expected AgentCommand::Logs, got {other:?}"),
    }
}

#[test]
fn test_agent_logs_parses_follow_flag() {
    // GIVEN "agent logs devis-generator --follow"
    use clap::Parser;

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: AgentCommand,
    }

    let cli = TestCli::parse_from(["test", "logs", "devis-generator", "--follow"]);
    // WHEN clap parses the argument line
    // THEN follow=true
    match cli.cmd {
        AgentCommand::Logs { follow, .. } => assert!(follow),
        other => panic!("expected AgentCommand::Logs, got {other:?}"),
    }
}

#[test]
fn test_agent_logs_file_path_rejected() {
    // GIVEN an agent_id that looks like a file path
    // WHEN each is tested against the file-path heuristic
    // THEN looks_like_file_path returns true
    assert!(looks_like_file_path("agents/foo.py"));
    assert!(looks_like_file_path("./agent.py"));
    // AND agent names are not rejected
    assert!(!looks_like_file_path("devis-generator"));
    assert!(!looks_like_file_path("rapport-hebdo"));
}

#[test]
fn test_format_a2a_agent_list_skills_read_from_id_field() {
    // GIVEN an A2A agents response with skills using the "id" key
    let resp = serde_json::json!({
        "agents": [{
            "agent_id": "uuid-1",
            "name": "excel-worker",
            "version": "0.1.0",
            "state": "active",
            "skills": [
                { "id": "read-excel", "name": "Read Excel", "description": "Reads an Excel file.", "input_modes": ["text"], "output_modes": ["text"] },
                { "id": "edit-excel", "name": "Edit Excel", "description": "", "input_modes": ["text"], "output_modes": ["file"] }
            ]
        }]
    });

    // WHEN extracting skill IDs
    let agents = resp["agents"].as_array().expect("agents");
    let skills = agents[0]["skills"].as_array().expect("skills");
    let ids: Vec<&str> = skills
        .iter()
        .filter_map(|s| s.get("id").and_then(|v| v.as_str()))
        .collect();

    // THEN both skill IDs are correctly resolved
    assert_eq!(ids, vec!["read-excel", "edit-excel"]);
}

#[test]
fn test_agent_logs_reads_the_actions_of_the_agent_in_the_journal() {
    // GIVEN an agent whose name needs encoding, and a number of lines asked for
    // WHEN the page `agent logs` reads is built
    let uri = super::lifecycle::agent_logs_uri("mail sorter", 20);

    // THEN it is a page of the audit journal, narrowed to the actions of that
    // agent, of the size asked for
    assert_eq!(
        uri,
        "/api/v1/audit/journal?limit=20&offset=0&agents=mail%20sorter&nature=action"
    );
}

/// Identifiers of the agents the packages of this repository declare, under
/// `agents/`.
///
/// Reads the tree rather than a hardcoded list: an agent removed from `agents/`
/// must make the assertion below fail, which a duplicated list would not do.
fn shipped_agent_names() -> Vec<String> {
    let agents_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../agents");
    let mut names = Vec::new();
    for family in ["examples", "system"] {
        let Ok(entries) = std::fs::read_dir(agents_root.join(family)) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(raw) = std::fs::read_to_string(entry.path().join("apollia.toml")) else {
                continue;
            };
            let Ok(manifest) = apollia_aip::apollia_manifest::parse(&raw) else {
                continue;
            };
            names.extend(manifest.agents.into_iter().map(|agent| agent.id));
        }
    }
    names
}

/// Sub-command names `apollia-os agent` really accepts, read from the parser.
fn agent_subcommand_names() -> Vec<String> {
    let command = AgentCommand::augment_subcommands(clap::Command::new("agent"));
    command
        .get_subcommands()
        .map(|sub| sub.get_name().to_string())
        .collect()
}

#[test]
fn file_path_hint_offers_a_command_an_operator_can_really_run() {
    // GIVEN the agents this repository ships and the sub-commands the parser
    // accepts, both read from the tree rather than restated here
    let shipped = shipped_agent_names();
    let subcommands = agent_subcommand_names();
    assert!(
        !shipped.is_empty() && !subcommands.is_empty(),
        "nothing read from agents/ or from the parser, the assertions below would be vacuous"
    );

    // WHEN every hint the crate can render is built
    let example = lifecycle::HINT_EXAMPLE_AGENT;

    // THEN the agent each one names is one the repository ships
    assert!(
        shipped.iter().any(|name| name == example),
        "hint example '{example}' is not shipped, an operator copying it lands on `agent not found`; shipped: {shipped:?}"
    );

    // AND the verb each one spells out is one the parser accepts, retyped whole
    for verb in lifecycle::HintVerb::ALL {
        let spelled = verb.as_str();
        assert!(
            subcommands.iter().any(|name| name == spelled),
            "hint verb '{spelled}' is not an agent sub-command, retyping the hint fails again; accepted: {subcommands:?}"
        );
        let hint = lifecycle::file_path_hint(verb, "agents/my_agent.py");
        assert!(
            hint.contains(&format!("apollia-os agent {spelled} {example}")),
            "hint for '{spelled}': {hint}"
        );
    }
}

#[test]
fn test_override_value_follows_the_field_shape() {
    // GIVEN values typed for a string field and for a list field
    let one = vec!["Sort the inbox.".to_owned()];
    let two = vec!["file_read".to_owned(), "web_fetch".to_owned()];
    // WHEN they are turned into override values
    let mission = definition::override_value("mission", &one);
    let access = definition::override_value("access", &two);
    let empty = definition::override_value("sources", &[]);
    let too_many = definition::override_value("mission", &two);
    let unknown = definition::override_value("model", &one);
    // THEN a string field takes exactly one value and a list field any number
    assert_eq!(mission.unwrap(), serde_json::json!("Sort the inbox."));
    assert_eq!(
        access.unwrap(),
        serde_json::json!(["file_read", "web_fetch"])
    );
    assert_eq!(empty.unwrap(), serde_json::json!([]));
    assert!(too_many.unwrap_err().contains("exactly one value"));
    assert!(unknown.unwrap_err().contains("unknown field"));
}

#[tokio::test]
async fn test_uninstall_refuses_the_apollia_agent() {
    // GIVEN the identifier of the Apollia agent, the built-in chat, and no
    // runtime behind the socket
    let dir = tempfile::tempdir().unwrap();
    let client = RuntimeClient::new(dir.path().join("absent.sock"));
    let erase = Erase {
        memory: false,
        history: false,
    };
    // WHEN it is uninstalled, confirmed
    let code = run_uninstall(&client, apollia_tools::APOLLIA_AGENT_ID, erase, true, true).await;
    // THEN the command refuses it with a usage error, before reaching the runtime
    assert_eq!(code, exit_codes::GENERAL_ERROR);
}

#[tokio::test]
async fn test_uninstall_without_a_runtime_is_a_runtime_error() {
    // GIVEN no runtime behind the socket
    let dir = tempfile::tempdir().unwrap();
    let client = RuntimeClient::new(dir.path().join("absent.sock"));
    let erase = Erase {
        memory: true,
        history: true,
    };
    // WHEN an agent and a package are uninstalled, confirmed
    let agent = run_uninstall(&client, "notes", erase, true, true).await;
    let package = run_package_uninstall(&client, "crm", erase, true, true).await;
    // THEN nothing is removed offline: both answer the runtime error
    assert_eq!(agent, exit_codes::RUNTIME_ERROR);
    assert_eq!(package, exit_codes::RUNTIME_ERROR);
}
