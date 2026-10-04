//! `apollia-os config keys`, and the default file every `config` command
//! writes when `~/.apollia/apollia.toml` is absent.

use std::path::Path;

use apollia_core::config::keys::{ConfigKey, KeyType, ReloadClass, KEYS};

use crate::exit_codes;

/// Writes the default `apollia.toml` at its one location when it is absent.
///
/// Only the canonical file is written: a path given with `--file` is the
/// caller's own and is left as it is. A failure is reported and the command
/// goes on, since every command already handles an absent file.
pub(super) fn ensure_default_file(file: Option<&Path>) {
    if file.is_some() {
        return;
    }
    ensure_default_file_at(&super::resolve_path(None));
}

/// [`ensure_default_file`] at an explicit location, for a test under a
/// throwaway home.
pub(super) fn ensure_default_file_at(path: &Path) {
    // A written file is logged by `config.default_file.written`.
    match apollia_core::config::write_default_file_if_absent(path) {
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "config.default_file.write_failed");
            if !crate::output::is_quiet() {
                eprintln!(
                    "warning: could not write the default configuration file at {}: {e}",
                    path.display()
                );
            }
        }
    }
}

/// Name of a key type, as `config keys` prints it.
pub(super) fn type_name(ty: KeyType) -> String {
    match ty {
        KeyType::Boolean => "boolean".to_owned(),
        KeyType::Integer => "integer".to_owned(),
        KeyType::Float => "float".to_owned(),
        KeyType::String => "string".to_owned(),
        KeyType::Path => "path".to_owned(),
        KeyType::Enum(values) => format!("one of {}", values.join(", ")),
        KeyType::StringArray => "string array".to_owned(),
        KeyType::TableArray => "table array".to_owned(),
        KeyType::Table => "table".to_owned(),
    }
}

fn reload_name(reload: ReloadClass) -> &'static str {
    match reload {
        ReloadClass::Reloadable => "reloadable",
        ReloadClass::Restart => "restart",
    }
}

/// One registry entry as `config keys --json` renders it.
pub(super) fn key_json(key: &ConfigKey) -> serde_json::Value {
    serde_json::json!({
        "path": key.path,
        "type": type_name(key.ty),
        "default": key.default,
        "doc": key.doc,
        "reload": reload_name(key.reload),
        "lockable": key.lockable,
        "withdrawn": key.withdrawn.is_some(),
        "withdrawn_detail": key.withdrawn,
    })
}

/// Prints the key registry: every key of `apollia.toml`, live and withdrawn.
pub(super) fn run_keys(json: bool) -> i32 {
    if json {
        let keys: Vec<serde_json::Value> = KEYS.iter().map(key_json).collect();
        let body = serde_json::json!({ "keys": keys });
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
        return exit_codes::SUCCESS;
    }
    for key in KEYS {
        if let Some(detail) = key.withdrawn {
            println!("{}  withdrawn: {detail}", key.path);
            continue;
        }
        let default = key.default.unwrap_or("not set");
        let lock = if key.lockable { ", lockable" } else { "" };
        println!(
            "{}  {}  default {}  ({}{lock})",
            key.path,
            type_name(key.ty),
            default,
            reload_name(key.reload)
        );
        println!("    {}", key.doc);
    }
    exit_codes::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_json_listing_carries_every_registry_entry_and_its_fields() {
        // GIVEN the registry
        // WHEN each entry is rendered for `config keys --json`
        let rendered: Vec<serde_json::Value> = KEYS.iter().map(key_json).collect();

        // THEN there is one object per entry, with path, type, default, reload
        //      class, lockable and withdrawn
        assert_eq!(rendered.len(), KEYS.len());
        let port = rendered.iter().find(|k| k["path"] == "api.port").unwrap();
        assert_eq!(port["type"], "integer");
        assert_eq!(port["default"], "7771");
        assert_eq!(port["reload"], "restart");
        assert_eq!(port["lockable"], false);
        assert_eq!(port["withdrawn"], false);
        let developer = rendered
            .iter()
            .find(|k| k["path"] == "desktop.developer_mode")
            .unwrap();
        assert_eq!(developer["lockable"], true);
        let oria = rendered.iter().find(|k| k["path"] == "oria").unwrap();
        assert_eq!(oria["withdrawn"], true);
    }

    #[test]
    fn an_absent_canonical_file_is_written_and_a_present_one_kept() {
        // GIVEN a throwaway home with no data folder
        let home = tempfile::tempdir().unwrap();
        let path = apollia_core::paths::config_file_under(home.path());

        // WHEN a config command starts
        ensure_default_file_at(&path);

        // THEN the default file is there with its header
        let first = std::fs::read(&path).unwrap();
        assert!(String::from_utf8_lossy(&first).starts_with("# apollia.toml"));

        // AND GIVEN the user then edits it
        std::fs::write(&path, "[api]\nbind = \"0.0.0.0\"\n").unwrap();
        // WHEN the next config command starts
        ensure_default_file_at(&path);
        // THEN its bytes are unchanged
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"[api]\nbind = \"0.0.0.0\"\n"
        );
    }
}
