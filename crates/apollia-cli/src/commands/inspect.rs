//! `apollia-os inspect <agent.py>`: static inspection of a Python agent.
//!
//! Loads the agent module in isolation via the PyO3 loader (the same path
//! `community.rs` uses for validation), introspects its
//! `__apollia_manifest__` through [`apollia_aip::validator::validate_agent`],
//! and runs a set of static checks against the agent's declared artifacts
//! (datasources, templates, secrets, permissions). No runtime, EventBus,
//! actor, or HTTP call is involved.
//!
//! Output:
//! - human-readable report by default (colored only on a TTY when neither
//!   `--no-color` nor `NO_COLOR` are set);
//! - one structured JSON document with `--json`.
//!
//! Exit codes: `0` when the inspection succeeds (warnings allowed), `1` when
//! it fails (validation error, missing or invalid artifact, or a file/arg
//! error). The runtime exit code `2` is never used: inspect makes no runtime
//! call.

use std::io::IsTerminal;
use std::path::Path;

use apollia_core::AgentManifest;
use serde::Serialize;

use crate::community::validation_sys_paths;
use crate::exit_codes;
use crate::note;

/// Schema version of the `--json` output document. Bump on breaking changes.
const SCHEMA_VERSION: u32 = 1;

/// Status of a checked artifact (datasource, template).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum ArtifactStatus {
    /// The artifact exists and (where applicable) parses.
    Ok,
    /// The declared file is absent on disk.
    Missing,
    /// The declared file exists but fails to parse.
    Invalid,
}

/// A declared datasource and its on-disk verification result.
#[derive(Debug, Serialize)]
struct DatasourceReport {
    /// Logical name declared in the manifest.
    name: String,
    /// Resolved path checked on disk (relative to the agent directory).
    path: String,
    /// Verification status.
    status: ArtifactStatus,
    /// Detail message when the status is not `Ok`.
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

/// A declared Jinja2 template and its on-disk presence result.
#[derive(Debug, Serialize)]
struct TemplateReport {
    /// Logical name declared in the manifest.
    name: String,
    /// Resolved path checked on disk (relative to the agent directory).
    path: String,
    /// Presence status (`Ok` when found, `Missing` otherwise). Jinja syntax
    /// is intentionally not validated.
    status: ArtifactStatus,
}

/// A declared secret. Inspection is static: the key is listed as declared,
/// never resolved against the secret store (that would need the runtime).
#[derive(Debug, Serialize)]
struct SecretReport {
    /// Logical secret key declared in the manifest.
    name: String,
    /// Always `"declared"`. Static inspection cannot confirm configuration.
    status: &'static str,
}

/// A declared skill rendered from the validated manifest.
#[derive(Debug, Serialize)]
struct SkillReport {
    /// Unique skill identifier.
    id: String,
    /// Human-readable description.
    description: String,
    /// Inferred input schema (Apollia field schema), if published.
    #[serde(skip_serializing_if = "Option::is_none")]
    input_schema: Option<serde_json::Value>,
    /// Declared output modes (e.g. `["text", "file"]`).
    output_modes: Vec<String>,
}

/// The declared permission surface of the agent.
#[derive(Debug, Serialize)]
struct PermissionsReport {
    /// Tools the agent requires (fail-fast at startup).
    tools_required: Vec<String>,
    /// Tools the agent can use if present (degraded otherwise).
    tools_optional: Vec<String>,
    /// Tools that require human approval before execution.
    tools_requiring_approval: Vec<String>,
    /// Whether the agent opts into tools flagged `dangerous`.
    dangerous_tools_allowed: bool,
}

/// Summary of the manifest identity block.
#[derive(Debug, Serialize)]
struct ManifestReport {
    /// Agent name.
    name: String,
    /// Semver version.
    version: String,
    /// Human-readable description.
    description: String,
    /// pip packages declared for the agent venv.
    packages: Vec<String>,
    /// Source Python class name, when captured by the validator.
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_class: Option<String>,
}

/// Full inspection report. Serialized verbatim in `--json` mode.
#[derive(Debug, Serialize)]
struct InspectReport {
    /// Schema version of this document.
    schema_version: u32,
    /// `true` when there are no errors (warnings are tolerated).
    ok: bool,
    /// Manifest identity block. Absent when loading or validation failed
    /// before a manifest could be produced.
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest: Option<ManifestReport>,
    /// Declared skills.
    skills: Vec<SkillReport>,
    /// Declared datasources with on-disk status.
    datasources: Vec<DatasourceReport>,
    /// Declared templates with on-disk status.
    templates: Vec<TemplateReport>,
    /// Declared secrets (static listing only).
    secrets: Vec<SecretReport>,
    /// Declared permission surface. Absent when validation failed early.
    #[serde(skip_serializing_if = "Option::is_none")]
    permissions: Option<PermissionsReport>,
    /// Non-fatal warnings.
    warnings: Vec<String>,
    /// Fatal errors. A non-empty list means `ok = false` and exit code 1.
    errors: Vec<String>,
}

/// Inspect a Python agent file and print a static report.
///
/// `path` is the agent `.py` file. `json` forces the structured JSON
/// document. `--quiet`, read from the output layer, restricts the human report
/// to warnings and errors. `no_color` (or the `NO_COLOR` env var, or a non-TTY
/// stdout) disables ANSI styling.
///
/// Returns [`exit_codes::SUCCESS`] when the inspection succeeds (warnings
/// allowed) and [`exit_codes::GENERAL_ERROR`] otherwise.
pub fn run(path: &Path, json: bool, no_color: bool) -> i32 {
    let report = build_report(path);
    let ok = report.ok;

    if json {
        match serde_json::to_string_pretty(&report) {
            Ok(text) => println!("{text}"),
            Err(e) => {
                // Last-resort minimal valid JSON so machine callers never see
                // a non-JSON document on the happy-path channel.
                println!(
                    "{{\"schema_version\":{SCHEMA_VERSION},\"ok\":false,\"errors\":[\"failed to serialize report: {e}\"]}}"
                );
                return exit_codes::GENERAL_ERROR;
            }
        }
    } else {
        let color = use_color(no_color);
        print_human(&report, color);
    }

    if ok {
        exit_codes::SUCCESS
    } else {
        exit_codes::GENERAL_ERROR
    }
}

/// Decide whether to emit ANSI color: only on a TTY, and only when neither
/// `--no-color` nor the `NO_COLOR` env var disable it.
fn use_color(no_color: bool) -> bool {
    if no_color || std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::io::stdout().is_terminal()
}

/// Load, validate, and statically check the agent at `path`.
fn build_report(path: &Path) -> InspectReport {
    let mut warnings: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    // Argument / file guards first, so we never reach into PyO3 with a bad
    // path and we return a clean, machine-readable error.
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if extension != "py" {
        errors.push(format!(
            "invalid path (expected a .py file): {}",
            path.display()
        ));
        return empty_report(errors);
    }
    if !path.exists() {
        errors.push(format!("agent file not found: {}", path.display()));
        return empty_report(errors);
    }

    // Reuse the community-agent sys.path resolution (venv + package root +
    // workspace SDK) and the shared PyO3 module loader.
    let extras = validation_sys_paths(path);
    let module = match apollia_aip::loader::load_agent_module_with_sys_paths(path, &extras) {
        Ok(module) => module,
        Err(e) => {
            errors.push(format!("failed to load agent module: {e}"));
            return empty_report(errors);
        }
    };

    let manifest = match apollia_aip::validator::validate_agent(&module) {
        Ok(validated) => validated.manifest,
        Err(e) => {
            errors.push(format!("manifest validation failed: {e}"));
            return empty_report(errors);
        }
    };

    let agent_dir = path.parent().unwrap_or_else(|| Path::new("."));

    let skills = build_skills(&manifest);
    let datasources = check_datasources(agent_dir, &manifest, &mut errors);
    let templates = check_templates(agent_dir, &manifest, &mut errors);
    let secrets = build_secrets(&manifest, &mut warnings);

    if manifest.dangerous_tools_allowed {
        warnings.push(
            "agent declares dangerous_tools_allowed: dangerous tools may run without per-call gating"
                .to_string(),
        );
    }

    let permissions = PermissionsReport {
        tools_required: manifest.tools_required.clone(),
        tools_optional: manifest.tools_optional.clone(),
        tools_requiring_approval: manifest.tools_requiring_approval.clone(),
        dangerous_tools_allowed: manifest.dangerous_tools_allowed,
    };

    let manifest_report = ManifestReport {
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        description: manifest.description.clone(),
        packages: manifest.packages.clone(),
        agent_class: manifest.agent_class.clone(),
    };

    let ok = errors.is_empty();
    InspectReport {
        schema_version: SCHEMA_VERSION,
        ok,
        manifest: Some(manifest_report),
        skills,
        datasources,
        templates,
        secrets,
        permissions: Some(permissions),
        warnings,
        errors,
    }
}

/// Build an early-return report carrying only errors (load or guard failed).
fn empty_report(errors: Vec<String>) -> InspectReport {
    InspectReport {
        schema_version: SCHEMA_VERSION,
        ok: false,
        manifest: None,
        skills: Vec::new(),
        datasources: Vec::new(),
        templates: Vec::new(),
        secrets: Vec::new(),
        permissions: None,
        warnings: Vec::new(),
        errors,
    }
}

/// Render each declared skill from the validated manifest.
fn build_skills(manifest: &AgentManifest) -> Vec<SkillReport> {
    manifest
        .skills
        .iter()
        .map(|s| SkillReport {
            id: s.id.clone(),
            description: s.description.clone(),
            input_schema: s.input_schema.clone(),
            output_modes: s.output_modes.clone(),
        })
        .collect()
}

/// Resolve and verify each declared datasource under `<dir>/datasources/`.
///
/// A datasource is `Ok` when its file exists and parses as YAML, `Missing`
/// when no file is found, and `Invalid` when the file parses fail. Missing or
/// invalid datasources are errors.
fn check_datasources(
    agent_dir: &Path,
    manifest: &AgentManifest,
    errors: &mut Vec<String>,
) -> Vec<DatasourceReport> {
    let mut out = Vec::with_capacity(manifest.datasources.len());
    for name in &manifest.datasources {
        let candidates = [
            agent_dir.join("datasources").join(format!("{name}.yaml")),
            agent_dir.join("datasources").join(format!("{name}.yml")),
        ];
        let found = candidates.iter().find(|p| p.exists());
        let display_path = candidates[0].display().to_string();
        match found {
            None => {
                errors.push(format!(
                    "datasource '{name}' file not found: {display_path}"
                ));
                out.push(DatasourceReport {
                    name: name.clone(),
                    path: display_path,
                    status: ArtifactStatus::Missing,
                    detail: Some("file not found".to_string()),
                });
            }
            Some(file) => {
                let resolved = file.display().to_string();
                match parse_yaml(file) {
                    Ok(()) => out.push(DatasourceReport {
                        name: name.clone(),
                        path: resolved,
                        status: ArtifactStatus::Ok,
                        detail: None,
                    }),
                    Err(e) => {
                        errors.push(format!("datasource '{name}' is invalid: {e}"));
                        out.push(DatasourceReport {
                            name: name.clone(),
                            path: resolved,
                            status: ArtifactStatus::Invalid,
                            detail: Some(e),
                        });
                    }
                }
            }
        }
    }
    out
}

/// Parse a YAML file, returning a human-readable error string on failure.
fn parse_yaml(path: &Path) -> Result<(), String> {
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_yaml::from_str::<serde_yaml::Value>(&raw)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Resolve and verify each declared template under `<dir>/templates/`.
///
/// A template is `Ok` when a file with a recognised Jinja extension exists.
/// Jinja syntax is not validated. A missing template is an error.
fn check_templates(
    agent_dir: &Path,
    manifest: &AgentManifest,
    errors: &mut Vec<String>,
) -> Vec<TemplateReport> {
    let mut out = Vec::with_capacity(manifest.templates.len());
    for name in &manifest.templates {
        let candidates = [
            agent_dir.join("templates").join(format!("{name}.j2")),
            agent_dir.join("templates").join(format!("{name}.jinja2")),
            agent_dir.join("templates").join(format!("{name}.jinja")),
        ];
        let found = candidates.iter().find(|p| p.exists());
        let display_path = candidates[0].display().to_string();
        match found {
            Some(file) => out.push(TemplateReport {
                name: name.clone(),
                path: file.display().to_string(),
                status: ArtifactStatus::Ok,
            }),
            None => {
                errors.push(format!("template '{name}' file not found: {display_path}"));
                out.push(TemplateReport {
                    name: name.clone(),
                    path: display_path,
                    status: ArtifactStatus::Missing,
                });
            }
        }
    }
    out
}

/// List declared secrets. Static inspection cannot reach the secret store
/// without the runtime, so every declared key is reported as `declared` and
/// a single advisory warning is emitted when at least one secret exists.
fn build_secrets(manifest: &AgentManifest, warnings: &mut Vec<String>) -> Vec<SecretReport> {
    if !manifest.secrets.is_empty() {
        warnings.push(format!(
            "{} secret(s) declared: configuration is not verified by static inspection",
            manifest.secrets.len()
        ));
    }
    manifest
        .secrets
        .iter()
        .map(|name| SecretReport {
            name: name.clone(),
            status: "declared",
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Human-readable rendering
// ─────────────────────────────────────────────────────────────────────────────

/// ANSI styling helpers gated on the `color` flag.
struct Style {
    color: bool,
}

impl Style {
    fn bold(&self, s: &str) -> String {
        if self.color {
            format!("\x1b[1m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    fn green(&self, s: &str) -> String {
        if self.color {
            format!("\x1b[32m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    fn yellow(&self, s: &str) -> String {
        if self.color {
            format!("\x1b[33m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    fn red(&self, s: &str) -> String {
        if self.color {
            format!("\x1b[31m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
}

/// Render the report in human-readable form. `--quiet` keeps only warnings and
/// errors plus the final status line.
fn print_human(report: &InspectReport, color: bool) {
    let style = Style { color };

    if !crate::output::is_quiet() {
        if let Some(manifest) = &report.manifest {
            let class = manifest.agent_class.as_deref().unwrap_or("unknown class");
            println!();
            println!("  {}", style.bold("Module loaded"));
            println!("    class    {class}");
            println!("    name     {}", manifest.name);
            println!("    version  {}", manifest.version);

            println!();
            println!("  {}", style.bold("Manifest"));
            println!("    name         {}", manifest.name);
            println!("    version      {}", manifest.version);
            println!("    description  {}", manifest.description);
            if manifest.packages.is_empty() {
                println!("    packages     (none)");
            } else {
                println!("    packages     {}", manifest.packages.join(", "));
            }

            println!();
            println!("  {}", style.bold("Skills"));
            if report.skills.is_empty() {
                println!("    (none declared)");
            } else {
                for skill in &report.skills {
                    println!("    - {} : {}", style.bold(&skill.id), skill.description);
                    match &skill.input_schema {
                        Some(schema) => println!(
                            "        input   {}",
                            serde_json::to_string(schema)
                                .unwrap_or_else(|_| "<unserializable>".into())
                        ),
                        None => println!("        input   (no published schema)"),
                    }
                    if skill.output_modes.is_empty() {
                        println!("        output  (modes unspecified)");
                    } else {
                        println!("        output  {}", skill.output_modes.join(", "));
                    }
                }
            }

            println!();
            println!("  {}", style.bold("Datasources"));
            if report.datasources.is_empty() {
                println!("    (none declared)");
            } else {
                for ds in &report.datasources {
                    let badge = status_badge(ds.status, &style);
                    println!("    {badge} {} -> {}", ds.name, ds.path);
                    if let Some(detail) = &ds.detail {
                        println!("        {detail}");
                    }
                }
            }

            println!();
            println!("  {}", style.bold("Templates"));
            if report.templates.is_empty() {
                println!("    (none declared)");
            } else {
                for tpl in &report.templates {
                    let badge = status_badge(tpl.status, &style);
                    println!("    {badge} {} -> {}", tpl.name, tpl.path);
                }
            }

            println!();
            println!("  {}", style.bold("Secrets"));
            if report.secrets.is_empty() {
                println!("    (none declared)");
            } else {
                for secret in &report.secrets {
                    println!("    - {} ({})", secret.name, secret.status);
                }
            }

            if let Some(perms) = &report.permissions {
                println!();
                println!("  {}", style.bold("Permissions"));
                println!(
                    "    tools.required           {}",
                    join_or_none(&perms.tools_required)
                );
                println!(
                    "    tools.optional           {}",
                    join_or_none(&perms.tools_optional)
                );
                println!(
                    "    tools.requiring_approval {}",
                    join_or_none(&perms.tools_requiring_approval)
                );
                println!(
                    "    dangerous_tools_allowed  {}",
                    perms.dangerous_tools_allowed
                );
            }
        }
    }

    note!();
    println!("  {} ({})", style.bold("Warnings"), report.warnings.len());
    for w in &report.warnings {
        println!("    {} {w}", style.yellow("!"));
    }

    note!();
    println!("  {} ({})", style.bold("Errors"), report.errors.len());
    for e in &report.errors {
        println!("    {} {e}", style.red("x"));
    }

    note!();
    if report.ok {
        println!("  {}", style.green("Inspection OK"));
    } else {
        println!("  {}", style.red("Inspection FAILED"));
    }
    note!();
}

/// Render a colored status badge for an artifact.
fn status_badge(status: ArtifactStatus, style: &Style) -> String {
    match status {
        ArtifactStatus::Ok => style.green("OK     "),
        ArtifactStatus::Missing => style.red("MISSING"),
        ArtifactStatus::Invalid => style.red("INVALID"),
    }
}

/// Join a list with commas, or render `(none)` when empty.
fn join_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "(none)".to_string()
    } else {
        items.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_invalid_extension_is_error() {
        // GIVEN a path that is not a .py file
        let path = PathBuf::from("/tmp/not_an_agent.txt");

        // WHEN we build the report
        let report = build_report(&path);

        // THEN the report fails with an invalid-path error and no manifest
        assert!(!report.ok);
        assert!(report.manifest.is_none());
        assert!(report.errors.iter().any(|e| e.contains("invalid path")));
    }

    #[test]
    fn test_missing_file_is_error() {
        // GIVEN a .py path that does not exist
        let path = PathBuf::from("/tmp/__apollia_inspect_nonexistent_xyz.py");

        // WHEN we build the report
        let report = build_report(&path);

        // THEN the report fails with a file-not-found error
        assert!(!report.ok);
        assert!(report.manifest.is_none());
        assert!(report.errors.iter().any(|e| e.contains("not found")));
    }

    #[test]
    fn test_empty_report_is_not_ok() {
        // GIVEN an early-return error report
        let report = empty_report(vec!["boom".to_string()]);

        // WHEN we inspect its shape
        // THEN ok is false and the schema version is stamped
        assert!(!report.ok);
        assert_eq!(report.schema_version, SCHEMA_VERSION);
        assert!(report.manifest.is_none());
    }

    #[test]
    fn test_error_report_serializes_to_valid_json() {
        // GIVEN a failing report
        let report = empty_report(vec!["bad agent".to_string()]);

        // WHEN serialized
        let text = serde_json::to_string(&report).expect("serialize must succeed");

        // THEN it is valid JSON carrying ok=false and the error
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
        assert_eq!(parsed["ok"], serde_json::Value::Bool(false));
        assert_eq!(parsed["schema_version"], serde_json::json!(SCHEMA_VERSION));
        assert_eq!(parsed["errors"][0], serde_json::json!("bad agent"));
    }

    #[test]
    fn test_use_color_disabled_by_flag() {
        // GIVEN the --no-color flag set
        // WHEN we resolve color
        // THEN color is disabled regardless of TTY
        assert!(!use_color(true));
    }

    #[test]
    fn test_join_or_none() {
        // GIVEN an empty and a populated list
        // WHEN joined
        // THEN empty renders as (none) and populated joins with commas
        assert_eq!(join_or_none(&[]), "(none)");
        assert_eq!(join_or_none(&["a".to_string(), "b".to_string()]), "a, b");
    }
}
