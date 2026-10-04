//! What changed between two configurations, key by key, and when each change
//! takes effect according to the key registry.

use std::collections::{BTreeMap, BTreeSet};

use apollia_core::config::keys::{ReloadClass, KEYS};
use apollia_core::config::ApolliaConfig;
use serde_json::Value;

/// Every leaf of `config` by its dotted path: a scalar, an array or an empty
/// table. An array is one leaf, as the registry declares it.
fn leaves(config: &ApolliaConfig) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    if let Ok(Value::Object(root)) = serde_json::to_value(config) {
        for (key, value) in root {
            flatten(key, value, &mut out);
        }
    }
    out
}

fn flatten(path: String, value: Value, out: &mut BTreeMap<String, Value>) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (key, child) in map {
                flatten(format!("{path}.{key}"), child, out);
            }
        }
        other => {
            out.insert(path, other);
        }
    }
}

/// The dotted keys whose value differs between `before` and `after`, in key
/// order.
pub(super) fn changed_keys(before: &ApolliaConfig, after: &ApolliaConfig) -> Vec<String> {
    let before = leaves(before);
    let after = leaves(after);
    let paths: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    paths
        .into_iter()
        .filter(|path| before.get(*path) != after.get(*path))
        .cloned()
        .collect()
}

/// When a new value of the dotted `key` takes effect: the class of the
/// registry entry that declares it or the nearest one above it. A key the
/// registry does not place waits for a restart, the safe answer.
pub(super) fn class_of(key: &str) -> ReloadClass {
    KEYS.iter()
        .filter(|k| k.withdrawn.is_none())
        .filter(|k| {
            key == k.path
                || key
                    .strip_prefix(k.path)
                    .is_some_and(|rest| rest.starts_with('.'))
        })
        .max_by_key(|k| k.path.len())
        .map_or(ReloadClass::Restart, |k| k.reload)
}

/// The first segment of a dotted key: its section.
pub(super) fn section_of(key: &str) -> &str {
    key.split('.').next().unwrap_or(key)
}

/// The changes between two configurations, split by when they take effect.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Split {
    /// The sections with at least one changed key the running engine applies.
    pub sections: Vec<String>,
    /// The changed keys that wait for a restart.
    pub restart: Vec<String>,
}

/// Splits the changes from `before` to `after` by their class.
pub(super) fn split(before: &ApolliaConfig, after: &ApolliaConfig) -> Split {
    let mut sections = BTreeSet::new();
    let mut restart = Vec::new();
    for key in changed_keys(before, after) {
        match class_of(&key) {
            ReloadClass::Reloadable => {
                sections.insert(section_of(&key).to_owned());
            }
            ReloadClass::Restart => restart.push(key),
        }
    }
    Split {
        sections: sections.into_iter().collect(),
        restart,
    }
}
