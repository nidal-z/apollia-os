use super::*;
use crate::note;

/// `apollia-os agent package list`: list installed packages.
pub(in crate::commands::agent) fn run_package_list(json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let pkg_repo = match open_package_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };
    let pkgs = match pkg_repo.list() {
        Ok(p) => p,
        Err(e) => return print_error_and_exit(&format!("database error: {e}"), json),
    };
    if json {
        let items: Vec<_> = pkgs
            .iter()
            .map(|p| {
                serde_json::json!({
                    "name": p.name,
                    "version": p.version,
                    "installed_at": p.installed_at,
                    "root_path": p.root_path.to_string_lossy(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "packages": items }))
                .unwrap_or_default()
        );
    } else if pkgs.is_empty() {
        println!("No agent packages installed.");
    } else {
        println!("{:<24} {:<12} INSTALLED", "NAME", "VERSION");
        for p in &pkgs {
            println!("{:<24} {:<12} {}", p.name, p.version, p.installed_at);
        }
    }
    exit_codes::SUCCESS
}

/// `apollia-os agent package info <name>`: show package details.
pub(in crate::commands::agent) fn run_package_info(name: &str, json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let pkg_repo = match open_package_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };
    let pkg = match pkg_repo.get(name) {
        Ok(Some(p)) => p,
        Ok(None) => {
            return print_error_and_exit(&format!("Package '{name}' not found"), json);
        }
        Err(e) => return print_error_and_exit(&format!("database error: {e}"), json),
    };
    let agents = pkg_repo.list_agents_for_package(name).unwrap_or_default();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "name": pkg.name,
                "version": pkg.version,
                "installed_at": pkg.installed_at,
                "root_path": pkg.root_path.to_string_lossy(),
                "agents": agents,
                "manifest": serde_json::from_str::<serde_json::Value>(&pkg.manifest_json).unwrap_or_default(),
            }))
            .unwrap_or_default()
        );
    } else {
        println!("Package: {} v{}", pkg.name, pkg.version);
        println!("  Installed: {}", pkg.installed_at);
        println!("  Path:      {}", pkg.root_path.display());
        note!("  Agents ({}):", agents.len());
        for a in &agents {
            println!("    - {a}");
        }
    }
    exit_codes::SUCCESS
}

/// `apollia-os agent enable <name>`: re-enable an installed agent.
///
/// Two-phase, symmetric with [`run_disable`]:
/// 1. Mark `enabled = true` in `agents.db` so the agent auto-starts at the
///    next runtime boot.
/// 2. Best-effort `POST /api/v1/agents` with the persisted `install_path`
///    so the agent is loaded into the live registry immediately. Without
///    this, `apollia-os run <name>` returns 404 until the daemon is
///    restarted.
pub(in crate::commands::agent) async fn run_enable(
    client: &RuntimeClient,
    name: &str,
    json: bool,
) -> i32 {
    let data_dir = apollia_data_dir();
    let repo = match open_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    // Check the agent is stored BEFORE flipping the flag, so an operator
    // typo is refused without touching the store.
    match repo.stored(name) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return print_error_and_exit(
                &format!("Agent '{name}' not found in installed agents"),
                json,
            );
        }
        Err(e) => return print_error_and_exit(&format!("repository read failed: {e}"), json),
    }

    match repo.set_enabled(name, true) {
        Ok(()) => {
            let load_outcome = match client.start_agent(name).await {
                Ok(_) => "loaded",
                Err(ClientError::ServerError { status: 409, .. }) => "already-loaded",
                Err(ClientError::ConnectionRefused) => "runtime-offline",
                Err(e) => {
                    tracing::warn!(
                        agent = %name,
                        error = %e,
                        detail = "the agent stays enabled for the next boot",
                        "agent.enable.load.failed"
                    );
                    "load-failed"
                }
            };
            if json {
                let output = serde_json::json!({
                    "name": name,
                    "enabled": true,
                    "runtime_load": load_outcome,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                let suffix = match load_outcome {
                    "loaded" => " and loaded into the runtime (`apollia-os run` works now)",
                    "already-loaded" => " (already running in the runtime)",
                    "runtime-offline" => " - runtime offline, load will happen on next start",
                    _ => " - runtime load failed, retry with `apollia-os agent start <name>`",
                };
                println!("Agent '{name}' enabled (will auto-start on boot){suffix}");
            }
            exit_codes::SUCCESS
        }
        Err(apollia_tools::AgentRepositoryError::NotFound(_)) => print_error_and_exit(
            &format!("Agent '{name}' not found in installed agents"),
            json,
        ),
        Err(e) => print_error_and_exit(&format!("database error: {e}"), json),
    }
}

/// `apollia-os agent disable <name>`: disable an installed agent.
///
/// Two-phase operation:
/// 1. Mark the agent `enabled = false` in `agents.db` so it won't auto-start
///    at the next runtime boot.
/// 2. Best-effort `DELETE /api/v1/agents/<name>` so the live registry stops
///    holding an executor for it. This is what makes `apollia-os agent list`
///    immediately show `disabled` instead of an outdated `active` state.
///    A runtime that is offline (or has never had this agent loaded) is
///    treated as a no-op for phase 2: the persisted disable is what
///    matters across reboots.
pub(in crate::commands::agent) async fn run_disable(
    client: &RuntimeClient,
    name: &str,
    json: bool,
) -> i32 {
    let data_dir = apollia_data_dir();
    let repo = match open_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    match repo.set_enabled(name, false) {
        Ok(()) => {
            let stop_outcome = match client.stop_agent(name).await {
                Ok(_) => "stopped",
                Err(ClientError::ServerError { status: 404, .. }) => "not-loaded",
                Err(ClientError::ConnectionRefused) => "runtime-offline",
                Err(e) => {
                    tracing::warn!(
                        agent = %name,
                        error = %e,
                        detail = "the agent stays disabled for the next boot",
                        "agent.disable.stop.failed"
                    );
                    "stop-failed"
                }
            };
            if json {
                let output = serde_json::json!({
                    "name": name,
                    "enabled": false,
                    "runtime_stop": stop_outcome,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                );
            } else {
                let suffix = match stop_outcome {
                    "stopped" => " and unloaded from the runtime",
                    "not-loaded" => " (was not loaded in the runtime)",
                    "runtime-offline" => " - runtime offline, change takes effect on next start",
                    _ => "",
                };
                println!("Agent '{name}' disabled (will not auto-start){suffix}");
            }
            exit_codes::SUCCESS
        }
        Err(apollia_tools::AgentRepositoryError::NotFound(_)) => print_error_and_exit(
            &format!("Agent '{name}' not found in installed agents"),
            json,
        ),
        Err(e) => print_error_and_exit(&format!("database error: {e}"), json),
    }
}
