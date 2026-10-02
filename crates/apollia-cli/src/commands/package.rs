//! `apollia-os package` subcommands: build, inspect, preview and install
//! `.apo` archives, and keep the keyring of trusted publisher keys.
//!
//! `build` and `inspect` work on files alone and never reach the runtime.
//! `build` reads a package folder, validates its `apollia.toml`, and writes one
//! `.apo` archive, signed on request with the author key kept in the secret
//! store. `inspect` reads an archive and reports its manifest, its entries and
//! the state of its signature; a signature that does not hold is a refusal,
//! exit 1.
//!
//! `preview`, `install` and `trust` go through the runtime (see [`remote`]):
//! the preview reads the installed packages, the keyring, the connections and
//! the governance of the runtime, and the installation adds MCP servers and
//! writes the audit journal.

mod remote;

use std::path::{Path, PathBuf};

use apollia_aip::apo_archive::{
    build_from_dir, ApoArchive, BuildOptions, EntryInfo, SignatureState, APO_EXTENSION,
};
use apollia_signing::{load_signing_key, store_signing_key, SigningError, SigningKeyPair};
use clap::Subcommand;

use crate::exit_codes;
use crate::output::emit_error;

/// Label of the author key in the secret store when `--key` is not given.
pub const DEFAULT_KEY_LABEL: &str = "default";

/// Package subcommands: `apollia-os package <verb>`.
#[derive(Debug, Subcommand)]
pub enum PackageCommand {
    /// Build a `.apo` archive from a package folder.
    ///
    /// Validates `apollia.toml`, refuses a folder whose Python modules define
    /// tools or workflows that the manifest does not describe (write the
    /// descriptors with `python -m apollia describe <folder>`), and writes the
    /// archive. No Python code runs.
    Build {
        /// Package folder, holding `apollia.toml` at its root.
        dir: PathBuf,
        /// Sign the archive with the author key. The key is read from the
        /// secret store under `--key`; it is generated and stored there the
        /// first time.
        #[arg(long)]
        sign: bool,
        /// Label of the author key in the secret store.
        #[arg(long, value_name = "LABEL", default_value = DEFAULT_KEY_LABEL, requires = "sign")]
        key: String,
        /// Archive to write. Default: `<name>-<version>.apo` in the current
        /// folder.
        #[arg(long, short = 'o', value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Show the manifest, the entries and the signature state of a `.apo` archive.
    ///
    /// The signature state is `unsigned`, or `signed` with the declared
    /// publisher and the fingerprint of the key. A signature that does not
    /// match the content is refused with exit 1.
    Inspect {
        /// The `.apo` archive.
        file: PathBuf,
    },
    /// Show what a `.apo` package contains and asks for, before installing it.
    ///
    /// Reports the agents, workflows and tools, the accesses each agent asks
    /// for and what each permits, the publisher and the signature (trusted
    /// when its key is in the keyring), the connections the package requires
    /// (present or missing) and the MCP servers it brings, and, for an
    /// installed package, what the new version adds. Nothing is written.
    /// Needs the runtime.
    Preview {
        /// The `.apo` archive.
        file: PathBuf,
    },
    /// Install a `.apo` package, or update it.
    ///
    /// A signature that does not hold is refused; an unsigned package
    /// installs with a notice. The local overrides of its agents are kept on
    /// update. Needs the runtime.
    Install {
        /// The `.apo` archive.
        file: PathBuf,
        /// Accept the Python dependencies the package installs.
        #[arg(long)]
        confirm_deps: bool,
        /// Accept what an update adds: accesses, required connections, MCP
        /// servers (read `package preview` first).
        #[arg(long)]
        confirm_changes: bool,
        /// Install a version older than the installed one.
        #[arg(long)]
        allow_downgrade: bool,
    },
    /// Trusted publisher keys: a package signed by one is shown under its label.
    Trust {
        /// Keyring subcommand.
        #[command(subcommand)]
        command: TrustCommand,
    },
}

/// `apollia-os package trust <verb>`.
#[derive(Debug, Subcommand)]
pub enum TrustCommand {
    /// Trust a publisher's public key, given as 64 hexadecimal characters or
    /// as a file holding them.
    Import {
        /// The key, or a file holding it.
        key: String,
        /// Name to show for the publisher.
        #[arg(long)]
        label: String,
    },
    /// List the trusted publisher keys.
    List,
    /// Stop trusting a publisher key.
    Remove {
        /// SHA-256 fingerprint of the key, as `package trust list` shows it.
        fingerprint: String,
        /// Remove without asking.
        #[arg(long)]
        confirm: bool,
    },
}

/// Execute a `package` subcommand and return the process exit code.
pub async fn run(cmd: &PackageCommand, socket: Option<PathBuf>, json: bool) -> i32 {
    match cmd {
        PackageCommand::Build {
            dir,
            sign,
            key,
            output,
        } => run_build(dir, *sign, key, output.as_deref(), json),
        PackageCommand::Inspect { file } => run_inspect(file, json),
        PackageCommand::Preview { file } => remote::run_preview(file, socket, json).await,
        PackageCommand::Install {
            file,
            confirm_deps,
            confirm_changes,
            allow_downgrade,
        } => {
            let options = remote::InstallFlags {
                confirm_deps: *confirm_deps,
                confirm_changes: *confirm_changes,
                allow_downgrade: *allow_downgrade,
            };
            remote::run_install(file, options, socket, json).await
        }
        PackageCommand::Trust { command } => remote::run_trust(command, socket, json).await,
    }
}

fn run_build(dir: &Path, sign: bool, label: &str, output: Option<&Path>, json: bool) -> i32 {
    let key = if sign {
        match author_key(label) {
            Ok(key) => Some(key),
            Err(message) => return emit_error(json, exit_codes::GENERAL_ERROR, &message),
        }
    } else {
        None
    };
    let output = match output {
        Some(path) => path.to_path_buf(),
        None => match default_output(dir) {
            Ok(path) => path,
            Err(message) => return emit_error(json, exit_codes::GENERAL_ERROR, &message),
        },
    };
    let options = BuildOptions {
        signing_key: key.as_ref(),
        ..BuildOptions::default()
    };
    let report = match build_from_dir(dir, &output, &options) {
        Ok(report) => report,
        Err(e) => return emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
    };
    if json {
        let doc = serde_json::json!({
            "path": output.display().to_string(),
            "package": report.manifest.package.name,
            "version": report.manifest.package.version,
            "entries": report.entries,
            "signature": signature_json(&report.signature),
        });
        println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
    } else {
        println!("{}", output.display());
        crate::note!(
            "  {} {} - {} entries, {}",
            report.manifest.package.name,
            report.manifest.package.version,
            report.entries.len(),
            signature_line(&report.signature)
        );
    }
    exit_codes::SUCCESS
}

fn run_inspect(file: &Path, json: bool) -> i32 {
    let mut archive = match ApoArchive::open(file) {
        Ok(archive) => archive,
        Err(e) => return emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
    };
    let signature = match archive.signature_state() {
        Ok(state) => state,
        Err(e) => return emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
    };
    let manifest = archive.manifest();
    if json {
        let manifest_json = match serde_json::to_value(manifest) {
            Ok(value) => value,
            Err(e) => return emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
        };
        let doc = serde_json::json!({
            "path": file.display().to_string(),
            "manifest": manifest_json,
            "entries": archive.entries(),
            "signature": signature_json(&signature),
        });
        println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
        return exit_codes::SUCCESS;
    }
    let package = &manifest.package;
    println!("{} {}", package.name, package.version);
    println!("  publisher  {}", package.publisher);
    println!("  signature  {}", signature_line(&signature));
    println!(
        "  content    {} agents, {} workflows, {} tools",
        manifest.agents.len(),
        manifest.workflows.len(),
        manifest.tools.len()
    );
    crate::note!();
    crate::note!("  {:>10}  PATH", "BYTES");
    for EntryInfo { path, size } in archive.entries() {
        println!("  {size:>10}  {path}");
    }
    exit_codes::SUCCESS
}

/// The author key stored under `label`, generated and stored on first use.
fn author_key(label: &str) -> Result<SigningKeyPair, String> {
    let store = apollia_auth::select_secret_store()
        .map_err(|e| format!("secret store unavailable: {e}"))?;
    match load_signing_key(store.as_ref(), label) {
        Ok(key) => Ok(key),
        Err(SigningError::KeyNotFound { .. }) => {
            let key = SigningKeyPair::generate().map_err(|e| e.to_string())?;
            store_signing_key(store.as_ref(), label, &key).map_err(|e| e.to_string())?;
            eprintln!(
                "generated author key `{label}`, fingerprint {}",
                key.public_key().fingerprint()
            );
            Ok(key)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// `<name>-<version>.apo` in the current folder, from the folder's manifest.
fn default_output(dir: &Path) -> Result<PathBuf, String> {
    let path = dir.join(apollia_aip::apollia_manifest::MANIFEST_FILE_NAME);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let manifest = apollia_aip::apollia_manifest::parse(&text).map_err(|e| e.to_string())?;
    Ok(PathBuf::from(format!(
        "{}-{}.{APO_EXTENSION}",
        manifest.package.name, manifest.package.version
    )))
}

fn signature_json(state: &SignatureState) -> serde_json::Value {
    match state {
        SignatureState::Unsigned => serde_json::json!({ "state": "unsigned" }),
        SignatureState::Signed {
            publisher,
            key_fingerprint,
        } => serde_json::json!({
            "state": "signed",
            "publisher": publisher,
            "key_fingerprint": key_fingerprint,
        }),
    }
}

fn signature_line(state: &SignatureState) -> String {
    match state {
        SignatureState::Unsigned => "unsigned".to_owned(),
        SignatureState::Signed {
            publisher,
            key_fingerprint,
        } => format!("signed by {publisher} (key {key_fingerprint})"),
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[derive(Debug, Parser)]
    struct Harness {
        #[command(subcommand)]
        cmd: PackageCommand,
    }

    #[test]
    fn test_parse_build_with_sign_and_output() {
        // GIVEN "package build ./pkg --sign --key work -o out.apo"
        let argv = [
            "x", "build", "./pkg", "--sign", "--key", "work", "-o", "out.apo",
        ];

        // WHEN it is parsed
        let parsed = Harness::try_parse_from(argv).unwrap();

        // THEN every option lands in Build
        let PackageCommand::Build {
            dir,
            sign,
            key,
            output,
        } = parsed.cmd
        else {
            panic!("expected build");
        };
        assert_eq!(dir, PathBuf::from("./pkg"));
        assert!(sign);
        assert_eq!(key, "work");
        assert_eq!(output, Some(PathBuf::from("out.apo")));
    }

    #[test]
    fn test_parse_key_without_sign_is_refused() {
        // GIVEN a key label without --sign
        let argv = ["x", "build", "./pkg", "--key", "work"];

        // WHEN it is parsed
        let parsed = Harness::try_parse_from(argv);

        // THEN clap refuses it
        assert!(parsed.is_err());
    }

    #[test]
    fn test_signature_json_shapes() {
        // GIVEN the two signature states
        let unsigned = SignatureState::Unsigned;
        let signed = SignatureState::Signed {
            publisher: "Acme".into(),
            key_fingerprint: "ab".repeat(32),
        };

        // WHEN they are rendered for --json
        let (u, s) = (signature_json(&unsigned), signature_json(&signed));

        // THEN each carries its state, the signed one its publisher and key
        assert_eq!(u["state"], "unsigned");
        assert_eq!(s["state"], "signed");
        assert_eq!(s["publisher"], "Acme");
        assert_eq!(s["key_fingerprint"], "ab".repeat(32));
    }
}
