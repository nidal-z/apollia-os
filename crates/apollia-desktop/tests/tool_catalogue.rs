//! The production tool surface the UI catalogues must cover.
//!
//! `ui/src/lib/i18n/production-tools.json` is the contract between the
//! runtime (which registers the tools) and the desktop catalogues (which must
//! carry `tools.labels.*` and `tools.descriptions.*` for each of them: the
//! HITL approval card resolves `tools.descriptions.<name>` with no fallback,
//! so a missing key renders as a raw key on the card). This test rebuilds the
//! production list from the same registration sources the runtime uses and
//! refuses a fixture that drifted; `tool-keys-complete.test.ts` (vitest)
//! crosses the fixture with `en.json` / `fr.json`.

use std::collections::BTreeSet;
use std::path::Path;

use apollia_mcp::mcp_resources::{MCP_RESOURCES_LIST, MCP_RESOURCES_READ};
use apollia_runtime::chat::plan_tool::{
    PLAN_ADD_STEP_TOOL_NAME, PLAN_MODIFY_STEP_TOOL_NAME, PLAN_PROPOSE_TOOL_NAME,
    PLAN_REMOVE_STEP_TOOL_NAME, PLAN_REORDER_TOOL_NAME, PLAN_SET_STEP_STATUS_TOOL_NAME,
    PLAN_SUBMIT_TOOL_NAME,
};
use apollia_runtime::chat::todo_tool::TODO_WRITE_TOOL_NAME;
use apollia_tools::tools::ask_user::PendingUserInputs;
use apollia_tools::{build_native_dispatcher, NativeDispatcherConfig};

/// Natives whose registration is feature-gated or config-gated
/// (`http`, `web-search`, `web-read`, `memory-search`). Their names are
/// stable strings; listing them keeps the fixture complete even when this
/// test binary is compiled without one of the features.
const GATED_NATIVES: &[&str] = &["http_fetch", "web_search", "web_read", "memory_search"];

/// Synthetic tool registered by the MCP manager in deferred loading mode.
/// `ToolSearchExecutor::name()` in `apollia-mcp/src/tool_search.rs` returns
/// this string; there is no exported constant for it.
const TOOL_SEARCH: &str = "tool_search";

#[test]
fn production_tools_fixture_matches_runtime_registration() {
    // GIVEN the native dispatcher built the way the chat runtime builds it,
    // on a throwaway sandbox (never the real ~/.apollia)
    let sandbox = tempfile::tempdir().expect("tempdir");
    let cfg = NativeDispatcherConfig {
        sandbox_root: sandbox.path().to_path_buf(),
        agent_id: "apollia:test:tool-catalogue".to_string(),
        venv_base_dir: sandbox.path().join("venvs"),
        memory_namespace: Some("apollia:test:tool-catalogue".to_string()),
        memory_shared_namespaces: Vec::new(),
        memory_base_dir: sandbox.path().join("memory"),
        http_allowlist: None,
        pending_user_inputs: Some(PendingUserInputs::new()),
        disabled_tools: Vec::new(),
        brave_api_key: None,
        web_search_config: Default::default(),
        web_read_config: Default::default(),
        governance_db_path: Some(sandbox.path().join("governance.db")),
    };
    let dispatcher = build_native_dispatcher(&cfg);

    // WHEN assembling the full production surface: natives, the chat-mode
    // synthetic tools (todo, plan), and the MCP synthetics
    let mut expected: BTreeSet<String> = dispatcher
        .tool_names()
        .into_iter()
        .map(str::to_string)
        .collect();
    for name in GATED_NATIVES {
        expected.insert((*name).to_string());
    }
    for name in [
        TODO_WRITE_TOOL_NAME,
        PLAN_PROPOSE_TOOL_NAME,
        PLAN_ADD_STEP_TOOL_NAME,
        PLAN_MODIFY_STEP_TOOL_NAME,
        PLAN_REMOVE_STEP_TOOL_NAME,
        PLAN_REORDER_TOOL_NAME,
        PLAN_SET_STEP_STATUS_TOOL_NAME,
        PLAN_SUBMIT_TOOL_NAME,
        TOOL_SEARCH,
        MCP_RESOURCES_LIST,
        MCP_RESOURCES_READ,
    ] {
        expected.insert(name.to_string());
    }

    // THEN the checked-in fixture carries exactly that list
    let fixture_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/src/lib/i18n/production-tools.json");
    let raw = std::fs::read_to_string(&fixture_path).expect("read production-tools.json");
    let fixture: Vec<String> = serde_json::from_str(&raw).expect("parse production-tools.json");
    let fixture_set: BTreeSet<String> = fixture.iter().cloned().collect();

    assert_eq!(
        fixture.len(),
        fixture_set.len(),
        "production-tools.json carries a duplicate entry"
    );
    let missing: Vec<&String> = expected.difference(&fixture_set).collect();
    let stale: Vec<&String> = fixture_set.difference(&expected).collect();
    assert!(
        missing.is_empty() && stale.is_empty(),
        "production-tools.json drifted from the runtime registration.\n\
         missing from fixture: {missing:?}\nstale in fixture: {stale:?}\n\
         Update the fixture, then add tools.labels.* and tools.descriptions.* \
         for any new name in en.json and fr.json."
    );
}
