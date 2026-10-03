//! `apollia-os agent export <id> [--sign] [--publisher NAME] [-o FILE]`: an
//! agent created in the interface, as a `.apo` package.
//!
//! The runtime builds the archive (`POST /api/v1/agent-definitions/{id}/export`)
//! and signs it with the author key of the secret store when `--sign` is
//! given; this command writes the file. The publisher is the `author.name`
//! setting (`apollia-os config set author.name "<name>"`), or `--publisher`
//! for this export only.

use std::path::{Path, PathBuf};

use super::{handle_error, print_compact_error_and_exit};
use crate::client::{ClientError, RuntimeClient};
use crate::exit_codes;

/// Where the archive goes: `output` when given, else the file name the
/// runtime offered in the current folder.
fn target_of(output: Option<&Path>, offered: &str) -> PathBuf {
    output.map_or_else(|| PathBuf::from(offered), Path::to_path_buf)
}

/// Refuses an output whose folder does not exist, before the runtime is
/// reached: the archive would be built for nothing.
fn check_output(output: Option<&Path>) -> Result<(), String> {
    let Some(output) = output else {
        return Ok(());
    };
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if parent.is_dir() {
        Ok(())
    } else {
        Err(format!("folder not found: {}", parent.display()))
    }
}

/// The choices of `apollia-os agent export`.
pub(in crate::commands::agent) struct ExportChoices<'a> {
    /// Sign with the author key.
    pub sign: bool,
    /// Publisher for this export only.
    pub publisher: Option<&'a str>,
    /// File to write.
    pub output: Option<&'a Path>,
}

/// `apollia-os agent export <agent_id>`.
pub(in crate::commands::agent) async fn run_export(
    client: &RuntimeClient,
    agent_id: &str,
    choices: ExportChoices<'_>,
    json: bool,
) -> i32 {
    let ExportChoices {
        sign,
        publisher,
        output,
    } = choices;
    if let Err(msg) = check_output(output) {
        return print_compact_error_and_exit(&msg, json);
    }
    let exported = match client.export_agent(agent_id, sign, publisher).await {
        Ok(exported) => exported,
        Err(e) => return export_error(e, agent_id, json),
    };
    let offered = exported
        .report
        .get("file_name")
        .and_then(|v| v.as_str())
        .map_or_else(|| format!("{agent_id}.apo"), str::to_owned);
    let target = target_of(output, &offered);
    if let Err(e) = std::fs::write(&target, &exported.bytes) {
        return print_compact_error_and_exit(&format!("{}: {e}", target.display()), json);
    }

    let report = &exported.report;
    if json {
        let mut out = report.clone();
        if let Some(object) = out.as_object_mut() {
            object.insert("path".into(), target.display().to_string().into());
            object.insert("bytes".into(), exported.bytes.len().into());
        }
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        return exit_codes::SUCCESS;
    }
    println!("{}", target.display());
    let publisher = report
        .get("publisher")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match report.get("key_fingerprint").and_then(|v| v.as_str()) {
        Some(fingerprint) => {
            crate::note!("  signed by {publisher} (key {fingerprint})");
        }
        None => crate::note!("  unsigned, publisher {publisher}"),
    }
    let listed = |key: &str| -> Vec<String> {
        report
            .get(key)
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let required = listed("required_connections");
    if !required.is_empty() {
        crate::note!("  requires the connections: {}", required.join(", "));
    }
    let removed = listed("removed_access");
    if !removed.is_empty() {
        crate::note!(
            "  left out (another package, agent or workflow): {}",
            removed.join(", ")
        );
    }
    exit_codes::SUCCESS
}

/// An unknown agent, an agent that cannot be exported or a missing publisher
/// is a usage error (exit 1); an unreachable runtime is exit 2.
fn export_error(err: ClientError, agent_id: &str, json: bool) -> i32 {
    match err {
        ClientError::ServerError { status: 404, .. } => {
            print_compact_error_and_exit(&format!("agent not found: {agent_id}"), json)
        }
        ClientError::ServerError {
            status: 409 | 422,
            body,
        } => print_compact_error_and_exit(&body, json),
        other => handle_error(other, json),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_is_the_offered_name_unless_an_output_is_given() {
        // GIVEN the name the runtime offered
        let offered = "veille-1.0.0.apo";

        // WHEN the target is chosen without and with an output
        let default = target_of(None, offered);
        let chosen = target_of(Some(Path::new("/tmp/out.apo")), offered);

        // THEN the offered name is used in the current folder, else the output
        assert_eq!(default, PathBuf::from("veille-1.0.0.apo"));
        assert_eq!(chosen, PathBuf::from("/tmp/out.apo"));
    }

    #[test]
    fn test_an_output_in_a_missing_folder_is_refused() {
        // GIVEN an output in a folder that exists, a bare name, and one in a
        // folder that does not
        let dir = tempfile::tempdir().unwrap();
        let present = dir.path().join("a.apo");
        let missing = dir.path().join("nope").join("a.apo");

        // WHEN each is checked
        // THEN only the missing folder is refused, by name
        assert!(check_output(Some(&present)).is_ok());
        assert!(check_output(Some(Path::new("a.apo"))).is_ok());
        assert!(check_output(None).is_ok());
        let refused = check_output(Some(&missing)).unwrap_err();
        assert!(refused.contains("nope"), "{refused}");
    }
}
