use super::*;

/// Supported agent template types.
pub(in crate::commands::agent) const VALID_AGENT_TYPES: &[&str] =
    &["react", "conversational", "orchestrated"];

/// `apollia-os agent new <name> [--type <type>]`: scaffold a new agent via the SDK.
pub(in crate::commands::agent) fn run_new(name: &str, agent_type: &str, json: bool) -> i32 {
    // Validate template type.
    if !VALID_AGENT_TYPES.contains(&agent_type) {
        let msg = format!(
            "Invalid type '{}'. Supported types: {}",
            agent_type,
            VALID_AGENT_TYPES.join(", ")
        );
        return print_error_and_exit(&msg, json);
    }

    // Verify the SDK is installed.
    if let Err(msg) = check_sdk_installed() {
        return print_error_and_exit(&msg, json);
    }

    // Check for name conflict in ~/.apollia/agents/.
    let agents_dir = apollia_data_dir().join("agents");
    let target_dir = agents_dir.join(name);
    if target_dir.exists() {
        let msg = format!(
            "An agent '{}' already exists. Use a different name or remove the existing one with: \
             apollia-os agent uninstall {}",
            name, name
        );
        return print_error_and_exit(&msg, json);
    }

    // Delegate to `python3 -m apollia new <name> --type <type> --output-dir <path>`.
    let mut probe = std::process::Command::new("python3");
    apollia_core::subprocess_env::scrub_bundled_python(&mut probe);
    let output = match probe
        .args([
            "-m",
            "apollia",
            "new",
            name,
            "--type",
            agent_type,
            "--output-dir",
        ])
        .arg(&target_dir)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            let msg = format!("Failed to execute python3: {e}");
            return print_error_and_exit(&msg, json);
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = format!("Scaffolding failed: {}", stderr.trim());
        return print_error_and_exit(&msg, json);
    }

    let files = list_generated_files(&target_dir);

    if json {
        let json_output = serde_json::json!({
            "name": name,
            "type": agent_type,
            "path": target_dir.to_string_lossy(),
            "files": files,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&json_output).unwrap_or_default()
        );
    } else {
        println!("Agent '{}' created in {}", name, target_dir.display());
        for f in &files {
            println!("  {f}");
        }
    }

    exit_codes::SUCCESS
}

/// Verify that the Apollia Python SDK is importable.
fn check_sdk_installed() -> Result<(), String> {
    let mut scaffold = std::process::Command::new("python3");
    apollia_core::subprocess_env::scrub_bundled_python(&mut scaffold);
    let output = scaffold
        .args(["-c", "import apollia"])
        .output()
        .map_err(|e| format!("python3 not found: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err("apollia-sdk is not installed. Install it with: pip install apollia-sdk".to_string())
    }
}

/// List files generated in `dir`, returning relative paths sorted alphabetically.
pub(in crate::commands::agent) fn list_generated_files(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect_files_recursive(dir, dir, &mut files);
    files.sort();
    files
}

/// Recursively collect file paths relative to `base`.
fn collect_files_recursive(base: &Path, current: &Path, out: &mut Vec<String>) {
    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(base, &path, out);
        } else if let Ok(rel) = path.strip_prefix(base) {
            out.push(rel.to_string_lossy().to_string());
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────
