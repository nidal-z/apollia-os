//! `apollia-os agent uninstall` and `apollia-os agent package uninstall`.
//!
//! Both go through the runtime, which removes the agents from its registry,
//! drops their triggers and records the uninstallation in the audit journal.
//! Without a runtime nothing is removed, exit 2. Apollia is refused before the
//! runtime is reached.

use serde_json::Value;

use crate::client::RuntimeClient;
use crate::exit_codes;
use crate::output::{emit_client_error, emit_error, require_confirmation};

/// What the user chose to erase rather than keep.
#[derive(Debug, Clone, Copy)]
pub(in crate::commands::agent) struct Erase {
    pub(in crate::commands::agent) memory: bool,
    pub(in crate::commands::agent) history: bool,
}

impl Erase {
    fn describe(self) -> &'static str {
        match (self.memory, self.history) {
            (false, false) => "memory and history kept",
            (true, false) => "memory erased, history kept",
            (false, true) => "memory kept, history erased",
            (true, true) => "memory and history erased",
        }
    }
}

/// `apollia-os agent uninstall <name>`: uninstall an agent installed on its
/// own.
pub(in crate::commands::agent) async fn run_uninstall(
    client: &RuntimeClient,
    name: &str,
    erase: Erase,
    confirm: bool,
    json: bool,
) -> i32 {
    // Apollia is part of the product: refused before anything is asked.
    if apollia_tools::is_built_in_agent(name) {
        let refusal = apollia_tools::AgentRepositoryError::BuiltIn(name.to_owned());
        return emit_error(json, exit_codes::GENERAL_ERROR, &refusal.to_string());
    }
    if let Some(code) = require_confirmation(
        confirm,
        json,
        &format!("uninstall agent '{name}' ({})", erase.describe()),
    ) {
        return code;
    }
    match client
        .uninstall_agent(name, erase.memory, erase.history)
        .await
    {
        Ok(report) => print_report(&report, json),
        Err(e) => emit_client_error(json, &e),
    }
}

/// `apollia-os agent package uninstall <name>`: uninstall a package and
/// everything it installed.
pub(in crate::commands::agent) async fn run_package_uninstall(
    client: &RuntimeClient,
    name: &str,
    erase: Erase,
    confirm: bool,
    json: bool,
) -> i32 {
    if let Some(code) = require_confirmation(
        confirm,
        json,
        &format!("uninstall package '{name}' ({})", erase.describe()),
    ) {
        return code;
    }
    match client
        .uninstall_package(name, erase.memory, erase.history)
        .await
    {
        Ok(report) => print_report(&report, json),
        Err(e) => emit_client_error(json, &e),
    }
}

fn print_report(report: &Value, json: bool) -> i32 {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(report).unwrap_or_default()
        );
        return exit_codes::SUCCESS;
    }
    let text = |key: &str| report.get(key).and_then(Value::as_str).unwrap_or_default();
    let list = |key: &str| -> Vec<String> {
        report
            .get(key)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    let flag = |key: &str| report.get(key).and_then(Value::as_bool).unwrap_or(false);
    println!("Uninstalled {} {}", text("name"), text("version"));
    for (label, key) in [
        ("Agents", "agents"),
        ("Workflows", "workflows"),
        ("Tools", "tools"),
        ("Triggers removed", "triggersRemoved"),
        ("MCP servers removed", "mcpServersRemoved"),
        ("Connections kept", "connectionsKept"),
    ] {
        let items = list(key);
        if !items.is_empty() {
            println!("  {label}: {}", items.join(", "));
        }
    }
    println!(
        "  Memory: {}",
        if flag("memoryCleared") {
            "erased"
        } else {
            "kept"
        }
    );
    println!(
        "  History: {}",
        if flag("historyCleared") {
            "erased"
        } else {
            "kept"
        }
    );
    for leftover in list("leftovers") {
        println!("  Not removed: {leftover}");
    }
    exit_codes::SUCCESS
}
