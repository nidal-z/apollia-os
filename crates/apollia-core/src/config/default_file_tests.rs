//! The default `apollia.toml`, held to the registry and to the validation.

use std::collections::BTreeSet;

use super::default_file::default_file_text_with_socket;
use super::keys::KEYS;
use super::{default_file_text, validate, write_default_file_if_absent, ApolliaConfig};

fn json(config: &ApolliaConfig) -> serde_json::Value {
    serde_json::to_value(config).unwrap()
}

/// `(line index, dotted path)` of every commented value line, the table read
/// from the last header above it.
fn value_lines(text: &str) -> Vec<(usize, String)> {
    let mut table = String::new();
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            table = header.to_owned();
            continue;
        }
        let Some((name, _)) = line.strip_prefix("# ").and_then(|l| l.split_once(" = ")) else {
            continue;
        };
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            out.push((i, format!("{table}.{name}")));
        }
    }
    out
}

/// Dotted paths of the keys written as "not set by default".
fn unset_lines(text: &str) -> Vec<String> {
    let mut table = String::new();
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            table = header.to_owned();
        } else if let Some(name) = line
            .strip_prefix("# ")
            .and_then(|l| l.strip_suffix(": not set by default."))
        {
            out.push(format!("{table}.{name}"));
        }
    }
    out
}

#[test]
fn the_default_text_validates_and_gives_the_defaults() {
    // GIVEN the default file
    let text = default_file_text();

    // WHEN it is validated
    let config = validate(&text).unwrap();

    // THEN it is the default configuration, with no [llm]
    assert_eq!(json(&config), json(&ApolliaConfig::default()));
    assert!(config.llm.is_none());
}

#[test]
fn every_value_line_uncommented_on_its_own_stays_valid() {
    // GIVEN the default file, the socket at an existing folder as it is on a
    //       machine where the file lives next to it
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("runtime.sock");
    let text = default_file_text_with_socket(&socket);
    let lines: Vec<&str> = text.lines().collect();
    let values = value_lines(&text);
    assert!(!values.is_empty());

    // WHEN each value line is uncommented, one at a time
    let mut faults = Vec::new();
    for (index, path) in &values {
        let mut edited: Vec<String> = lines.iter().map(|l| (*l).to_owned()).collect();
        edited[*index] = lines[*index].trim_start_matches("# ").to_owned();
        match validate(&edited.join("\n")) {
            Ok(config) if path == "api.unix_socket" => {
                assert_eq!(config.api.unix_socket, socket);
            }
            // THEN the file still validates, and to the defaults
            Ok(config) => {
                if json(&config) != json(&ApolliaConfig::default()) {
                    faults.push(format!("{path}: not the default once uncommented"));
                }
            }
            Err(diagnostics) => faults.push(format!("{path}: {diagnostics:?}")),
        }
    }
    assert!(faults.is_empty(), "lines that break the file: {faults:#?}");
}

#[test]
fn every_live_key_outside_llm_is_written_once_in_registry_order() {
    // GIVEN the live keys of the registry outside [llm]
    let expected: Vec<&str> = KEYS
        .iter()
        .filter(|k| k.withdrawn.is_none() && !k.path.starts_with("llm."))
        .map(|k| k.path)
        .collect();

    // WHEN the default file is read back, value lines and unset lines together
    let text = default_file_text();
    let mut written: Vec<String> = value_lines(&text).into_iter().map(|(_, p)| p).collect();
    written.extend(unset_lines(&text));

    // THEN every key is there once, and no other
    let written_set: BTreeSet<&str> = written.iter().map(String::as_str).collect();
    assert_eq!(written.len(), written_set.len(), "a key written twice");
    let expected_set: BTreeSet<&str> = expected.iter().copied().collect();
    assert_eq!(written_set, expected_set);

    // AND the sections follow the order of the registry
    let mut registry_sections: Vec<&str> = Vec::new();
    for path in &expected {
        let section = path.split('.').next().unwrap();
        if !registry_sections.contains(&section) {
            registry_sections.push(section);
        }
    }
    let mut file_sections: Vec<&str> = Vec::new();
    for line in text.lines() {
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let section = header.split('.').next().unwrap();
            if !file_sections.contains(&section) {
                file_sections.push(section);
            }
        }
    }
    assert_eq!(file_sections, registry_sections);
}

#[test]
fn every_key_is_preceded_by_its_documentation() {
    // GIVEN the default file
    let text = default_file_text();

    // WHEN the registry documentation of each written key is looked up
    // THEN the comment block right above its line starts with that documentation
    let lines: Vec<&str> = text.lines().collect();
    for (index, path) in value_lines(&text) {
        let entry = KEYS.iter().find(|k| k.path == path).unwrap();
        let first_word = entry.doc.split_whitespace().next().unwrap();
        let mut above = index;
        while above > 0 && lines[above - 1].starts_with("# ") {
            above -= 1;
        }
        assert!(
            lines[above].starts_with(&format!("# {first_word}")),
            "{path}: documentation missing above its line"
        );
    }
}

#[test]
fn the_header_explains_the_file_and_llm_is_a_sentence_without_keys() {
    // GIVEN the default file
    let text = default_file_text();

    // WHEN its header and its [llm] part are read
    // THEN the header says what the file is, where the reference is, and what
    //      a commented line means
    assert!(text.starts_with("# apollia.toml, the configuration of Apollia OS."));
    assert!(text.contains("https://docs.apollia.fr/reference/configuration"));
    assert!(text.contains("A commented line is worth its default value"));
    // AND [llm] is entirely commented, points at the AI settings, and writes
    //     no key, since uncommenting one alone would leave [llm] incomplete
    assert!(text.contains("# [llm]\n"));
    assert!(text.contains("AI settings of the"));
    assert!(!text.lines().any(|l| l.starts_with("[llm")));
    assert!(!value_lines(&text)
        .iter()
        .any(|(_, p)| p.starts_with("llm.")));
}

#[test]
fn an_absent_file_is_written_with_its_header() {
    // GIVEN a data folder that does not exist yet
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".apollia").join("apollia.toml");

    // WHEN the default file is asked for
    let written = write_default_file_if_absent(&path).unwrap();

    // THEN it exists, holding the default text
    assert!(written);
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.starts_with("# apollia.toml, the configuration of Apollia OS."));
    assert!(validate(&content).is_ok());
}

#[test]
fn a_present_file_is_left_byte_for_byte() {
    // GIVEN a file the user wrote
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("apollia.toml");
    let mine = b"[api]\nport = 8080\n# mine\n";
    std::fs::write(&path, mine).unwrap();

    // WHEN the default file is asked for
    let written = write_default_file_if_absent(&path).unwrap();

    // THEN nothing is written and its bytes are unchanged
    assert!(!written);
    assert_eq!(std::fs::read(&path).unwrap(), mine);
}
