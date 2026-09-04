//! `llm costs`, the cost threshold, and the local setup wizard.

use std::path::PathBuf;

use crate::client::RuntimeClient;
use crate::exit_codes;

use super::handle_error;

// ─────────────────────────────────────────────
// Costs handler
// ─────────────────────────────────────────────

/// `apollia-os llm costs`: display aggregated usage and costs per backend.
pub(super) async fn run_costs(
    client: &RuntimeClient,
    get_threshold: bool,
    set_threshold: Option<f64>,
    config_path_override: Option<&std::path::Path>,
    json: bool,
) -> i32 {
    if get_threshold {
        return run_get_cost_threshold(config_path_override, json);
    }
    if let Some(v) = set_threshold {
        return run_set_cost_threshold(v, config_path_override, json);
    }
    match client.get_llm_costs().await {
        Ok(resp) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&resp).unwrap_or_default()
                );
            } else {
                format_llm_costs(&resp);
            }
            exit_codes::SUCCESS
        }
        Err(e) => handle_error(e, json),
    }
}

/// Locates the `apollia.toml` the cost alert threshold lives in.
///
/// Delegates to the runtime's own search order (`./apollia.toml`, then
/// `$XDG_CONFIG_HOME/apollia/apollia.toml`). `~/.apollia/` holds the data files,
/// not the configuration: a threshold written there is a threshold the router
/// never loads.
pub(super) fn resolve_apollia_toml(override_path: Option<&std::path::Path>) -> PathBuf {
    crate::commands::config::resolve_path(override_path)
}

pub(super) fn emit_llm_error(msg: String, json: bool) -> i32 {
    crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg.to_string())
}

pub(super) fn run_get_cost_threshold(override_path: Option<&std::path::Path>, json: bool) -> i32 {
    let path = resolve_apollia_toml(override_path);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Treat an absent file as "no threshold set" so scripts can detect
            // the unconfigured state with `--json | jq .threshold == null`.
            if json {
                let body = serde_json::json!({
                    "path": path.display().to_string(),
                    "threshold": serde_json::Value::Null,
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&body).unwrap_or_default()
                );
            } else {
                println!("  (no cost alert threshold set in {})", path.display());
            }
            return exit_codes::SUCCESS;
        }
        Err(e) => return emit_llm_error(format!("read {} failed: {e}", path.display()), json),
    };
    let doc: toml_edit::DocumentMut = match content.parse() {
        Ok(d) => d,
        Err(e) => return emit_llm_error(format!("parse {} failed: {e}", path.display()), json),
    };
    let threshold = doc
        .get("llm")
        .and_then(|t| t.get("cost_alert_threshold_usd"))
        .and_then(|v| v.as_float());
    if json {
        let body = serde_json::json!({
            "path": path.display().to_string(),
            "threshold": threshold,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    } else {
        match threshold {
            Some(v) => println!("  cost_alert_threshold_usd = {v} (in {})", path.display()),
            None => println!("  (no cost alert threshold set in {})", path.display()),
        }
    }
    exit_codes::SUCCESS
}

pub(super) fn run_set_cost_threshold(
    value: f64,
    override_path: Option<&std::path::Path>,
    json: bool,
) -> i32 {
    let path = resolve_apollia_toml(override_path);
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return emit_llm_error(format!("create {} failed: {e}", parent.display()), json);
        }
    }
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let mut doc: toml_edit::DocumentMut = match content.parse() {
        Ok(d) => d,
        Err(e) => return emit_llm_error(format!("parse {} failed: {e}", path.display()), json),
    };
    if value <= 0.0 {
        // Clear semantics: drop the key but keep the table.
        if let Some(table) = doc.get_mut("llm").and_then(|i| i.as_table_mut()) {
            table.remove("cost_alert_threshold_usd");
        }
    } else {
        let llm_table = doc
            .entry("llm")
            .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
            .as_table_mut();
        if let Some(table) = llm_table {
            table.insert("cost_alert_threshold_usd", toml_edit::value(value));
        }
    }
    if let Err(e) = std::fs::write(&path, doc.to_string()) {
        return emit_llm_error(format!("write {} failed: {e}", path.display()), json);
    }
    if json {
        let body = serde_json::json!({
            "path": path.display().to_string(),
            "threshold": if value <= 0.0 { serde_json::Value::Null } else { serde_json::json!(value) },
            "cleared": value <= 0.0,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    } else if value <= 0.0 {
        println!("  * cost_alert_threshold cleared in {}", path.display());
    } else {
        println!(
            "  * cost_alert_threshold_usd = {value} written to {}",
            path.display()
        );
    }
    exit_codes::SUCCESS
}

/// Arguments for [`run_setup`], grouped to keep the signature small.
pub(super) struct SetupArgs<'a> {
    pub(super) local: bool,
    pub(super) model: &'a std::path::Path,
    pub(super) backend_name: &'a str,
    pub(super) device_override: Option<&'a str>,
    pub(super) system_db_override: Option<&'a std::path::Path>,
    pub(super) models_dir_override: Option<&'a std::path::Path>,
    pub(super) json: bool,
}

pub(super) fn run_setup(args: SetupArgs<'_>) -> i32 {
    let SetupArgs {
        local,
        model,
        backend_name,
        device_override,
        system_db_override,
        models_dir_override,
        json,
    } = args;

    if let Err(code) = validate_setup_model(local, model, json) {
        return code;
    }

    let device = device_override.map(str::to_string).unwrap_or_else(|| {
        if cfg!(target_os = "macos") {
            "metal".to_string()
        } else {
            "cpu".to_string()
        }
    });

    let apollia_home = dirs::home_dir()
        .map(apollia_core::paths::data_dir_under)
        .unwrap_or_else(|| PathBuf::from(apollia_core::paths::DATA_DIR_NAME));
    let models_dir = models_dir_override
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| apollia_home.join("models"));
    if let Err(e) = std::fs::create_dir_all(&models_dir) {
        return emit_llm_error(format!("create {} failed: {e}", models_dir.display()), json);
    }

    let file_name = match model.file_name().and_then(|s| s.to_str()) {
        Some(n) => n.to_string(),
        None => return emit_llm_error("invalid model filename".into(), json),
    };
    let dest = models_dir.join(&file_name);
    // Guard against copying the model onto itself. A naive `dest != model` path
    // compare misses the case where `models_dir` resolves (e.g. via a symlink)
    // to the directory already holding the source: the two paths differ as
    // strings but point at the same file, and std::fs::copy opens the
    // destination for writing (truncating it) before reading the source,
    // zeroing the model. Compare canonicalized paths so an already-present
    // source is never destroyed.
    let same_file = std::fs::canonicalize(model)
        .ok()
        .zip(std::fs::canonicalize(&dest).ok())
        .map(|(src, dst)| src == dst)
        .unwrap_or(false);
    if !same_file {
        if let Err(e) = std::fs::copy(model, &dest) {
            return emit_llm_error(
                format!("copy {} → {}: {e}", model.display(), dest.display()),
                json,
            );
        }
    }
    let model_path_storage = format!("~/.apollia/models/{file_name}");

    let system_db_path = system_db_override
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| apollia_core::paths::DataFile::System.path(&apollia_home));
    if let Some(parent) = system_db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let quantization = infer_quantization(&file_name);

    let backend_config = apollia_core::LlmBackendConfig {
        name: backend_name.to_string(),
        provider: apollia_core::LlmProvider::LlamaCpp,
        model: model_path_storage.clone(),
        config_json: serde_json::json!({
            "model_path": model_path_storage,
            "device": device,
            "quantization": quantization,
        }),
        enabled: true,
        is_default: true,
    };

    let repo = match apollia_core::LlmBackendRepository::open(&system_db_path) {
        Ok(r) => r,
        Err(e) => {
            return emit_llm_error(
                format!("open {} failed: {e}", system_db_path.display()),
                json,
            );
        }
    };
    if let Err(e) = repo.save(&backend_config) {
        return emit_llm_error(format!("save backend failed: {e}"), json);
    }

    if json {
        let body = serde_json::json!({
            "backend_name": backend_name,
            "model_path": dest.display().to_string(),
            "model_path_storage": model_path_storage,
            "device": device,
            "quantization": quantization,
            "system_db": system_db_path.display().to_string(),
            "is_default": true,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    } else {
        println!("  * local LLM backend '{backend_name}' configured");
        println!("    model    : {}", dest.display());
        println!("    device   : {device}");
        println!("    quant.   : {quantization}");
        println!("    system   : {}", system_db_path.display());
        println!("    -> run `apollia-os llm reload` (or restart the daemon) to make it live.");
    }
    exit_codes::SUCCESS
}

/// Validate the `--local` flag and the supplied model file.
///
/// Returns `Err(exit_code)` when the caller should bail out early.
pub(super) fn validate_setup_model(
    local: bool,
    model: &std::path::Path,
    json: bool,
) -> Result<(), i32> {
    if !local {
        return Err(emit_llm_error(
            "only --local is supported in v0.1.0 (declare a cloud provider with `llm backends create --api-key`)".into(),
            json,
        ));
    }
    if !model.exists() {
        return Err(emit_llm_error(
            format!("model file not found: {}", model.display()),
            json,
        ));
    }
    let extension_ok = model
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("gguf"))
        .unwrap_or(false);
    if !extension_ok {
        return Err(emit_llm_error(
            format!("expected a .gguf file, got {}", model.display()),
            json,
        ));
    }
    Ok(())
}

/// Infer the GGUF quantization tag from a filename (best-effort).
///
/// Mirrors the helper in `apollia-desktop/src/commands/config.rs` so the CLI
/// and Desktop classify the same file the same way.
pub(super) fn infer_quantization(file_name: &str) -> String {
    let upper = file_name.to_uppercase();
    let patterns = [
        "Q8_0", "Q6_K", "Q5_K_M", "Q5_K_S", "Q5_0", "Q4_K_M", "Q4_K_S", "Q4_0", "Q3_K_M", "Q3_K_S",
        "Q2_K", "IQ4_XS", "IQ3_M", "IQ2_S", "F16", "F32",
    ];
    for p in &patterns {
        if upper.contains(p) {
            return p.to_lowercase();
        }
    }
    "q4_k_m".to_string()
}

/// Render `GET /api/v1/llm/costs` as a human-readable table.
pub(super) fn format_llm_costs(resp: &serde_json::Value) {
    // The cost endpoint returns `rows` (one per backend), each with call_count,
    // total_tokens, and total_cost_usd. Local usage has zero cost, so we show
    // token usage regardless of cost rather than dropping it.
    let rows = resp
        .get("rows")
        .and_then(|b| b.as_array())
        .cloned()
        .unwrap_or_default();

    println!(
        "  {:<18} {:<22} {:>8} {:>12} {:>12}",
        "BACKEND", "MODEL", "CALLS", "TOKENS", "COST ($)"
    );

    if rows.is_empty() {
        println!("  (no usage recorded)");
    } else {
        let mut total_calls = 0u64;
        let mut total_tokens = 0u64;
        let mut total_cost = 0.0f64;
        for b in &rows {
            let name = b.get("backend").and_then(|v| v.as_str()).unwrap_or("?");
            let model = b.get("model").and_then(|v| v.as_str()).unwrap_or("?");
            let calls = b.get("call_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let tokens = b.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let cost = b
                .get("total_cost_usd")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            total_calls += calls;
            total_tokens += tokens;
            total_cost += cost;
            println!("  {name:<18} {model:<22} {calls:>8} {tokens:>12} {cost:>12.4}");
        }
        println!(
            "  {:<18} {:<22} {:>8} {:>12} {:>12.4}",
            "TOTAL", "", total_calls, total_tokens, total_cost
        );
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_threshold_file_is_the_one_the_runtime_reads() {
        // GIVEN no explicit --config override on `llm costs`
        // WHEN the threshold path is resolved
        let costs_path = resolve_apollia_toml(None);

        // THEN it is the very file the runtime walks to at startup, otherwise
        // the threshold is written where nothing ever reads it
        let runtime_path = crate::commands::config::resolve_path(None);
        assert_eq!(
            costs_path, runtime_path,
            "llm costs writes to a file the runtime never loads"
        );
    }

    #[test]
    fn cost_threshold_override_is_honoured_verbatim() {
        // GIVEN an explicit --config override
        let explicit = std::path::Path::new("/tmp/apollia-costs-override.toml");

        // WHEN the threshold path is resolved
        let resolved = resolve_apollia_toml(Some(explicit));

        // THEN the override is used as given, with no search of its own
        assert_eq!(resolved, explicit.to_path_buf());
    }
}
