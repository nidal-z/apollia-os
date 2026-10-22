//! The secrets a server configuration carries in clear, filed into the secret
//! store.
//!
//! A configuration reaches the runtime with its env values as typed. Two kinds
//! of value are secrets and never stay in `mcp.db`: every literal header value
//! of a server reached at an address (its env entries are its HTTP headers),
//! and every literal value of a variable whose name marks it secret (a key, a
//! token, a password). Each is written to the store under
//! [`static_secret_user`] and replaced by the `${APOLLIA_SECRET:KEY}`
//! placeholder the session resolves at start.
//!
//! A value holding any `${...}` placeholder is not literal and is kept as is.

use std::collections::BTreeSet;

use apollia_auth::SecretStore;
use thiserror::Error;

use crate::config::{static_secret_user, McpServerConfig, MCP_STATIC_SECRET_SERVICE};

/// The name fragments that mark a variable secret, compared without case.
const SECRET_NAME_MARKS: &[&str] = &[
    "KEY",
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "PASSWD",
    "PASS",
    "PWD",
    "AUTH",
    "CREDENTIAL",
    "BEARER",
    "COOKIE",
    "SESSION",
    "PRIVATE",
];

/// A secret could not be written to the store.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SecretFilingError {
    /// The store refused the value of the variable `var`.
    #[error("the secret of '{var}' could not be stored")]
    Store {
        /// The env variable or header the value belongs to.
        var: String,
        /// What the store answered.
        #[source]
        source: apollia_auth::AuthError,
    },
}

/// Whether the name of a variable marks its value secret.
#[must_use]
pub fn is_secret_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    SECRET_NAME_MARKS.iter().any(|mark| upper.contains(mark))
}

/// Whether `value` holds no placeholder and something to hide.
fn is_literal(value: &str) -> bool {
    !value.trim().is_empty() && !value.contains("${")
}

/// Whether the env entries of `config` are HTTP headers.
fn carries_headers(config: &McpServerConfig) -> bool {
    config.transport != "stdio"
}

/// Whether the value of `var` in `config` is a secret kept in clear.
fn is_literal_secret(config: &McpServerConfig, var: &str, value: &str) -> bool {
    is_literal(value) && (carries_headers(config) || is_secret_name(var))
}

/// The variables of `config` whose value is a secret kept in clear, sorted.
#[must_use]
pub fn literal_secret_vars(config: &McpServerConfig) -> Vec<String> {
    let mut vars: Vec<String> = config
        .env
        .iter()
        .filter(|(var, value)| is_literal_secret(config, var, value))
        .map(|(var, _)| var.clone())
        .collect();
    vars.sort();
    vars
}

/// The key a secret of the variable `var` is first proposed under: its name,
/// with every character a placeholder cannot carry turned into `_`.
fn key_for(var: &str) -> String {
    let key: String = var
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if key.is_empty() {
        "SECRET".to_owned()
    } else {
        key
    }
}

fn placeholder(key: &str) -> String {
    format!("${{APOLLIA_SECRET:{key}}}")
}

/// `config` as it may be shown: every secret kept in clear replaced by the
/// placeholder of its variable. A value already a placeholder is unchanged.
#[must_use]
pub fn masked(config: &McpServerConfig) -> McpServerConfig {
    let mut shown = config.clone();
    for var in literal_secret_vars(config) {
        shown.env.insert(var.clone(), placeholder(&key_for(&var)));
    }
    shown
}

/// Puts back, in `config`, the stored value of each variable a client sent
/// back masked: a variable whose value in `stored` is a secret kept in clear,
/// and whose value in `config` is the placeholder [`masked`] showed for it.
pub fn restore_masked(config: &mut McpServerConfig, stored: &McpServerConfig) {
    for var in literal_secret_vars(stored) {
        let shown = placeholder(&key_for(&var));
        if config.env.get(&var) == Some(&shown) {
            if let Some(value) = stored.env.get(&var) {
                config.env.insert(var, value.clone());
            }
        }
    }
}

/// The key of the placeholder `value` is made of alone, if it is one.
fn sole_secret_key(value: &str) -> Option<&str> {
    value
        .strip_prefix("${APOLLIA_SECRET:")?
        .strip_suffix('}')
        .filter(|key| !key.is_empty() && !key.contains('}'))
}

/// Files every secret `config` keeps in clear into `store`, and replaces each
/// with its placeholder. Returns the variables moved, sorted.
///
/// A variable that `previous` (the configuration it replaces) already
/// pointed at a key keeps that key, so a changed value overwrites the old
/// one. Otherwise the key is the variable's name, made unique among the keys
/// the configuration names. Stops at the first value the store refuses, with
/// `config` holding the values moved before it.
pub fn file_literal_secrets(
    config: &mut McpServerConfig,
    store: &dyn SecretStore,
    previous: Option<&McpServerConfig>,
) -> Result<Vec<String>, SecretFilingError> {
    let vars = literal_secret_vars(config);
    if vars.is_empty() {
        return Ok(vars);
    }
    let mut taken: BTreeSet<String> = config.static_secret_keys().into_iter().collect();
    for var in &vars {
        let kept = previous
            .and_then(|p| p.env.get(var))
            .and_then(|value| sole_secret_key(value))
            .filter(|key| !taken.contains(*key))
            .map(str::to_owned);
        let key = match kept {
            Some(key) => key,
            None => unique_key(&key_for(var), &taken),
        };
        let Some(value) = config.env.get(var).cloned() else {
            continue;
        };
        store
            .set(
                MCP_STATIC_SECRET_SERVICE,
                &static_secret_user(&config.name, &key),
                &value,
            )
            .map_err(|source| SecretFilingError::Store {
                var: var.clone(),
                source,
            })?;
        config.env.insert(var.clone(), placeholder(&key));
        taken.insert(key);
    }
    Ok(vars)
}

/// `base`, or `base_2`, `base_3`, ..., the first one `taken` does not hold.
fn unique_key(base: &str, taken: &BTreeSet<String>) -> String {
    if !taken.contains(base) {
        return base.to_owned();
    }
    (2u32..)
        .map(|n| format!("{base}_{n}"))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or_else(|| base.to_owned())
}

#[cfg(test)]
#[path = "secret_filing/tests.rs"]
mod tests;
