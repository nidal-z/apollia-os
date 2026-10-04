//! The `apollia.toml` written on a first start.
//!
//! Generated from the key registry ([`keys`](super::keys)): a header, then
//! every section in registry order, each key under its documentation and
//! written as a commented line holding its default. A commented line is worth
//! its default and follows the defaults of later versions; an uncommented one
//! is the user's choice. `[llm]` carries a sentence and no key line: its
//! backends live in the engine's database and are managed from the AI
//! settings, and two of its keys have no default, so no single line of it could
//! be uncommented on its own.

use std::path::Path;

use super::keys::{ConfigKey, KEYS};

const HEADER: &str = "\
# apollia.toml, the configuration of Apollia OS.
#
# Every setting is listed below, each under what it does, with its default.
# A commented line is worth its default value, and follows the defaults of
# later versions. To change a setting, remove the \"# \" in front of its value
# and edit the value; to return to the default, comment the line again.
#
# Reference of every key: https://docs.apollia.fr/reference/configuration
# Check the file with: apollia-os config validate
";

const LLM_SECTION: &str = "\
# [llm]
# The language models are chosen and configured in the AI settings of the
# application, which keep them in the engine's database. Nothing here is
# needed. The reference page describes how a backend can be declared in this
# file instead.
";

/// The text of the default `apollia.toml`, the Unix socket at its default.
///
/// The text validates through [`validate`](super::validate) and gives
/// [`ApolliaConfig::default()`](super::ApolliaConfig), and every value line,
/// uncommented on its own, still validates.
pub fn default_file_text() -> String {
    default_file_text_with_socket(&crate::paths::socket_path_or_temp())
}

/// [`default_file_text`] with the default of `api.unix_socket` given: the
/// default is an absolute path under the home folder, which the file has to
/// spell out since the key does not expand `~`.
pub(crate) fn default_file_text_with_socket(socket: &Path) -> String {
    let mut out = String::from(HEADER);
    for section in sections() {
        if section == "llm" {
            out.push('\n');
            out.push_str(LLM_SECTION);
            continue;
        }
        for (table, keys) in tables_of(section) {
            out.push_str(&format!("\n[{table}]\n"));
            for key in keys {
                out.push('\n');
                push_key(&mut out, &table, key, socket);
            }
        }
    }
    out
}

/// Writes the default `apollia.toml` at `path` when no file is there, creating
/// its folder, through the configuration writer. Never writes over an existing file, even one created between
/// the check and the write.
///
/// Returns `true` when the file was written, `false` when one was already
/// there.
///
/// # Errors
///
/// The I/O error of creating the folder or writing the file.
pub fn write_default_file_if_absent(path: &Path) -> std::io::Result<bool> {
    let written = super::writer::create_if_absent(path, &default_file_text())?;
    if written {
        tracing::event!(
            tracing::Level::INFO,
            path = %path.display(),
            "config.default_file.written"
        );
    }
    Ok(written)
}

/// The top-level sections of the live keys, in registry order.
fn sections() -> Vec<&'static str> {
    let mut seen: Vec<&'static str> = Vec::new();
    for key in live_keys() {
        let section = key.path.split('.').next().unwrap_or(key.path);
        if !seen.contains(&section) {
            seen.push(section);
        }
    }
    seen
}

/// The tables of one section and their keys, the section's own keys first,
/// then each sub-table in registry order. A TOML table is declared once, so a
/// key of the section listed after a sub-table is moved up with the others.
fn tables_of(section: &str) -> Vec<(String, Vec<&'static ConfigKey>)> {
    let mut tables: Vec<(String, Vec<&'static ConfigKey>)> = vec![(section.to_owned(), Vec::new())];
    for key in live_keys().filter(|k| k.path.split('.').next() == Some(section)) {
        let table = key
            .path
            .rsplit_once('.')
            .map_or(section, |(table, _)| table);
        match tables.iter_mut().find(|(t, _)| t == table) {
            Some((_, keys)) => keys.push(key),
            None => tables.push((table.to_owned(), vec![key])),
        }
    }
    tables
}

fn live_keys() -> impl Iterator<Item = &'static ConfigKey> {
    KEYS.iter().filter(|k| k.withdrawn.is_none())
}

fn push_key(out: &mut String, table: &str, key: &ConfigKey, socket: &Path) {
    let name = key
        .path
        .strip_prefix(table)
        .and_then(|rest| rest.strip_prefix('.'))
        .unwrap_or(key.path);
    for line in wrap(key.doc, 76) {
        out.push_str("# ");
        out.push_str(&line);
        out.push('\n');
    }
    match default_literal(key, socket) {
        Some(value) => out.push_str(&format!("# {name} = {value}\n")),
        None => out.push_str(&format!("# {name}: not set by default.\n")),
    }
}

fn default_literal(key: &ConfigKey, socket: &Path) -> Option<String> {
    if key.path == "api.unix_socket" {
        return Some(toml::Value::String(socket.display().to_string()).to_string());
    }
    key.default.map(str::to_owned)
}

/// Splits `text` into lines of at most `width` characters, on spaces.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}
