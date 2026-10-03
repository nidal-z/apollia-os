//! The shared validation of `apollia.toml`: strict at every level, faults at
//! their line, section bounds checked, withdrawn keys accepted.

use super::{validate, ConfigDiagnostic};

const BACKEND: &str = "[llm]\ndefault = \"cloud\"\n\n[[llm.backends]]\nname = \"cloud\"\ntype = \"api\"\napi_url = \"https://api.example.org/v1\"\napi_key_env = \"EXAMPLE_KEY\"\nmodel = \"m\"\n";

/// Validates `text` and returns its single diagnostic.
fn single_fault(text: &str) -> ConfigDiagnostic {
    let diagnostics = validate(text).expect_err("the document must be refused");
    assert_eq!(diagnostics.len(), 1, "one fault expected: {diagnostics:?}");
    diagnostics.into_iter().next().unwrap()
}

/// The fault of a document refused for one unknown key.
fn unknown(text: &str) -> ConfigDiagnostic {
    let fault = single_fault(text);
    assert!(fault.message.contains("unknown"), "{fault}");
    fault
}

#[test]
fn an_unknown_key_at_the_root_is_refused() {
    // GIVEN a key outside any section
    let text = "typo = 1\n";
    // WHEN the document is validated
    // THEN it is refused, at the key
    let fault = unknown(text);
    assert_eq!((fault.path.as_str(), fault.line), ("typo", 1), "{fault}");
}

#[test]
fn an_unknown_section_is_refused() {
    // GIVEN a section nothing reads and the registry does not withdraw
    let text = "[nonsense]\nx = 1\n";
    // WHEN the document is validated
    let fault = single_fault(text);
    // THEN it is refused, naming the section
    assert!(fault.path.starts_with("nonsense"), "{fault}");
    assert!(fault.message.contains("unknown"), "{fault}");
}

#[test]
fn an_unknown_key_in_api_is_refused() {
    // GIVEN `api.typo`, the case that used to be accepted then lost
    let text = "[api]\nbind = \"127.0.0.1\"\ntypo = 1\n";
    // WHEN the document is validated
    // THEN it is refused, at line 3
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("api.typo", 3),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_runtime_is_refused() {
    // GIVEN a typo in [runtime]
    let text = "[runtime]\neventbus_capacityy = 1024\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("runtime.eventbus_capacityy", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_hitl_is_refused() {
    // GIVEN a typo in [hitl]
    let text = "[hitl]\ntimeout = 3\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("hitl.timeout", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_tools_is_refused() {
    // GIVEN a typo in [tools]
    let text = "[tools]\ndisable = []\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("tools.disable", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_tools_web_search_is_refused() {
    // GIVEN a typo in [tools.web_search]
    let text = "[tools.web_search]\nengine = \"brave\"\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("tools.web_search.engine", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_tools_web_search_brave_is_refused() {
    // GIVEN a typo in [tools.web_search.brave]
    let text = "[tools.web_search.brave]\nkey = \"x\"\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("tools.web_search.brave.key", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_tools_web_search_duckduckgo_is_refused() {
    // GIVEN a typo in [tools.web_search.duckduckgo]
    let text = "[tools.web_search.duckduckgo]\nregion = \"fr\"\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("tools.web_search.duckduckgo.region", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_tools_web_read_is_refused() {
    // GIVEN a typo in [tools.web_read]
    let text = "[tools.web_read]\nssrf = false\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("tools.web_read.ssrf", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_mcp_is_refused() {
    // GIVEN a typo in [mcp]
    let text = "[mcp]\ntool_loader = \"eager\"\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("mcp.tool_loader", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_hooks_is_refused() {
    // GIVEN a typo in [hooks]
    let text = "[hooks]\nhandler = []\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("hooks.handler", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_a_hook_handler_is_refused() {
    // GIVEN a typo inside one [[hooks.handlers]] entry
    let text = "[[hooks.handlers]]\nevents = [\"pre_tool_use\"]\ntype = \"http\"\nurl = \"http://127.0.0.1:9000/hook\"\ntimeout = 100\n";
    // WHEN the document is validated
    let fault = single_fault(text);
    // THEN it is refused, inside the handler table
    assert!(fault.path.starts_with("hooks.handlers"), "{fault}");
    assert!(fault.message.contains("unknown"), "{fault}");
}

#[test]
fn an_unknown_key_in_chat_is_refused() {
    // GIVEN a typo in [chat]
    let text = "[chat]\nplan_mode = true\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("chat.plan_mode", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_filesystem_is_refused() {
    // GIVEN a typo in [filesystem]
    let text = "[filesystem]\ntrusted = []\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("filesystem.trusted", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_filesystem_journal_is_refused() {
    // GIVEN a typo in [filesystem.journal]
    let text = "[filesystem.journal]\nsessions = 3\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("filesystem.journal.sessions", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_models_is_refused() {
    // GIVEN a typo in [models]
    let text = "[models]\nregistry = \"x\"\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("models.registry", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_models_import_is_refused() {
    // GIVEN a typo in [models.import]
    let text = "[models.import]\nverified = true\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("models.import.verified", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_workflows_is_refused() {
    // GIVEN a typo in [workflows]
    let text = "[workflows]\nsteps = 3\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("workflows.steps", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_desktop_is_refused() {
    // GIVEN a typo in [desktop]
    let text = "[desktop]\ndevelopper_mode = true\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("desktop.developper_mode", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_observability_is_refused() {
    // GIVEN a typo in [observability]
    let text = "[observability]\nretention = 3\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("observability.retention", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_ui_is_refused() {
    // GIVEN a typo in [ui]
    let text = "[ui]\nlink_preview = true\n";
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("ui.link_preview", 2),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_llm_is_refused() {
    // GIVEN a typo in [llm]
    let text = BACKEND.replacen(
        "default = \"cloud\"\n",
        "default = \"cloud\"\ndefaults = 1\n",
        1,
    );
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(&text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("llm.defaults", 3),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_an_llm_backend_is_refused() {
    // GIVEN a typo inside one [[llm.backends]] entry
    let text = format!("{BACKEND}api_keyenv = \"X\"\n");
    // WHEN the document is validated
    let fault = single_fault(&text);
    // THEN it is refused, inside the backend table
    assert!(fault.path.starts_with("llm.backends"), "{fault}");
    assert!(fault.message.contains("unknown"), "{fault}");
}

#[test]
fn an_unknown_key_in_llm_observability_is_refused() {
    // GIVEN a typo in [llm.observability]
    let text = format!("{BACKEND}\n[llm.observability]\nlog_tokens = true\n");
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(&text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("llm.observability.log_tokens", 12),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_llm_routing_is_refused() {
    // GIVEN a typo in [llm.routing]
    let text = format!(
        "{BACKEND}\n[llm.routing]\nprecise = \"cloud\"\nfast = \"cloud\"\nslow = \"cloud\"\n"
    );
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(&text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("llm.routing.slow", 14),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_llm_routing_hybrid_is_refused() {
    // GIVEN a typo in [llm.routing.hybrid]
    let text = format!(
        "{BACKEND}\n[llm.routing]\nprecise = \"cloud\"\nfast = \"cloud\"\n\n[llm.routing.hybrid]\nfrontier = \"cloud\"\ncost_ceiling_usd = 1.0\nceiling = \"hard_stop\"\n"
    );
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(&text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("llm.routing.hybrid.ceiling", 18),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_llm_runner_is_refused() {
    // GIVEN a typo in [llm.runner]
    let text = format!("{BACKEND}\n[llm.runner]\nback_end = \"cpu\"\n");
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(&text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("llm.runner.back_end", 12),
        "{fault}"
    );
}

#[test]
fn an_unknown_key_in_llm_guardrails_is_refused() {
    // GIVEN a typo in [llm.guardrails]
    let text = format!("{BACKEND}\n[llm.guardrails]\nlevl = \"strict\"\n");
    // WHEN the document is validated
    // THEN it is refused, at its line
    let fault = unknown(&text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("llm.guardrails.levl", 12),
        "{fault}"
    );
}

#[test]
fn the_guardrails_section_is_read_and_its_level_left_to_the_runtime() {
    // GIVEN [llm.guardrails] with a ceiling, and one with a level nobody knows
    let ceiling = format!("{BACKEND}\n[llm.guardrails]\nlevel = \"ceiling\"\nceiling_gb = 12.0\n");
    let unknown_level = format!("{BACKEND}\n[llm.guardrails]\nlevel = \"strcit\"\n");
    // WHEN both are validated
    let read = validate(&ceiling).expect("a guardrails section is accepted");
    let lenient = validate(&unknown_level);
    // THEN the first is carried by the root, and the second is accepted: the
    // runtime holds an unknown level to balanced
    let guardrails = read.llm.expect("[llm] present").guardrails;
    assert_eq!(guardrails.level, "ceiling");
    assert_eq!(guardrails.ceiling_gb, Some(12.0));
    assert!(lenient.is_ok(), "{lenient:?}");
}

#[test]
fn an_unknown_key_in_server_is_refused() {
    // GIVEN a typo in [server]
    let text = "[server]\nenabled = true\nrequire_auht = true\n";
    // WHEN the document is validated
    // THEN it is refused, at line 3
    let fault = unknown(text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("server.require_auht", 3),
        "{fault}"
    );
}

#[test]
fn the_server_section_and_the_mcp_bridge_switch_are_read() {
    // GIVEN the compatible server and the MCP bridge turned on
    let text = "[server]\nenabled = true\nport = 8123\nrequire_auth = true\n\n[mcp]\nserver_enabled = true\n";
    // WHEN the document is validated
    let config = validate(text).expect("both sections are accepted");
    // THEN the root carries every value
    assert!(config.server.enabled);
    assert_eq!(config.server.port, 8123);
    assert!(config.server.require_auth);
    assert!(config.mcp.server_enabled);
}

#[test]
fn an_unknown_key_in_a_pricing_override_is_refused() {
    // GIVEN a typo inside one [llm.pricing_overrides] entry
    let text = format!(
        "{BACKEND}\n[llm.pricing_overrides.m]\ninput_per_mtok = 1.0\noutput_per_mtok = 2.0\ncached = 0.5\n"
    );
    // WHEN the document is validated
    // THEN it is refused
    let fault = unknown(&text);
    assert_eq!(
        (fault.path.as_str(), fault.line),
        ("llm.pricing_overrides.m.cached", 14),
        "{fault}"
    );
}

#[test]
fn a_wrong_type_on_line_12_is_reported_on_line_12() {
    // GIVEN a document whose twelfth line gives a string where a number goes
    let text = "# Apollia\n\n[api]\nbind = \"127.0.0.1\"\n\n[chat]\nplan_mode_default = true\n\n# runtime\n\n[runtime]\neventbus_capacity = \"large\"\n";
    assert_eq!(text.lines().nth(11), Some("eventbus_capacity = \"large\""));

    // WHEN the document is validated
    let fault = single_fault(text);

    // THEN the diagnostic is on line 12, at the key, with the parser's message
    assert_eq!(fault.line, 12, "{fault}");
    assert_eq!(fault.path, "runtime.eventbus_capacity", "{fault}");
    assert!(fault.message.contains("invalid type"), "{fault}");
}

#[test]
fn a_syntax_error_is_reported_at_its_line() {
    // GIVEN a document whose third line is not TOML
    let text = "[chat]\nplan_mode_default = true\nthis is not toml\n";

    // WHEN the document is validated
    let fault = single_fault(text);

    // THEN the diagnostic is on line 3
    assert_eq!(fault.line, 3, "{fault}");
}

#[test]
fn an_api_section_out_of_its_rules_is_refused_by_its_own_validate() {
    // GIVEN a TLS certificate without its key
    let text = "[api]\nbind = \"127.0.0.1\"\ntls_cert = \"/etc/cert.pem\"\n";

    // WHEN the document is validated
    let fault = single_fault(text);

    // THEN ApiConfig::validate refuses it, placed at the section
    assert_eq!(fault.path, "api.tls", "{fault}");
    assert_eq!(fault.line, 1, "{fault}");
    assert!(fault.message.contains("tls_cert and tls_key"), "{fault}");
}

#[test]
fn a_runtime_section_out_of_its_bounds_is_refused_by_its_own_validate() {
    // GIVEN an event bus smaller than its lower bound
    let text = "[runtime]\nmailbox_capacity = 100\neventbus_capacity = 8\n";

    // WHEN the document is validated
    let fault = single_fault(text);

    // THEN RuntimeConfig::validate refuses it, at the key
    assert_eq!(fault.path, "runtime.eventbus_capacity", "{fault}");
    assert_eq!(fault.line, 3, "{fault}");
    assert!(fault.message.contains("out of bounds"), "{fault}");
}

#[test]
fn a_hitl_section_out_of_its_bounds_is_refused_by_its_own_validate() {
    // GIVEN a scan interval below its lower bound
    let text = "[hitl]\nscan_interval_secs = 1\n";

    // WHEN the document is validated
    let fault = single_fault(text);

    // THEN HitlConfig::validate refuses it, at the key
    assert_eq!(fault.path, "hitl.scan_interval_secs", "{fault}");
    assert_eq!(fault.line, 2, "{fault}");
}

#[test]
fn faults_in_several_sections_are_all_reported() {
    // GIVEN two sections out of bounds
    let text = "[hitl]\nscan_interval_secs = 1\n\n[runtime]\neventbus_capacity = 8\n";

    // WHEN the document is validated
    let diagnostics = validate(text).expect_err("two faults");

    // THEN both are reported
    let paths: Vec<_> = diagnostics.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["runtime.eventbus_capacity", "hitl.scan_interval_secs"]
    );
}

#[test]
fn an_empty_document_gives_the_defaults() {
    // GIVEN an empty file
    // WHEN it is validated
    let config = validate("").expect("an empty file is valid");

    // THEN every section has its default and [llm] is absent
    assert!(config.llm.is_none());
    assert_eq!(config.api.port, 7771);
    assert!(!config.desktop.developer_mode);
}

#[test]
fn withdrawn_keys_are_accepted_and_the_rest_is_read() {
    // GIVEN a file written for an earlier version: inert and deprecated
    //       sections, the retired [llm.vertex], and live keys around them
    let text = format!(
        "[memory]\nenabled = true\n\n[[triggers]]\nname = \"t\"\n\n[chat]\nplan_mode_default = true\n\n{BACKEND}\n[llm.vertex]\nproject_id = \"p\"\n\n[oria]\nmax_replans = 3\n"
    );

    // WHEN the document is validated
    let config = validate(&text).expect("withdrawn keys never fail a file");

    // THEN the live keys are read
    assert!(config.chat.plan_mode_default);
    assert_eq!(config.llm.map(|l| l.default), Some("cloud".to_owned()));
}

#[test]
fn a_fault_after_a_withdrawn_section_keeps_its_line() {
    // GIVEN a withdrawn section above a fault
    let text = "[memory]\nenabled = true\nsize = 3\n\n[api]\ntypo = 1\n";

    // WHEN the document is validated
    let fault = single_fault(text);

    // THEN the fault is still placed on its own line
    assert_eq!(fault.path, "api.typo", "{fault}");
    assert_eq!(fault.line, 6, "{fault}");
}

#[test]
fn the_backend_mirror_of_the_desktop_reads_back() {
    // GIVEN a backend block as the desktop mirror writes it, provider included
    let text = format!("{BACKEND}provider = \"openai\"\n");

    // WHEN the document is validated
    let config = validate(&text).expect("the mirror's own output must read back");

    // THEN the backend is there
    assert_eq!(config.llm.map(|l| l.backends.len()), Some(1));
}

#[test]
fn a_socket_the_file_names_must_have_its_folder() {
    // GIVEN a socket in a folder that does not exist
    let text = "[api]\nunix_socket = \"/nonexistent-apollia-dir/runtime.sock\"\n";

    // WHEN the document is validated
    let fault = single_fault(text);

    // THEN ApiConfig::validate refuses it, at the key
    assert_eq!(fault.path, "api.unix_socket", "{fault}");
    assert_eq!(fault.line, 2, "{fault}");
}
