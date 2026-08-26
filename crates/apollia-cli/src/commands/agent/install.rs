use super::*;

/// `apollia-os agent install <source> [--skip-tests]`: install an agent permanently.
///
/// `source` may be a local Python file path or a Git URL with an optional
/// `#<tag>` fragment.
///
/// **Local path**: performs AIP duck-typing validation (PyO3), manifest
/// conformance check, an optional pytest run, and copies the file into
/// `~/.apollia/agents/<name>/`.
///
/// **Git URL**: checks that `git` is available, clones the repository
/// (depth 1), performs AIP duck-typing validation on the discovered `.py`
/// file, installs to `~/.apollia/agents/community/<name>/`, and writes
/// `registry.json`.
pub(in crate::commands::agent) async fn run_install(
    source_arg: &str,
    client: &RuntimeClient,
    json: bool,
    skip_tests: bool,
) -> i32 {
    print_trust_banner(json);
    match parse_install_source(source_arg) {
        AgentInstallSource::Git { url, tag } => {
            run_install_git(&url, tag.as_deref(), client, json, skip_tests).await
        }
        AgentInstallSource::Local { path } => {
            run_install_local(&path, client, json, skip_tests).await
        }
    }
}

/// Install a community agent from a Git remote URL.
async fn run_install_git(
    url: &str,
    tag: Option<&str>,
    client: &RuntimeClient,
    json: bool,
    skip_tests: bool,
) -> i32 {
    // Verify git is reachable before doing any work.
    if let Err(e) = registry_remote::check_git_available() {
        return print_error_and_exit(&e.to_string(), json);
    }

    // Create a temporary clone directory, removed when `temp` is dropped.
    let temp = match registry_remote::TempInstallDir::new() {
        Ok(t) => t,
        Err(e) => return print_error_and_exit(&format!("cannot create temp dir: {e}"), json),
    };

    // Clone the repository.
    if let Err(e) = registry_remote::git_clone(url, tag, temp.path()).await {
        return print_error_and_exit(&e.to_string(), json);
    }

    // Find the Python agent file in the clone root.
    let agent_py = match registry_remote::find_agent_file(temp.path()) {
        Some(p) => p,
        None => {
            return print_error_and_exit("no Python agent file found in the repository root", json);
        }
    };

    // AIP duck-typing validation (PyO3).
    let manifest = match validate_community_agent(&agent_py, skip_tests).await {
        Ok(m) => m,
        Err(AgentValidationError::FileNotFound(_)) => {
            return print_error_and_exit(
                &format!("agent file not found: {}", agent_py.display()),
                json,
            );
        }
        Err(e) => return print_error_and_exit(&format!("agent validation failed: {e}"), json),
    };

    if manifest.dangerous_tools_allowed {
        eprintln!(
            "Warning: community agent '{}' requests dangerous_tools_allowed - user approval required",
            manifest.name
        );
    }

    // Install files, pip packages, and update registry.json.
    let data_dir = apollia_data_dir();
    let source = AgentInstallSource::Git {
        url: url.to_string(),
        tag: tag.map(|t| t.to_string()),
    };
    let entry = match registry_remote::install_from_dir(temp.path(), &manifest, source, &data_dir) {
        Ok(e) => e,
        Err(e) => return print_error_and_exit(&format!("installation failed: {e}"), json),
    };

    // Check if runtime is running (informational only).
    if client.list_agents().await.is_err() {
        eprintln!("Info: Runtime not running - agent will auto-start on next boot");
    }

    if json {
        let install_path = data_dir.join("agents").join("community").join(&entry.name);
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "name": entry.name,
                "version": entry.version,
                "install_path": install_path.to_string_lossy(),
                "source": "git",
            }))
            .unwrap_or_default()
        );
    } else {
        println!(
            "Agent '{}' v{} installed from Git successfully",
            entry.name, entry.version,
        );
    }

    exit_codes::SUCCESS
}

/// Install a community agent from a local Python file or an agent package directory.
async fn run_install_local(
    source_path: &Path,
    client: &RuntimeClient,
    json: bool,
    skip_tests: bool,
) -> i32 {
    // Detect package directory (has agent.toml at root).
    if source_path.is_dir() {
        if source_path.join("agent.toml").exists() {
            return run_install_package(source_path, client, json, skip_tests).await;
        }
        return print_error_and_exit(
            &format!(
                "directory '{}' has no agent.toml - not a valid agent package",
                source_path.display()
            ),
            json,
        );
    }

    // Validate the agent and load its manifest.
    let manifest = match validate_community_agent(source_path, skip_tests).await {
        Ok(m) => m,
        Err(AgentValidationError::FileNotFound(_)) => {
            return print_error_and_exit(
                &format!("file not found: {}", source_path.display()),
                json,
            );
        }
        Err(e) => {
            return print_error_and_exit(&format!("agent validation failed: {e}"), json);
        }
    };

    // Surface security and skip-tests warnings to the operator.
    if manifest.dangerous_tools_allowed {
        eprintln!(
            "Warning: community agent '{}' requests dangerous_tools_allowed - user approval required",
            manifest.name
        );
    }
    if skip_tests {
        eprintln!(
            "Warning: installing '{}' without running its test suite - not recommended",
            manifest.name
        );
    }

    // Canonicalize the source path for reliable storage.
    let canonical = match source_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return print_error_and_exit(
                &format!("cannot resolve path {}: {e}", source_path.display()),
                json,
            );
        }
    };

    let data_dir = apollia_data_dir();
    let agents_dir = data_dir.join("agents").join(&manifest.name);

    // Create install directory.
    if let Err(e) = std::fs::create_dir_all(&agents_dir) {
        return print_error_and_exit(
            &format!("cannot create directory {}: {e}", agents_dir.display()),
            json,
        );
    }

    // Copy Python file to install location.
    let install_path = agents_dir.join("agent.py");
    if let Err(e) = std::fs::copy(&canonical, &install_path) {
        return print_error_and_exit(
            &format!(
                "cannot copy {} to {}: {e}",
                canonical.display(),
                install_path.display()
            ),
            json,
        );
    }

    // Persist in AgentRepository.
    let repo = match open_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    let now = now_rfc3339();
    let agent = InstalledAgent {
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        install_path: install_path.clone(),
        source_path: canonical,
        manifest,
        enabled: true,
        installed_at: now.clone(),
        updated_at: now,
    };

    if let Err(e) = repo.save(&agent) {
        return print_error_and_exit(&format!("failed to save to database: {e}"), json);
    }

    // Check if runtime is running (informational only).
    let runtime_running = client.list_agents().await.is_ok();
    if !runtime_running {
        eprintln!("Info: Runtime not running - agent will auto-start on next boot");
    }

    if json {
        let output = serde_json::json!({
            "name": agent.name,
            "version": agent.version,
            "install_path": install_path.to_string_lossy(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else {
        println!(
            "Agent '{}' v{} installed successfully",
            agent.name, agent.version,
        );
    }

    exit_codes::SUCCESS
}

// ─────────────────────────────────────────────────────────────────────────────
// Package commands
// ─────────────────────────────────────────────────────────────────────────────

/// `apollia-os agent install <dir>`: install a multi-agent package from a folder.
///
/// 1. Validates `agent.toml` + duck-types every `.py`
/// 2. Copies the folder to `~/.apollia/agents/packages/<name>/`
/// 3. Registers each agent in `AgentRepository`
/// 4. Registers the package in `PackageRepository`
/// 5. Injects triggers into `TriggerDefinitionRepository`
async fn run_install_package(
    source_path: &Path,
    client: &RuntimeClient,
    json: bool,
    skip_tests: bool,
) -> i32 {
    // Step 1: parse the manifest structure and verify each `.py` exists on
    // disk. Strict duck-typing (`load_package`) is deferred to the per-agent
    // loop below so a single broken worker (e.g. failing top-level Python
    // import) doesn't tank the entire package install: partial installs
    // mark the failing agents as DEGRADED and report a summary at the end.
    let pkg = match load_manifest_only(source_path) {
        Ok(p) => p,
        Err(PackageLoaderError::ManifestNotFound(_)) => {
            return print_error_and_exit("agent.toml not found in directory", json);
        }
        Err(e) => return print_error_and_exit(&format!("package validation failed: {e}"), json),
    };

    let pkg_name = pkg.manifest.package.name.clone();
    let pkg_version = pkg.manifest.package.version.clone();

    let data_dir = apollia_data_dir();
    let install_root = data_dir.join("agents").join("packages").join(&pkg_name);

    // Step 2: copy directory to install location.
    if let Err(e) = copy_dir_all(source_path, &install_root) {
        return print_error_and_exit(
            &format!("failed to copy package to {}: {e}", install_root.display()),
            json,
        );
    }

    // Open repositories.
    let agent_repo = match open_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };
    let pkg_repo = match open_package_repository_or_create(&data_dir) {
        Ok(r) => r,
        Err(e) => return print_error_and_exit(&e, json),
    };

    // Step 3: register each agent (best-effort per agent).
    //
    // We *do not* abort the whole package on a single agent's validation
    // failure: one broken worker (e.g. a stale top-level Python import)
    // shouldn't make the rest of the package un-installable. Failed agents
    // are reported in the final summary so the operator can fix and re-run.
    //
    // For each agent that declares pip dependencies we provision its venv
    // *before* duck-typing (mirroring what the Supervisor does at boot)
    // so top-level imports of declared packages resolve. Without this step
    // the validator never finds pip-installed modules (incl. the apollia
    // SDK when the agent imports it from a site-packages location).
    let now = now_rfc3339();
    let venv_base = data_dir.join("venvs");
    let install_ctx = PackageInstallCtx {
        version: &pkg_version,
        now: &now,
        skip_tests,
        source_path,
        install_root: &install_root,
        venv_base: &venv_base,
    };
    let (agent_count, failed_agents) =
        register_package_agents(&pkg, &agent_repo, &install_ctx).await;

    if agent_count == 0 {
        return print_error_and_exit(
            &format!(
                "package validation failed: 0 of {} agents could be installed",
                pkg.agents.len()
            ),
            json,
        );
    }

    // Step 4: register the package itself.
    let installed_pkg = InstalledPackage {
        name: pkg_name.clone(),
        version: pkg_version.clone(),
        root_path: install_root.clone(),
        manifest_json: pkg.manifest_json.clone(),
        installed_at: now.clone(),
        updated_at: now.clone(),
    };
    if let Err(e) = pkg_repo.save(&installed_pkg) {
        return print_error_and_exit(&format!("failed to save package: {e}"), json);
    }
    for entry in &pkg.agents {
        if let Err(e) = pkg_repo.link_agent(&pkg_name, &entry.name) {
            return print_error_and_exit(
                &format!("failed to link agent '{}' to package: {e}", entry.name),
                json,
            );
        }
    }

    // Step 5: inject triggers.
    let toml_str = match std::fs::read_to_string(source_path.join("agent.toml")) {
        Ok(s) => s,
        Err(e) => return print_error_and_exit(&format!("failed to read agent.toml: {e}"), json),
    };
    let trigger_count = match inject_package_triggers(&data_dir, &toml_str) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("Warning: trigger injection failed: {e}");
            0
        }
    };

    let runtime_running = client.list_agents().await.is_ok();
    if !runtime_running {
        eprintln!("Info: Runtime not running - agents will auto-start on next boot");
    }

    let summary = InstallSummary {
        pkg_name: &pkg_name,
        pkg_version: &pkg_version,
        agent_count,
        trigger_count,
        install_root: &install_root,
        failed_agents: &failed_agents,
    };
    print_install_summary(&summary, json);
    exit_codes::SUCCESS
}

/// Fields rendered by [`print_install_summary`].
struct InstallSummary<'a> {
    pkg_name: &'a str,
    pkg_version: &'a str,
    agent_count: u32,
    trigger_count: usize,
    install_root: &'a Path,
    failed_agents: &'a [(String, String)],
}

/// Print the package install summary (JSON or human-readable form).
fn print_install_summary(s: &InstallSummary<'_>, json: bool) {
    if json {
        let failed_view: Vec<serde_json::Value> = s
            .failed_agents
            .iter()
            .map(|(n, r)| serde_json::json!({ "agent": n, "reason": r }))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "name": s.pkg_name,
                "version": s.pkg_version,
                "agent_count": s.agent_count,
                "trigger_count": s.trigger_count,
                "install_path": s.install_root.to_string_lossy(),
                "failed_agents": failed_view,
            }))
            .unwrap_or_default()
        );
    } else {
        println!(
            "Package '{}' v{} installed: {} agents, {} triggers",
            s.pkg_name, s.pkg_version, s.agent_count, s.trigger_count,
        );
        println!("  Install path : {}", s.install_root.display());
        if !s.failed_agents.is_empty() {
            println!(
                "  ! {} agent(s) skipped due to validation errors:",
                s.failed_agents.len()
            );
            for (name, reason) in s.failed_agents {
                println!("    - {name}: {reason}");
            }
        }
    }
}

/// Register every agent declared in the package (best-effort, per agent).
///
/// Returns the count of successfully registered agents and the list of
/// `(agent_name, failure_reason)` pairs for the ones that were skipped.
async fn register_package_agents(
    pkg: &apollia_aip::package_loader::AgentPackage,
    agent_repo: &AgentRepository,
    install_ctx: &PackageInstallCtx<'_>,
) -> (u32, Vec<(String, String)>) {
    let mut agent_count = 0;
    let mut failed_agents: Vec<(String, String)> = Vec::new();
    for entry in &pkg.agents {
        let installed_entry_path = install_ctx.install_root.join(
            entry
                .entry
                .strip_prefix(install_ctx.source_path)
                .unwrap_or(&entry.entry),
        );

        // Read this agent's declared pip dependencies from the manifest.
        let agent_packages: Vec<String> = pkg
            .manifest
            .agents
            .iter()
            .find(|a| a.name == entry.name)
            .map(|a| a.packages.clone())
            .unwrap_or_default();

        if let Err(msg) =
            provision_agent_venv(&entry.name, install_ctx.venv_base, &agent_packages).await
        {
            failed_agents.push((entry.name.clone(), msg));
            continue;
        }

        match register_installed_agent(agent_repo, &entry.name, &installed_entry_path, install_ctx)
            .await
        {
            Ok(()) => agent_count += 1,
            Err(msg) => failed_agents.push((entry.name.clone(), msg)),
        }
    }
    (agent_count, failed_agents)
}

/// Provision a per-agent venv for the declared pip packages, mirroring the
/// Supervisor's boot behaviour so top-level imports resolve during
/// validation. A no-op when `packages` is empty. Returns the operator-facing
/// failure reason on error.
async fn provision_agent_venv(
    agent_name: &str,
    venv_base: &Path,
    packages: &[String],
) -> Result<(), String> {
    if packages.is_empty() {
        return Ok(());
    }
    let executor =
        apollia_tools::tools::python_executor::PythonExecutor::new(agent_name, venv_base)
            .map_err(|e| format!("could not initialise per-agent venv: {e}"))?;
    if let Err(e) = executor.setup_venv(packages).await {
        let msg = format!(
            "venv provisioning failed (the agent declares {} pip dep(s)): {e}",
            packages.len()
        );
        tracing::warn!(
            agent = %agent_name,
            error = %msg,
            detail = "the agent is skipped",
            "package.install.venv.failed"
        );
        return Err(msg);
    }
    Ok(())
}

/// Per-package install metadata shared across each agent's registration.
struct PackageInstallCtx<'a> {
    version: &'a str,
    now: &'a str,
    skip_tests: bool,
    source_path: &'a Path,
    install_root: &'a Path,
    venv_base: &'a Path,
}

/// Duck-type validate a packaged agent and persist its `InstalledAgent` row.
/// Returns the operator-facing failure reason on error (the caller records it
/// in the per-package skip summary).
async fn register_installed_agent(
    agent_repo: &AgentRepository,
    agent_name: &str,
    installed_entry_path: &Path,
    ctx: &PackageInstallCtx<'_>,
) -> Result<(), String> {
    let agent_manifest = match validate_community_agent(installed_entry_path, ctx.skip_tests).await
    {
        Ok(m) => m,
        Err(e) => {
            let msg = e.to_string();
            tracing::warn!(
                agent = %agent_name,
                error = %msg,
                detail = "the agent is skipped",
                "package.install.validation.failed"
            );
            return Err(msg);
        }
    };

    let installed_agent = InstalledAgent {
        name: agent_name.to_string(),
        version: ctx.version.to_string(),
        install_path: installed_entry_path.to_path_buf(),
        source_path: installed_entry_path.to_path_buf(),
        manifest: agent_manifest,
        enabled: true,
        installed_at: ctx.now.to_string(),
        updated_at: ctx.now.to_string(),
    };
    if let Err(e) = agent_repo.save(&installed_agent) {
        tracing::warn!(
            agent = %agent_name,
            error = %e,
            detail = "the agent is skipped",
            "package.install.persist.failed"
        );
        return Err(format!("save failed: {e}"));
    }
    Ok(())
}

/// Parse triggers from `agent.toml` content and upsert into `triggers_def.db`.
///
/// Returns the number of triggers successfully injected.
fn inject_package_triggers(data_dir: &Path, toml_str: &str) -> Result<usize, String> {
    let trigger_defs =
        parse_triggers_from_toml_str(toml_str).map_err(|e| format!("trigger parse error: {e}"))?;

    if trigger_defs.is_empty() {
        return Ok(0);
    }

    let triggers_db = data_dir.join(apollia_core::paths::DataFile::TriggersDef.file_name());
    let repo = TriggerDefinitionRepository::open(&triggers_db)
        .map_err(|e| format!("cannot open triggers_def.db: {e}"))?;

    let mut count = 0;
    for def in &trigger_defs {
        let row = trigger_def_to_row(def);
        // Upsert: delete if exists, then insert.
        let _ = repo.delete(&def.id);
        repo.insert(&row)
            .map_err(|e| format!("failed to insert trigger '{}': {e}", def.id))?;
        count += 1;
    }
    Ok(count)
}

/// Convert a [`TriggerDefinition`] to a [`TriggerDefinitionRow`] for persistence.
fn trigger_def_to_row(def: &apollia_triggers::TriggerDefinition) -> TriggerDefinitionRow {
    let (source_type, source_config) = match &def.source {
        TriggerSourceConfig::Cron { schedule } => (
            "cron".to_string(),
            serde_json::json!({"schedule": schedule}),
        ),
        TriggerSourceConfig::Interval { every } => {
            ("interval".to_string(), serde_json::json!({"every": every}))
        }
        TriggerSourceConfig::Oneshot { fire_at } => (
            "oneshot".to_string(),
            serde_json::json!({"fire_at": fire_at.to_rfc3339()}),
        ),
        TriggerSourceConfig::FileWatch {
            path,
            events,
            recursive,
            follow_symlinks,
            exclude_patterns,
        } => (
            "file_watch".to_string(),
            serde_json::json!({
                "path": path.to_string_lossy(),
                "events": events,
                "recursive": recursive,
                "follow_symlinks": follow_symlinks,
                "exclude_patterns": exclude_patterns,
            }),
        ),
        TriggerSourceConfig::Webhook { secret } => {
            ("webhook".to_string(), serde_json::json!({"secret": secret}))
        }
    };

    let on_busy = match &def.on_busy {
        OnBusyPolicy::Skip => OnBusy::Drop,
        OnBusyPolicy::Queue { .. } | OnBusyPolicy::Block => OnBusy::Queue,
    };

    TriggerDefinitionRow {
        id: def.id.clone(),
        agent: if def.agent.is_empty() {
            None
        } else {
            Some(def.agent.clone())
        },
        enabled: def.enabled,
        on_busy,
        source_type,
        source_config,
        input_template: if def.input_template.0.is_empty() {
            None
        } else {
            Some(def.input_template.0.clone())
        },
        created_at: now_rfc3339(),
        updated_at: now_rfc3339(),
    }
}

/// Recursively copy a directory tree from `src` to `dst`.
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
