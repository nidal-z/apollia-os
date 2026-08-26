//! `tools config` and `tools reload`: read and write `apollia.toml`.

use apollia_core::{ToolsConfig, WebSearchBackend};
use toml_edit::{DocumentMut, Item, Value};

use crate::exit_codes;
use crate::note;

use super::support::{
    emit_error, emit_unknown_tool, is_known_tool, load_tools_config, read_or_empty,
    resolve_data_dir, resolve_writable_config_path, write_config_file,
};
use super::ToolsConfigCmd;

// ─── Config ───────────────────────────────────────────────────────────

pub(super) fn run_config(cmd: &ToolsConfigCmd, json: bool) -> i32 {
    match cmd {
        ToolsConfigCmd::Get { name } => run_config_get(name, json),
        ToolsConfigCmd::Set { key_path, value } => run_config_set(key_path, value, json),
    }
}

pub(super) fn run_config_get(name: &str, json: bool) -> i32 {
    if !is_known_tool(name) {
        return emit_unknown_tool(name, json);
    }
    let cfg = load_tools_config(json);
    let value = effective_config_for(name, &cfg);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
        );
    } else {
        let toml_value = json_to_toml_value(&value);
        let mut doc = DocumentMut::new();
        let header = format!("tools.{name}");
        doc[&header] = toml_edit::Item::Value(toml_value);
        let s = doc.to_string();
        let trimmed = s.trim_start_matches('\n');
        print!("{trimmed}");
    }
    exit_codes::SUCCESS
}

pub(super) fn effective_config_for(name: &str, cfg: &ToolsConfig) -> serde_json::Value {
    match name {
        "web_search" => serde_json::json!({
            "backend": backend_to_str(cfg.web_search.backend),
            "require_configured": cfg.web_search.require_configured,
            "brave": {
                "api_key_env_var": cfg.web_search.brave.api_key_env_var,
                "timeout_secs": cfg.web_search.brave.timeout_secs,
                "max_results": cfg.web_search.brave.max_results,
            },
            "duckduckgo": {
                "timeout_secs": cfg.web_search.duckduckgo.timeout_secs,
                "max_response_kb": cfg.web_search.duckduckgo.max_response_kb,
            },
        }),
        "web_read" => serde_json::json!({
            "timeout_secs": cfg.web_read.timeout_secs,
            "max_response_kb": cfg.web_read.max_response_kb,
            "ssrf_guard": cfg.web_read.ssrf_guard,
        }),
        _ => serde_json::Value::Object(serde_json::Map::new()),
    }
}

pub(super) fn backend_to_str(b: WebSearchBackend) -> &'static str {
    match b {
        WebSearchBackend::Auto => "auto",
        WebSearchBackend::DuckDuckGo => "duckduckgo",
        WebSearchBackend::Brave => "brave",
    }
}

pub(super) fn json_to_toml_value(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::from(""),
        serde_json::Value::Bool(b) => Value::from(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(f) = n.as_f64() {
                Value::from(f)
            } else {
                Value::from(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::from(s.as_str()),
        serde_json::Value::Array(a) => {
            let arr: toml_edit::Array = a.iter().map(json_to_toml_value).collect();
            Value::Array(arr)
        }
        serde_json::Value::Object(o) => {
            let mut tbl = toml_edit::InlineTable::new();
            for (k, val) in o {
                tbl.insert(k, json_to_toml_value(val));
            }
            Value::InlineTable(tbl)
        }
    }
}

pub(super) fn run_config_set(key_path: &str, value: &str, json: bool) -> i32 {
    let parts: Vec<&str> = key_path.split('.').collect();
    if parts.len() < 2 {
        return emit_error(
            format!("invalid key '{key_path}' - expected format: <tool>.<key>"),
            json,
        );
    }
    let tool = parts[0];
    let key_segments = &parts[1..];

    let parsed = match parse_value_for(tool, key_segments, value) {
        Ok(v) => v,
        Err(msg) => return emit_error(msg, json),
    };

    let path = match resolve_writable_config_path() {
        Ok(p) => p,
        Err(code) => return code,
    };

    let original = match read_or_empty(&path) {
        Ok(s) => s,
        Err(e) => return emit_error(format!("read {} failed: {e}", path.display()), json),
    };
    let mut doc: DocumentMut = match original.parse() {
        Ok(d) => d,
        Err(e) => return emit_error(format!("invalid TOML in {}: {e}", path.display()), json),
    };

    set_nested_value(&mut doc, &["tools", tool], key_segments, parsed);

    let written = doc.to_string();

    if let Err(e) = write_config_file(&path, &written) {
        return emit_error(format!("write {} failed: {e}", path.display()), json);
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "key": key_path,
                "value": value,
                "path": path.display().to_string(),
            }))
            .unwrap_or_default()
        );
    } else {
        note!("✔ {} → {} ({})", key_path, value, path.display());
    }
    exit_codes::SUCCESS
}

pub(super) fn parse_value_for(
    tool: &str,
    key_segments: &[&str],
    raw: &str,
) -> Result<Value, String> {
    let key = key_segments.join(".");
    match (tool, key.as_str()) {
        ("web_search", "backend") => match raw {
            "auto" | "duckduckgo" | "brave" => Ok(Value::from(raw)),
            _ => Err(format!(
                "invalid value '{raw}' for web_search.backend - expected: auto | duckduckgo | brave"
            )),
        },
        ("web_search", "require_configured") => parse_bool(raw),
        ("web_search", "brave.api_key_env_var") => Ok(Value::from(raw)),
        ("web_search", "brave.timeout_secs") => parse_int_in(raw, 1, 120),
        ("web_search", "brave.max_results") => parse_int_in(raw, 1, 20),
        ("web_search", "duckduckgo.timeout_secs") => parse_int_in(raw, 1, 120),
        ("web_search", "duckduckgo.max_response_kb") => parse_int_in(raw, 16, 16_384),
        ("web_read", "timeout_secs") => parse_int_in(raw, 1, 120),
        ("web_read", "max_response_kb") => parse_int_in(raw, 64, 32_768),
        ("web_read", "ssrf_guard") => parse_bool(raw),
        _ => Err(format!(
            "unknown key '{tool}.{key}'\n{}",
            valid_keys_help(tool)
        )),
    }
}

pub(super) fn valid_keys_help(tool: &str) -> String {
    match tool {
        "web_search" => "valid keys: backend, require_configured, brave.api_key_env_var, \
                         brave.timeout_secs, brave.max_results, duckduckgo.timeout_secs, \
                         duckduckgo.max_response_kb"
            .to_string(),
        "web_read" => "valid keys: timeout_secs, max_response_kb, ssrf_guard".to_string(),
        _ => {
            "tool with no TOML configuration - configurable tools: web_search, web_read".to_string()
        }
    }
}

pub(super) fn parse_bool(raw: &str) -> Result<Value, String> {
    match raw {
        "true" => Ok(Value::from(true)),
        "false" => Ok(Value::from(false)),
        _ => Err(format!("boolean expected (true|false), got '{raw}'")),
    }
}

pub(super) fn parse_int_in(raw: &str, min: i64, max: i64) -> Result<Value, String> {
    let n: i64 = raw
        .parse()
        .map_err(|_| format!("integer expected, got '{raw}'"))?;
    if n < min || n > max {
        return Err(format!("value {n} out of range [{min}, {max}]"));
    }
    Ok(Value::from(n))
}

pub(super) fn set_nested_value(
    doc: &mut DocumentMut,
    table_path: &[&str],
    leaf_segments: &[&str],
    value: Value,
) {
    let table = ensure_table(doc.as_table_mut(), table_path);
    let (last, parents) = match leaf_segments.split_last() {
        Some(s) => s,
        None => return,
    };
    let target = ensure_subtable(table, parents);
    target[last] = Item::Value(value);
}

pub(super) fn ensure_table<'a>(
    root: &'a mut toml_edit::Table,
    path: &[&str],
) -> &'a mut toml_edit::Table {
    let mut current = root;
    for segment in path {
        if !current.contains_key(segment) {
            let mut t = toml_edit::Table::new();
            t.set_implicit(false);
            current[segment] = Item::Table(t);
        }
        let item = &mut current[segment];
        if !matches!(item, Item::Table(_)) {
            *item = Item::Table(toml_edit::Table::new());
        }
        current = item
            .as_table_mut()
            // SAFETY: the two branches above both leave `item` an
            // `Item::Table`, and nothing between them and here replaces it.
            .expect("the item was just matched or set as a table");
    }
    current
}

pub(super) fn ensure_subtable<'a>(
    root: &'a mut toml_edit::Table,
    path: &[&str],
) -> &'a mut toml_edit::Table {
    ensure_table(root, path)
}

// ─── Reload ───────────────────────────────────────────────────────────

pub(super) fn run_reload(json: bool) -> i32 {
    let data_dir = match resolve_data_dir() {
        Ok(d) => d,
        Err(code) => return code,
    };
    let snapshot = match apollia_tools::load_governance_snapshot(&data_dir) {
        Ok(s) => s,
        Err(e) => return emit_error(format!("snapshot reload failed: {e}"), json),
    };
    if json {
        let payload = serde_json::json!({
            "disabled_tools": snapshot.disabled_tools,
            "brave_api_key_present": snapshot.brave_api_key.is_some(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_default()
        );
    } else {
        note!("✔ Governance snapshot reloaded");
        if snapshot.disabled_tools.is_empty() {
            println!("  Disabled tools    : (none)");
        } else {
            println!(
                "  Disabled tools    : {}",
                snapshot.disabled_tools.join(", ")
            );
        }
        let brave = if snapshot.brave_api_key.is_some() {
            "present"
        } else {
            "absent"
        };
        println!("  Brave API key     : {brave}");
        note!(
            "  Note: the runtime rereads this snapshot on every agent run - \
             no restart required."
        );
    }
    exit_codes::SUCCESS
}
