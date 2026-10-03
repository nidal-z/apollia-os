use super::*;

const COMPLETE: &str = include_str!("fixtures/complete.toml");

const MINIMAL_TOOL_ONLY: &str = r#"
[package]
name = "lone"
version = "0.1.0"
publisher = "Someone"
sdk = ">=0.3"

[[tools]]
name = "ping"
module = "tools/ping.py"
description = "Answer pong."
action = "read"
"#;

/// The complete fixture with `line` inserted right after the first line equal
/// to `header` (or at the top when `header` is empty).
fn with_line_after(header: &str, line: &str) -> String {
    if header.is_empty() {
        return format!("{line}\n{COMPLETE}");
    }
    let mut out = String::new();
    let mut done = false;
    for l in COMPLETE.lines() {
        out.push_str(l);
        out.push('\n');
        if !done && l == header {
            out.push_str(line);
            out.push('\n');
            done = true;
        }
    }
    assert!(done, "header {header} not in fixture");
    out
}

fn complete() -> ApolliaManifest {
    parse(COMPLETE).expect("fixture parses")
}

// ── Criterion 1: strict schema ────────────────────────────────────────────

#[test]
fn test_parse_unknown_key_at_every_level_is_refused_with_its_path() {
    // GIVEN the fixture with an unknown key added under each table in turn
    let cases = [
        ("", "bogus"),
        ("[package]", "package.bogus"),
        ("[[agents]]", "agents[0].bogus"),
        ("[agents.defaults]", "agents[0].defaults.bogus"),
        ("[agents.defaults.model]", "agents[0].defaults.model.bogus"),
        (
            "[agents.defaults.model.request]",
            "agents[0].defaults.model.request.bogus",
        ),
        ("[[tools]]", "tools[0].bogus"),
        ("[[workflows]]", "workflows[0].bogus"),
        ("[[requires]]", "requires[0].bogus"),
        ("[[mcp_servers]]", "mcp_servers[0].bogus"),
        ("[pip]", "pip.bogus"),
        ("[[triggers]]", "triggers[0].bogus"),
        ("[triggers.source]", "triggers[0].source.bogus"),
    ];

    // WHEN each variant is parsed
    let got: Vec<Result<ApolliaManifest, ApolliaManifestError>> = cases
        .iter()
        .map(|(header, _)| parse(&with_line_after(header, "bogus = 1")))
        .collect();

    // THEN each is refused as an unknown key, named by its full path
    let expected: Vec<Result<ApolliaManifest, ApolliaManifestError>> = cases
        .iter()
        .map(|(_, path)| {
            Err(ApolliaManifestError::UnknownKey {
                path: path.to_string(),
            })
        })
        .collect();
    assert_eq!(got, expected);
}

#[test]
fn test_parse_unknown_key_in_inline_table_names_its_path() {
    // GIVEN a misspelt request setting written as inline tables
    let text = format!(
        "{MINIMAL_TOOL_ONLY}\n[[agents]]\nid = \"a\"\nname = \"A\"\nmission = \"m\"\n\
         defaults = {{ model = {{ request = {{ temprature = 0.1 }} }} }}\n"
    );

    // WHEN it is parsed
    let err = parse(&text);

    // THEN the refusal names the misspelt key under the agent
    assert_eq!(
        err,
        Err(ApolliaManifestError::UnknownKey {
            path: "agents[0].defaults.model.request.temprature".into()
        })
    );
}

#[test]
fn test_parse_unknown_key_in_inline_trigger_source_names_its_path() {
    // GIVEN an inline trigger source with an extra key
    let text = format!(
        "{MINIMAL_TOOL_ONLY}\n[[workflows]]\nname = \"w\"\nmodule = \"w.py\"\n\
         description = \"d\"\naction = \"read\"\n\n[[triggers]]\nid = \"t\"\n\
         target = \"w\"\nsource = {{ type = \"cron\", schedule = \"* * * * *\", secret = \"x\" }}\n"
    );

    // WHEN it is parsed
    let err = parse(&text);

    // THEN the extra key is named under the source
    assert_eq!(
        err,
        Err(ApolliaManifestError::UnknownKey {
            path: "triggers[0].source.secret".into()
        })
    );
}

#[test]
fn test_parse_wrong_type_is_a_schema_error_at_its_path() {
    // GIVEN a version written as a number
    let text = MINIMAL_TOOL_ONLY.replace("version = \"0.1.0\"", "version = 1");

    // WHEN it is parsed
    let err = parse(&text);

    // THEN the error is a schema error located on the key's value
    assert!(
        matches!(&err, Err(ApolliaManifestError::Schema { path, .. }) if path == "package.version"),
        "{err:?}"
    );
}

#[test]
fn test_parse_action_family_outside_the_five_is_refused() {
    // GIVEN a tool whose family is the former `delete`
    let text = MINIMAL_TOOL_ONLY.replace("action = \"read\"", "action = \"delete\"");

    // WHEN it is parsed
    let err = parse(&text);

    // THEN it is refused at the tool's action
    assert!(
        matches!(&err, Err(ApolliaManifestError::Schema { path, .. }) if path == "tools[0].action"),
        "{err:?}"
    );
}

#[test]
fn test_parse_autonomy_outside_the_four_tiers_is_refused() {
    // GIVEN an agent whose default tier does not exist
    let text = COMPLETE.replace("autonomy = \"supervised\"", "autonomy = \"unbounded\"");

    // WHEN it is parsed
    let err = parse(&text);

    // THEN it is refused at the agent's default tier
    assert!(
        matches!(&err, Err(ApolliaManifestError::Schema { path, .. }) if path == "agents[0].defaults.autonomy"),
        "{err:?}"
    );
}

#[test]
fn test_agent_defaults_carry_the_declared_autonomy() {
    // GIVEN the complete fixture, which declares a supervised agent

    // WHEN it is read
    let m = complete();

    // THEN the agent's default tier is the declared one
    assert_eq!(
        m.agents[0].defaults.autonomy,
        Some(apollia_core::AutonomyLevel::Supervised)
    );
}

#[test]
fn test_action_family_sensitive_is_every_family_but_read() {
    // GIVEN the five families
    let all = [
        ActionFamily::Read,
        ActionFamily::ReadPersonal,
        ActionFamily::WriteOrDelete,
        ActionFamily::SendExternal,
        ActionFamily::ExecuteCode,
    ];

    // WHEN asking which are sensitive
    let sensitive: Vec<bool> = all.iter().map(|f| f.is_sensitive()).collect();

    // THEN only `read` is not
    assert_eq!(sensitive, [false, true, true, true, true]);
}

// ── Criterion 2: validation, one test per rule ───────────────────────────

#[test]
fn test_validate_package_without_agent_with_one_tool_is_valid() {
    // GIVEN a package with a single tool and no agent

    // WHEN it is parsed
    let manifest = parse(MINIMAL_TOOL_ONLY);

    // THEN it is accepted
    assert!(manifest.is_ok(), "{manifest:?}");
}

#[test]
fn test_validate_package_without_agent_workflow_or_tool_is_refused() {
    // GIVEN a package with nothing in it
    let text =
        "[package]\nname = \"void\"\nversion = \"0.1.0\"\npublisher = \"x\"\nsdk = \">=0.3\"\n";

    // WHEN it is parsed
    let err = parse(text);

    // THEN it is refused as empty
    assert_eq!(err, Err(ApolliaManifestError::EmptyPackage));
}

#[test]
fn test_validate_identifier_shared_by_agent_and_tool_is_refused() {
    // GIVEN the fixture with its agent renamed like its tool
    let mut manifest = complete();
    manifest.agents[0].id = "extract_totals".into();
    manifest.agents[0].defaults.access.clear();

    // WHEN it is validated
    let err = manifest.validate();

    // THEN the duplicate is named
    assert_eq!(
        err,
        Err(ApolliaManifestError::DuplicateId {
            id: "extract_totals".into()
        })
    );
}

#[test]
fn test_validate_duplicate_trigger_id_is_refused() {
    // GIVEN the fixture with its trigger declared twice
    let mut manifest = complete();
    manifest.triggers.push(manifest.triggers[0].clone());

    // WHEN it is validated
    let err = manifest.validate();

    // THEN the duplicate is named
    assert_eq!(
        err,
        Err(ApolliaManifestError::DuplicateId {
            id: "first-monday".into()
        })
    );
}

#[test]
fn test_validate_identifier_outside_alphabet_is_refused() {
    // GIVEN a tool name with an upper-case letter
    let mut manifest = complete();
    manifest.tools[0].name = "Extract".into();
    manifest.agents[0].defaults.access.clear();

    // WHEN it is validated
    let err = manifest.validate();

    // THEN the field and value are named
    assert_eq!(
        err,
        Err(ApolliaManifestError::InvalidIdentifier {
            field: "tools.name".into(),
            value: "Extract".into()
        })
    );
}

#[test]
fn test_validate_package_access_to_unknown_tool_is_refused() {
    // GIVEN an agent granted a package tool that does not exist
    let mut manifest = complete();
    manifest.agents[0].defaults.access = vec!["pkg:invoices/missing".into()];

    // WHEN it is validated
    let err = manifest.validate();

    // THEN the access is refused
    assert_eq!(
        err,
        Err(ApolliaManifestError::UnknownPackageTool {
            agent: "invoice-clerk".into(),
            access: "pkg:invoices/missing".into(),
            package: "invoices".into()
        })
    );
}

#[test]
fn test_validate_package_access_to_another_package_is_refused() {
    // GIVEN an agent granted a tool of another package, named like an own tool
    let mut manifest = complete();
    manifest.agents[0].defaults.access = vec!["pkg:other/extract_totals".into()];

    // WHEN it is validated
    let err = manifest.validate();

    // THEN the access is refused
    assert!(
        matches!(err, Err(ApolliaManifestError::UnknownPackageTool { .. })),
        "{err:?}"
    );
}

#[test]
fn test_validate_mcp_server_name_outside_alphabet_is_refused() {
    // GIVEN an MCP server name with a space
    let mut manifest = complete();
    manifest.mcp_servers[0].name = "my ledger".into();

    // WHEN it is validated
    let err = manifest.validate();

    // THEN the name is refused
    assert_eq!(
        err,
        Err(ApolliaManifestError::InvalidMcpServerName {
            name: "my ledger".into()
        })
    );
}

#[test]
fn test_validate_version_not_semver_is_refused() {
    // GIVEN a two-part version
    let mut manifest = complete();
    manifest.package.version = "1.2".into();

    // WHEN it is validated
    let err = manifest.validate();

    // THEN the version is refused
    assert_eq!(
        err,
        Err(ApolliaManifestError::InvalidVersion {
            value: "1.2".into()
        })
    );
}

#[test]
fn test_validate_sdk_requirement_not_semver_is_refused() {
    // GIVEN an SDK requirement that is no requirement
    let mut manifest = complete();
    manifest.package.sdk = "latest".into();

    // WHEN it is validated
    let err = manifest.validate();

    // THEN the requirement is refused
    assert_eq!(
        err,
        Err(ApolliaManifestError::InvalidSdkRequirement {
            value: "latest".into()
        })
    );
}

#[test]
fn test_validate_trigger_targeting_a_tool_is_refused() {
    // GIVEN a trigger aimed at a tool rather than an agent or a workflow
    let mut manifest = complete();
    manifest.triggers[0].target = "extract_totals".into();

    // WHEN it is validated
    let err = manifest.validate();

    // THEN the target is refused
    assert_eq!(
        err,
        Err(ApolliaManifestError::UnknownTriggerTarget {
            trigger: "first-monday".into(),
            target: "extract_totals".into()
        })
    );
}

// ── Criterion 3: round trip ──────────────────────────────────────────────

#[test]
fn test_complete_fixture_round_trips_identically() {
    // GIVEN the complete commented fixture, read
    let first = complete();

    // WHEN it is written back and read again
    let text = first.to_toml().expect("serialises");
    let second = parse(&text).expect("re-reads");

    // THEN both readings are identical
    assert_eq!(second, first);
}

#[test]
fn test_complete_fixture_reads_every_section() {
    // GIVEN the complete fixture

    // WHEN it is read
    let m = complete();

    // THEN every section carries what the fixture wrote
    let counts = (
        m.agents.len(),
        m.tools.len(),
        m.workflows.len(),
        m.requires.len(),
        m.mcp_servers.len(),
        m.pip.as_ref().map(|p| p.packages.len()),
        m.triggers.len(),
        m.agents[0]
            .defaults
            .sources
            .contains(&PROFILE_SOURCE.to_string()),
        m.agents[0]
            .defaults
            .model
            .as_ref()
            .and_then(|mo| mo.request.as_ref())
            .and_then(|r| r.top_k),
    );
    assert_eq!(counts, (1, 1, 1, 1, 1, Some(1), 1, true, Some(40)));
}

#[test]
fn test_to_toml_with_null_in_input_schema_is_a_serialize_error() {
    // GIVEN a tool whose input schema holds a JSON null
    let mut manifest = complete();
    manifest.tools[0].inputs = Some(serde_json::json!({ "default": null }));

    // WHEN it is written
    let err = manifest.to_toml();

    // THEN serialisation fails with a typed error
    assert!(
        matches!(err, Err(ApolliaManifestError::Serialize { .. })),
        "{err:?}"
    );
}

#[test]
fn test_the_manifest_the_python_sdk_writes_is_read_as_written() {
    // GIVEN the manifest `python -m apollia describe` writes for the SDK's
    // example package; the SDK test suite holds that file equal to its output
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sdk/tests/fixtures/invoice_pkg_described.toml");
    let text = std::fs::read_to_string(&path).expect("the SDK fixture is in the tree");

    // WHEN it is parsed with the strict schema
    let m = parse(&text).expect("the SDK writes nothing the schema refuses");

    // THEN every descriptor is read, with its family, steps and inputs, the
    //      date, long text and file inputs with their format, in their order
    let tools: Vec<(&str, bool)> = m
        .tools
        .iter()
        .map(|t| (t.name.as_str(), t.is_sensitive()))
        .collect();
    assert_eq!(tools, [("extract_totals", false), ("file_totals", true)]);
    let workflow = &m.workflows[0];
    assert_eq!(workflow.name, "close-month");
    assert_eq!(workflow.steps, ["extract", "file"]);
    assert_eq!(workflow.action, ActionFamily::WriteOrDelete);
    let inputs = workflow
        .inputs
        .as_ref()
        .expect("the workflow declares inputs");
    let format = |name: &str| inputs["properties"][name]["format"].as_str();
    assert_eq!(format("closing_day"), Some("date"));
    assert_eq!(format("note"), Some("long-text"));
    assert_eq!(format("ledger"), Some("file-path"));
    assert_eq!(format("folder"), None);
    assert_eq!(
        inputs["x-order"],
        serde_json::json!(["folder", "notify", "closing_day", "note", "ledger"])
    );
}

#[test]
fn test_is_older_version_compares_semver_and_ignores_what_does_not_parse() {
    // GIVEN pairs of incoming and installed versions
    let cases = [
        ("1.0.0", "1.1.0", true),
        ("1.2.0", "1.1.0", false),
        ("1.1.0", "1.1.0", false),
        ("1.10.0", "1.9.0", false),
        ("not-a-version", "1.0.0", false),
    ];

    for (incoming, installed, expected) in cases {
        // WHEN they are compared
        let older = is_older_version(incoming, installed);

        // THEN only a strictly lower semver version is older
        assert_eq!(older, expected, "{incoming} against {installed}");
    }
}
