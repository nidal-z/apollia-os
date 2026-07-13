use super::*;

/// `apollia-os agent validate <path>`: validate an agent manifest without starting it.
///
/// Performs AIP duck-typing validation via PyO3, then reports the manifest
/// summary and any tool requirements. Exit 0 on success (with optional warnings),
/// exit 1 if the manifest is invalid or a required tool is absent.
pub(in crate::commands::agent) fn run_validate(path: &Path, json: bool) -> i32 {
    // File existence check before invoking PyO3.
    if !path.exists() {
        return print_error_and_exit(&format!("file not found: {}", path.display()), json);
    }

    let loader = CliAgentLoader;
    let manifest = match loader.load_and_validate(path) {
        Ok(m) => m,
        Err(e) => return print_error_and_exit(&format!("manifest invalid: {e}"), json),
    };

    if json {
        print_validate_json(&manifest);
    } else {
        print_validate_text(&manifest);
    }

    exit_codes::SUCCESS
}

/// Emit the full validated-manifest payload as pretty JSON.
fn print_validate_json(manifest: &apollia_core::AgentManifest) {
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "valid": true,
            "name": manifest.name,
            "version": manifest.version,
            "description": manifest.description,
            "execution_mode": manifest.execution_mode,
            "memory_namespace": manifest.memory_namespace,
            "max_concurrent_tasks": manifest.max_concurrent_tasks,
            "supports_a2a": manifest.supports_a2a,
            "supports_streaming": manifest.supports_streaming,
            "dangerous_tools_allowed": manifest.dangerous_tools_allowed,
            "user_memory_write": manifest.user_memory_write,
            "tools_required": manifest.tools_required,
            "tools_optional": manifest.tools_optional,
            "tools_requiring_approval": manifest.tools_requiring_approval,
            "step_budget": manifest.step_budget,
            "skills": manifest.skills,
            "tags": manifest.tags,
            "packages": manifest.packages,
            "datasources": manifest.datasources,
            "templates": manifest.templates,
            "secrets": manifest.secrets,
        }))
        .unwrap_or_default()
    );
}

/// Compute the comma-joined feature-flag summary for a manifest, or `None`
/// when no flags are set.
fn manifest_feature_flags(manifest: &apollia_core::AgentManifest) -> Option<String> {
    let mut feature_flags = Vec::new();
    if manifest.supports_a2a {
        feature_flags.push("a2a");
    }
    if manifest.supports_streaming {
        feature_flags.push("streaming");
    }
    if manifest.user_memory_write {
        feature_flags.push("writes user memory");
    }
    if manifest.dangerous_tools_allowed {
        feature_flags.push("dangerous tools allowed");
    }
    if feature_flags.is_empty() {
        None
    } else {
        Some(feature_flags.join(", "))
    }
}

/// Render the validated-manifest summary as human-readable text.
fn print_validate_text(manifest: &apollia_core::AgentManifest) {
    let required = &manifest.tools_required;
    let optional = &manifest.tools_optional;

    println!("✔ Manifest valide");
    println!("  Name             : {}", manifest.name);
    println!("  Version          : {}", manifest.version);
    if !manifest.description.is_empty() {
        println!("  Description      : {}", manifest.description);
    }
    println!("  Execution mode   : {}", manifest.execution_mode);
    if let Some(ns) = &manifest.memory_namespace {
        println!("  Memory namespace : {ns}");
    }
    println!("  Max concurrency  : {}", manifest.max_concurrent_tasks);
    if let Some(flags) = manifest_feature_flags(manifest) {
        println!("  Features         : {flags}");
    }
    if !required.is_empty() {
        println!("  Required tools   : {}", required.join(", "));
    }
    if !optional.is_empty() {
        println!("  Optional tools   : {}", optional.join(", "));
        println!("  ⚠ Optional tools not checked - agent may start in DEGRADED mode if absent");
    }
    if !manifest.tools_requiring_approval.is_empty() {
        println!(
            "  HITL gated tools : {}",
            manifest.tools_requiring_approval.join(", ")
        );
    }
    if let Some(budget) = &manifest.step_budget {
        println!(
            "  Step budget      : {} steps, {} tool calls, {}s wall-clock",
            budget.max_steps, budget.max_tool_calls, budget.wall_clock_secs
        );
    }
    print_validate_text_skills(manifest);
    print_validate_text_lists(manifest);
    print_validate_text_setup_notes(manifest);
}

/// Print the declared skills section of a validated manifest.
fn print_validate_text_skills(manifest: &apollia_core::AgentManifest) {
    if manifest.skills.is_empty() {
        return;
    }
    println!("  Skills ({})       :", manifest.skills.len());
    for skill in &manifest.skills {
        println!("    - {} ({})", skill.id, skill.name);
    }
}

/// Print the multi-line setup notes section of a validated manifest.
fn print_validate_text_setup_notes(manifest: &apollia_core::AgentManifest) {
    let Some(notes) = &manifest.setup_notes else {
        return;
    };
    if notes.is_empty() {
        return;
    }
    println!("  Setup notes      :");
    for line in notes.lines() {
        println!("    {line}");
    }
}

/// Print the trailing string-list fields (tags, deps, datasources, etc.) of
/// a validated manifest in the human-readable summary.
fn print_validate_text_lists(manifest: &apollia_core::AgentManifest) {
    if !manifest.tags.is_empty() {
        println!("  Tags             : {}", manifest.tags.join(", "));
    }
    if !manifest.packages.is_empty() {
        println!("  Pip deps         : {}", manifest.packages.join(", "));
    }
    if !manifest.datasources.is_empty() {
        println!("  Datasources      : {}", manifest.datasources.join(", "));
    }
    if !manifest.templates.is_empty() {
        println!("  Templates        : {}", manifest.templates.join(", "));
    }
    if !manifest.secrets.is_empty() {
        println!("  Secrets          : {}", manifest.secrets.join(", "));
    }
}
