#![allow(clippy::unwrap_used, clippy::expect_used)]
//! An agent never grants itself a permission.
//!
//! Permission rules are the user's: they are written from the desktop or with
//! `apollia-os permissions`, never by a tool an agent can call. A native tool
//! that read or wrote them would let a third-party agent persist its own
//! `allow` on `bash_executor`, with no human in the way on the task path.
//!
//! The guard has two halves. The first refuses a native tool whose name opens
//! the `permission_` family, at the two places a native tool is named: the
//! catalogue the registry lists, and the dispatcher built with every gated
//! tool switched on. The second refuses a native tool that is handed the
//! governance database: the tool modules and the dispatcher factory must not
//! name it. The tool registry itself still opens that database for the tool
//! states and the encrypted credentials; the guard is on the tools, not on the
//! registry.

use std::path::{Path, PathBuf};

use apollia_core::{WebReadConfig, WebSearchConfig};
use apollia_tools::tools::ask_user::PendingUserInputs;
use apollia_tools::{build_native_dispatcher, NativeDispatcherConfig, NATIVE_TOOL_NAMES};

/// The name family reserved for tools that would act on permission rules.
const FORBIDDEN_PREFIX: &str = "permission_";

/// What a tool module would have to name to reach the permission rules.
const GOVERNANCE_MARKERS: &[&str] = &[
    "GOVERNANCE_DB_FILENAME",
    "governance.db",
    "governance_db",
    "GovernanceDb",
    "permission_rules",
    "PrefixRuleEngine",
];

fn widest_config(root: &Path) -> NativeDispatcherConfig {
    NativeDispatcherConfig {
        sandbox_roots: vec![root.to_path_buf()],
        write_roots: vec![root.to_path_buf()],
        paused_folders: Vec::new(),
        agent_id: "third-party-agent".to_string(),
        venv_base_dir: root.join("venvs"),
        memory_namespace: Some("third-party-agent".to_string()),
        memory_shared_namespaces: Vec::new(),
        memory_base_dir: root.join("memory"),
        http_allowlist: Some(vec!["example.com".to_string()]),
        pending_user_inputs: Some(PendingUserInputs::new()),
        session_id: Some("s1".to_string()),
        disabled_tools: Vec::new(),
        brave_api_key: None,
        web_search_config: WebSearchConfig::default(),
        web_read_config: WebReadConfig::default(),
        python_interpreter: None,
        max_output_chars: apollia_tools::executor::DEFAULT_MAX_OUTPUT_CHARS,
        sources: apollia_tools::SourceAccess::memory_only(true),
    }
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read tools dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_native_tool_name_opens_the_permission_family() {
    // GIVEN the catalogue the registry lists and a dispatcher built with every
    // optional tool switched on, the widest surface an agent can be handed
    let root = tempfile::tempdir().expect("tempdir");
    let dispatcher = build_native_dispatcher(&widest_config(root.path()));

    // WHEN every name either of them carries is collected
    let mut names: Vec<String> = NATIVE_TOOL_NAMES.iter().map(|n| n.to_string()).collect();
    names.extend(dispatcher.tool_names().into_iter().map(str::to_string));

    // THEN none opens the permission family: no agent holds a tool that acts
    // on permission rules
    let offending: Vec<&String> = names
        .iter()
        .filter(|n| n.starts_with(FORBIDDEN_PREFIX))
        .collect();
    assert!(
        offending.is_empty(),
        "native tools named in the permission family: {offending:?}; \
         permission rules are written by the user, never by an agent"
    );
}

#[test]
fn no_native_tool_is_handed_the_governance_database() {
    // GIVEN every source file of the native tools and of their factory
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut sources = Vec::new();
    rust_sources(&crate_root.join("src/tools"), &mut sources);
    sources.push(crate_root.join("src/native_dispatcher.rs"));
    assert!(sources.len() > 1, "the tool sources were found");

    // WHEN each is searched for what reaching the permission rules requires
    let mut hits = Vec::new();
    for path in &sources {
        let text = std::fs::read_to_string(path).expect("read source");
        for marker in GOVERNANCE_MARKERS {
            if text.contains(marker) {
                hits.push(format!("{}: {marker}", path.display()));
            }
        }
    }

    // THEN none names it: no native tool can be given the governance database
    // to write rules into
    assert!(
        hits.is_empty(),
        "native tools reach the governance database: {hits:?}"
    );
}
