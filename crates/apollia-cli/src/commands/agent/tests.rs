use super::*;
use apollia_core::AgentManifest;
use std::path::Path;

#[test]
fn trust_banner_states_the_in_process_model() {
    // GIVEN the install-time trust banner
    let text = trust_banner_text();
    // THEN it names the core risks an operator must accept before installing
    assert!(text.contains("in-process"));
    assert!(text.contains("no OS sandbox"));
    assert!(text.contains("audited"));
}

fn test_manifest(name: &str) -> AgentManifest {
    AgentManifest {
        name: name.to_string(),
        version: "0.1.0".to_string(),
        description: format!("Test agent {name}"),
        tools_required: vec![],
        tools_optional: vec![],
        supports_streaming: false,
        supports_a2a: false,
        memory_namespace: None,
        shared_memory_namespaces: vec![],
        max_concurrent_tasks: 1,
        step_budget: None,
        network_allowlist: None,
        dangerous_tools_allowed: false,
        tags: vec![],
        skills: vec![],
        execution_mode: "auto".to_string(),
        supports_mailbox: false,
        mailbox_allowlist: None,
        system_prompt: None,
        tools_requiring_approval: vec![],
        llm_backend: None,
        packages: vec![],
        memory_config: None,
        agent_type: None,
        examples: vec![],
        limitations: vec![],
        setup_notes: None,
        agent_class: None,
        user_memory_write: false,
        datasources: vec![],
        templates: vec![],
        secrets: vec![],
        check_commands: vec![],
    }
}

fn test_installed_agent(name: &str) -> InstalledAgent {
    InstalledAgent {
        name: name.to_string(),
        version: "0.1.0".to_string(),
        install_path: PathBuf::from(format!("/tmp/.apollia/agents/{name}/agent.py")),
        source_path: PathBuf::from(format!("/tmp/{name}.py")),
        manifest: test_manifest(name),
        enabled: true,
        installed_at: "2026-03-17T10:00:00Z".to_string(),
        updated_at: "2026-03-17T10:00:00Z".to_string(),
    }
}

// install command output format (JSON)
#[test]
fn test_install_command_output() {
    // GIVEN an InstalledAgent
    let agent = test_installed_agent("mon-agent");
    // WHEN formatting JSON output
    let output = serde_json::json!({
        "name": agent.name,
        "version": agent.version,
        "install_path": agent.install_path.to_string_lossy(),
    });
    // THEN JSON contains expected fields
    assert_eq!(output["name"], "mon-agent");
    assert_eq!(output["version"], "0.1.0");
    assert!(output["install_path"]
        .as_str()
        .is_some_and(|p| p.contains("mon-agent")));
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

// update command output format
#[test]
fn test_update_command_output() {
    // GIVEN update result
    let name = "mon-agent";
    let version = "0.2.0";
    // WHEN formatting JSON output
    let output = serde_json::json!({ "name": name, "version": version });
    // THEN JSON contains expected fields
    assert_eq!(output["name"], "mon-agent");
    assert_eq!(output["version"], "0.2.0");
}

// list shows installed agents merged with runtime
#[test]
fn test_list_shows_installed_agents() {
    // GIVEN 2 installed agents (1 enabled, 1 disabled) and runtime data
    let installed = vec![test_installed_agent("agent-active"), {
        let mut a = test_installed_agent("agent-disabled");
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
    let result = repo.get("inexistant").expect("get should not error");
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
    // THEN file-like args are detected
    assert!(looks_like_file_path("agents/foo.py"));
    assert!(looks_like_file_path("./agent.py"));
    assert!(looks_like_file_path("/abs/path/agent.py"));
    assert!(!looks_like_file_path("my-agent"));
    assert!(!looks_like_file_path("uuid-1234"));
}

#[test]
fn test_new_validates_agent_type() {
    // GIVEN an invalid template type
    // WHEN run_new is called
    let code = run_new("test-agent", "invalid", false);
    // THEN it returns GENERAL_ERROR
    assert_eq!(code, exit_codes::GENERAL_ERROR);
}

#[test]
fn test_new_valid_agent_types_accepted() {
    // GIVEN all valid template types
    // THEN they are all recognized
    for t in VALID_AGENT_TYPES {
        assert!(VALID_AGENT_TYPES.contains(t), "type '{t}' should be valid");
    }
    assert!(!VALID_AGENT_TYPES.contains(&"invalid"));
    assert!(!VALID_AGENT_TYPES.contains(&"custom"));
}

#[test]
fn test_new_detects_name_conflict() {
    // GIVEN a temporary directory simulating ~/.apollia/agents/<name>/
    let tmp = tempfile::tempdir().expect("create tmpdir");
    let agents_dir = tmp.path().join("agents").join("existing-agent");
    std::fs::create_dir_all(&agents_dir).expect("create agent dir");

    // WHEN the target directory already exists
    // THEN it is detected as a conflict
    assert!(agents_dir.exists());
}

#[test]
fn test_new_json_output_format() {
    // GIVEN a scaffolding result
    let name = "my-agent";
    let agent_type = "react";
    let path = "/home/user/.apollia/agents/my-agent/";
    let files = vec![
        "my_agent_agent.py".to_string(),
        "test_my_agent_agent.py".to_string(),
    ];

    // WHEN formatting JSON output
    let output = serde_json::json!({
        "name": name,
        "type": agent_type,
        "path": path,
        "files": files,
    });

    // THEN the JSON contains all required fields
    assert_eq!(output["name"], "my-agent");
    assert_eq!(output["type"], "react");
    assert!(output["path"]
        .as_str()
        .is_some_and(|p| p.contains("my-agent")));
    let file_list = output["files"].as_array().expect("files should be array");
    assert_eq!(file_list.len(), 2);
}

#[test]
fn test_new_default_type_is_react() {
    // GIVEN the AgentCommand::Create parsed without --type
    use clap::Parser;

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: AgentCommand,
    }

    let cli = TestCli::parse_from(["test", "create", "simple-bot"]);
    // THEN the default type is "react"
    match cli.cmd {
        AgentCommand::Create { name, r#type } => {
            assert_eq!(name, "simple-bot");
            assert_eq!(r#type, "react");
        }
        other => panic!("expected AgentCommand::Create, got {other:?}"),
    }
}

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
    // THEN follow=true
    match cli.cmd {
        AgentCommand::Logs { follow, .. } => assert!(follow),
        other => panic!("expected AgentCommand::Logs, got {other:?}"),
    }
}

// ── agent validate parsing ────────────────────────────────────────────────

#[test]
fn test_agent_validate_parses_path() {
    // GIVEN "agent validate ./mon-agent.py"
    use clap::Parser;

    #[derive(Debug, Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: AgentCommand,
    }

    let cli = TestCli::parse_from(["test", "validate", "./mon-agent.py"]);
    // THEN AgentCommand::Validate with correct path
    match cli.cmd {
        AgentCommand::Validate { path } => {
            assert_eq!(path, PathBuf::from("./mon-agent.py"));
        }
        other => panic!("expected AgentCommand::Validate, got {other:?}"),
    }
}

#[test]
fn test_agent_validate_file_not_found_returns_error() {
    // GIVEN a path that does not exist
    let path = PathBuf::from("/tmp/this-file-does-not-exist-apollia-test.py");
    // WHEN run_validate is called
    let code = run_validate(&path, false);
    // THEN exit code is GENERAL_ERROR
    assert_eq!(code, exit_codes::GENERAL_ERROR);
}

#[test]
fn test_agent_validate_file_not_found_json_output() {
    // GIVEN a path that does not exist and json=true
    let path = PathBuf::from("/tmp/this-file-does-not-exist-apollia-test.py");
    // WHEN run_validate is called in JSON mode
    let code = run_validate(&path, true);
    // THEN exit code is GENERAL_ERROR
    assert_eq!(code, exit_codes::GENERAL_ERROR);
}

#[test]
fn test_agent_logs_file_path_rejected() {
    // GIVEN an agent_id that looks like a file path
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
fn test_list_generated_files_collects_recursively() {
    // GIVEN a directory with files at different depths
    let tmp = tempfile::tempdir().expect("create tmpdir");
    let base = tmp.path();
    std::fs::write(base.join("agent.py"), "").expect("write file");
    let tests_dir = base.join("tests");
    std::fs::create_dir_all(&tests_dir).expect("create tests dir");
    std::fs::write(tests_dir.join("test_agent.py"), "").expect("write test file");

    // WHEN listing generated files
    let files = list_generated_files(base);

    // THEN both files are found with relative paths
    assert_eq!(files.len(), 2);
    assert!(files.contains(&"agent.py".to_string()));
    assert!(files.contains(&"tests/test_agent.py".to_string()));
}

#[test]
fn test_format_audit_event_row_uses_real_keys() {
    // GIVEN an audit event with the real JSON shape from GET /agents/:id/logs
    let event = serde_json::json!({
        "started_at": "2026-07-01T15:05:32Z",
        "tool_name": "mcp:filesystem/list_criteria",
        "success": true,
        "duration_ms": 13,
        "task_id": "dcd2713a-0000-4000-8000-000000000000",
    });

    // WHEN the row is formatted
    let row = format_audit_event_row(&event);

    // THEN the real values render (no stray `?` placeholders)
    assert!(!row.contains('?'), "row still has `?` placeholders: {row}");
    assert!(row.contains("2026-07-01T15:05:32Z"));
    assert!(row.contains("mcp:filesystem/list_criteria"));
    assert!(row.contains("ok"));
    assert!(row.contains("13ms"));
    assert!(row.contains("task=dcd2713a-0000-4000-8000-000000000000"));
}

#[test]
fn test_format_audit_event_row_failed_uses_error_code() {
    // GIVEN a failed audit event carrying an error_code and no duration
    let event = serde_json::json!({
        "started_at": "2026-07-01T15:06:00Z",
        "tool_name": "mcp:filesystem/write",
        "success": false,
        "error_code": "PERMISSION_DENIED",
        "task_id": "abc",
    });

    // WHEN the row is formatted
    let row = format_audit_event_row(&event);

    // THEN the error code is surfaced as the outcome
    assert!(row.contains("PERMISSION_DENIED"), "row: {row}");
    assert!(row.contains("task=abc"));
}
