use super::*;

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
        println!("  Agents ({}):", agents.len());
        for a in &agents {
            println!("    - {a}");
        }
    }
    exit_codes::SUCCESS
}

/// `apollia-os agent package uninstall <name>`: remove package, all its agents and triggers.
pub(in crate::commands::agent) async fn run_package_uninstall(name: &str, json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let pkg_repo = match open_package_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    let pkg = match pkg_repo.get(name) {
        Ok(Some(p)) => p,
        Ok(None) => return print_error_and_exit(&format!("Package '{name}' not found"), json),
        Err(e) => return print_error_and_exit(&format!("database error: {e}"), json),
    };
    let agent_names = pkg_repo.list_agents_for_package(name).unwrap_or_default();

    // Delete agents from AgentRepository.
    let agent_repo = match open_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };
    for agent_name in &agent_names {
        let _ = agent_repo.delete(agent_name);
    }

    // Delete package from PackageRepository (cascades package_agents).
    if let Err(e) = pkg_repo.delete(name) {
        return print_error_and_exit(&format!("failed to delete package: {e}"), json);
    }

    // Remove install directory (best-effort).
    let _ = std::fs::remove_dir_all(&pkg.root_path);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "name": name,
                "status": "uninstalled",
                "agents_removed": agent_names.len(),
            }))
            .unwrap_or_default()
        );
    } else {
        println!(
            "Package '{name}' uninstalled ({} agents removed)",
            agent_names.len()
        );
    }
    exit_codes::SUCCESS
}

// ─── Trigger injection ───────────────────────────────────────────────────────

/// `apollia-os agent uninstall <name>`: remove an installed agent.
pub(in crate::commands::agent) fn run_uninstall(name: &str, json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let repo = match open_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    // Check agent exists.
    match repo.get(name) {
        Ok(Some(_)) => {}
        Ok(None) => {
            return print_error_and_exit(
                &format!("Agent '{name}' not found in installed agents"),
                json,
            );
        }
        Err(e) => return print_error_and_exit(&format!("database error: {e}"), json),
    }

    // Delete from repository.
    if let Err(e) = repo.delete(name) {
        return print_error_and_exit(&format!("failed to delete from database: {e}"), json);
    }

    // Remove install directory (best-effort, ignore errors).
    let agents_dir = data_dir.join("agents").join(name);
    let _ = std::fs::remove_dir_all(&agents_dir);

    if json {
        let output = serde_json::json!({
            "name": name,
            "status": "uninstalled",
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else {
        println!("Agent '{name}' uninstalled");
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

    // Look up install_path BEFORE flipping the flag so we can short-circuit
    // when the agent is unknown (operator typo) without persisting a stale
    // `enabled=true` row.
    let install_path = match repo.get(name) {
        Ok(Some(entry)) => entry.install_path.to_string_lossy().to_string(),
        Ok(None) => {
            return print_error_and_exit(
                &format!("Agent '{name}' not found in installed agents"),
                json,
            );
        }
        Err(e) => return print_error_and_exit(&format!("repository read failed: {e}"), json),
    };

    match repo.set_enabled(name, true) {
        Ok(()) => {
            let load_outcome = match client.start_agent(&install_path).await {
                Ok(_) => "loaded",
                Err(ClientError::ServerError { status: 409, .. }) => "already-loaded",
                Err(ClientError::ConnectionRefused) => "runtime-offline",
                Err(e) => {
                    tracing::warn!(
                        agent = %name,
                        error = %e,
                        "enable: failed to load into the live registry - agent remains enabled=true for next boot"
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
                        "disable: failed to stop live registry entry - agent remains enabled=false for next boot"
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

/// `apollia-os agent update <name> <path>`: update an installed agent.
pub(in crate::commands::agent) fn run_update(name: &str, source_path: &Path, json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let repo = match open_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    // Check the agent exists.
    let existing = match repo.get(name) {
        Ok(Some(a)) => a,
        Ok(None) => {
            return print_error_and_exit(
                &format!("Agent '{name}' not found in installed agents"),
                json,
            );
        }
        Err(e) => return print_error_and_exit(&format!("database error: {e}"), json),
    };

    // Validate source file exists.
    if !source_path.exists() {
        return print_error_and_exit(&format!("file not found: {}", source_path.display()), json);
    }

    let canonical = match source_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return print_error_and_exit(
                &format!("cannot resolve path {}: {e}", source_path.display()),
                json,
            );
        }
    };

    // Load and validate the new Python module.
    let loader = CliAgentLoader;
    let manifest = match loader.load_and_validate(&canonical) {
        Ok(m) => m,
        Err(e) => {
            return print_error_and_exit(&format!("failed to load agent: {e}"), json);
        }
    };

    // Copy new file to install location.
    if let Err(e) = std::fs::copy(&canonical, &existing.install_path) {
        return print_error_and_exit(
            &format!(
                "cannot copy {} to {}: {e}",
                canonical.display(),
                existing.install_path.display()
            ),
            json,
        );
    }

    // Update repository entry.
    let updated = InstalledAgent {
        name: existing.name.clone(),
        version: manifest.version.clone(),
        install_path: existing.install_path,
        source_path: canonical,
        manifest,
        enabled: existing.enabled,
        installed_at: existing.installed_at,
        updated_at: now_rfc3339(),
    };

    if let Err(e) = repo.save(&updated) {
        return print_error_and_exit(&format!("failed to update database: {e}"), json);
    }

    if json {
        let output = serde_json::json!({
            "name": updated.name,
            "version": updated.version,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else {
        println!("Agent '{}' updated to v{}", updated.name, updated.version);
    }

    exit_codes::SUCCESS
}

// ─────────────────────────────────────────────────────────────────────────────
// Scaffolding (agent new)
// ─────────────────────────────────────────────────────────────────────────────

/// `apollia-os agent repair <name>`: re-provision an installed agent's venv.
///
/// Looks the agent up in the package repository, parses the `manifest_json`
/// stored for its owning package, finds the matching agent entry, then runs
/// `PythonExecutor::setup_venv` against the declared `packages` list. Idempotent:
/// safe to run on already-provisioned venvs (pip is a no-op when satisfied).
pub(in crate::commands::agent) async fn run_repair(name: &str, json: bool) -> i32 {
    let data_dir = apollia_data_dir();
    let pkg_repo = match open_package_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    // Find the package owning this agent: list packages, parse each manifest,
    // and match by agent name. The repository stores the resolved manifest JSON
    // alongside the package row so we do not need to re-parse agent.toml from disk.
    let packages = match pkg_repo.list() {
        Ok(p) => p,
        Err(e) => return print_error_and_exit(&format!("database error: {e}"), json),
    };

    let (package_name, declared) = match find_agent_package_deps(&packages, name) {
        Some(f) => f,
        None => {
            return print_error_and_exit(
                &format!(
                    "agent '{name}' not found in any installed package\n\
                     Hint: run `apollia-os agent list` to see installed agents."
                ),
                json,
            );
        }
    };

    if declared.is_empty() {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "agent": name,
                    "package": package_name,
                    "venv_provisioned": false,
                    "reason": "no pip dependencies declared in manifest",
                }))
                .unwrap_or_default()
            );
        } else {
            println!("Agent '{name}' (package {package_name}) declares no pip packages.");
            println!("Nothing to repair.");
        }
        return exit_codes::SUCCESS;
    }

    let venv_base = data_dir.join("venvs");
    let executor =
        match apollia_tools::tools::python_executor::PythonExecutor::new(name, &venv_base) {
            Ok(e) => e,
            Err(e) => {
                return print_error_and_exit(&format!("could not init venv: {e}"), json);
            }
        };

    if !json {
        println!(
            "Provisioning venv for '{name}' ({} package(s))…",
            declared.len()
        );
    }

    if let Err(e) = executor.setup_venv(&declared).await {
        return print_error_and_exit(&format!("venv provisioning failed: {e}"), json);
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "agent": name,
                "package": package_name,
                "venv_provisioned": true,
                "packages": declared,
            }))
            .unwrap_or_default()
        );
    } else {
        println!(
            "OK venv for '{name}' provisioned at {}",
            venv_base.join(name).display()
        );
        println!("    Packages installed:");
        for p in &declared {
            println!("      - {p}");
        }
        println!();
        println!("Restart the runtime to pick up the new venv:");
        println!("    apollia-os stop && apollia-os start");
    }
    exit_codes::SUCCESS
}

/// Locate the package owning `agent_name` and return its `(package_name,
/// declared_pip_packages)`. Returns `None` when no installed package
/// declares an agent with that name.
fn find_agent_package_deps(
    packages: &[InstalledPackage],
    agent_name: &str,
) -> Option<(String, Vec<String>)> {
    for pkg in packages {
        let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&pkg.manifest_json) else {
            continue;
        };
        let Some(agents) = manifest.get("agents").and_then(|v| v.as_array()) else {
            continue;
        };
        let Some(entry) = agents
            .iter()
            .find(|e| e.get("name").and_then(|v| v.as_str()) == Some(agent_name))
        else {
            continue;
        };
        let pkgs: Vec<String> = entry
            .get("packages")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        return Some((pkg.name.clone(), pkgs));
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────
