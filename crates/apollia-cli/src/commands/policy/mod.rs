//! `apollia-os policy` subcommands: prepare and check the organisation policy.
//!
//! Every leaf works on files alone and never reaches the runtime. `keygen`
//! creates the organisation key pair, `init` writes a commented policy that
//! imposes nothing, `sign` signs it, `verify` checks a folder, and `show` reads
//! the policy the runtime reads at startup. No leaf writes into the system
//! policy folder: IT deploys the prepared folder there with its own tools.

mod show;

use std::path::{Path, PathBuf};

use apollia_permissions::load_policy;
use apollia_permissions::org_policy::{
    policy_location, refuse_system_dir, sign_policy_dir, verify_policy_dir, write_policy_template,
    PolicyVerdict, ORG_KEY_FILE, ORG_PRIVATE_KEY_FILE,
};
use apollia_signing::{read_private_key_file, write_private_key_file, SigningKeyPair};
use clap::Subcommand;

use crate::exit_codes;
use crate::output::emit_error;

/// Publisher name written in `SIGNATURE` when `--publisher` is not given.
pub const DEFAULT_PUBLISHER: &str = "Organisation";

/// Policy subcommands: `apollia-os policy <verb>`.
#[derive(Debug, Subcommand)]
pub enum PolicyCommand {
    /// Create the organisation key pair in a folder.
    ///
    /// Writes `org.key`, the private key, readable by its owner only from the
    /// moment it is created, and `org.pub`, the public key; prints the
    /// fingerprint of the public key. Existing files are never replaced. Keep
    /// `org.key` off the workstations.
    Keygen {
        /// Folder that receives `org.key` and `org.pub`, created when missing.
        #[arg(long, value_name = "DIR")]
        out: PathBuf,
    },
    /// Write a commented `policy.toml` that imposes nothing, dated now.
    ///
    /// The folder is created when missing; an existing `policy.toml` is never
    /// replaced.
    Init {
        /// Policy folder.
        dir: PathBuf,
    },
    /// Sign the `policy.toml` of a folder with the organisation private key.
    ///
    /// Writes `SIGNATURE` and `org.pub` next to it, replacing earlier ones. A
    /// policy that breaks the schema is not signed.
    Sign {
        /// The organisation private key, as written by `policy keygen`.
        #[arg(long, value_name = "FILE")]
        key: PathBuf,
        /// Publisher name declared in the signature.
        #[arg(long, value_name = "NAME", default_value = DEFAULT_PUBLISHER)]
        publisher: String,
        /// Policy folder.
        dir: PathBuf,
    },
    /// Check a policy folder: schema, signature, and rights in the system folder.
    ///
    /// The rights are checked when the folder is the one the runtime reads.
    /// A folder that does not hold is refused with exit 1; `--json` carries
    /// the verdict, the reason (`schema`, `signature`, `key`, `owner`, or
    /// `missing` when the folder does not exist) and its detail.
    Verify {
        /// Policy folder.
        dir: PathBuf,
    },
    /// Show the policy the runtime reads at startup, or none.
    Show,
}

/// Execute a `policy` subcommand and return the process exit code.
pub fn run(cmd: &PolicyCommand, json: bool) -> i32 {
    match cmd {
        PolicyCommand::Keygen { out } => run_keygen(out, json),
        PolicyCommand::Init { dir } => run_init(dir, json),
        PolicyCommand::Sign {
            key,
            publisher,
            dir,
        } => run_sign(key, publisher, dir, json),
        PolicyCommand::Verify { dir } => run_verify(dir, json),
        PolicyCommand::Show => show::run(json),
    }
}

fn print_json(doc: &serde_json::Value) {
    println!("{}", serde_json::to_string_pretty(doc).unwrap_or_default());
}

fn run_keygen(out: &Path, json: bool) -> i32 {
    if let Err(e) = refuse_system_dir(out) {
        return emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string());
    }
    let private_path = out.join(ORG_PRIVATE_KEY_FILE);
    let public_path = out.join(ORG_KEY_FILE);
    for path in [&private_path, &public_path] {
        if path.exists() {
            let message = format!("{} already exists", path.display());
            return emit_error(json, exit_codes::GENERAL_ERROR, &message);
        }
    }
    if let Err(e) = std::fs::create_dir_all(out) {
        let message = format!("cannot create {}: {e}", out.display());
        return emit_error(json, exit_codes::GENERAL_ERROR, &message);
    }
    let pair = match SigningKeyPair::generate() {
        Ok(pair) => pair,
        Err(e) => return emit_error(json, exit_codes::GENERAL_ERROR, &e.to_string()),
    };
    if let Err(e) = write_private_key_file(&private_path, &pair) {
        return emit_error(json, exit_codes::GENERAL_ERROR, &chain(&e));
    }
    let public_key = pair.public_key();
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&public_path)
        .and_then(|mut file| {
            std::io::Write::write_all(&mut file, format!("{}\n", public_key.to_hex()).as_bytes())
        });
    if let Err(e) = written {
        let message = format!("cannot write {}: {e}", public_path.display());
        return emit_error(json, exit_codes::GENERAL_ERROR, &message);
    }
    let fingerprint = public_key.fingerprint();
    if json {
        print_json(&serde_json::json!({
            "private_key": private_path.display().to_string(),
            "public_key": public_path.display().to_string(),
            "fingerprint": fingerprint,
        }));
    } else {
        println!("{fingerprint}");
        crate::note!("  private key: {}", private_path.display());
        crate::note!("  public key:  {}", public_path.display());
        crate::note!(
            "  Keep the private key off the workstations; anyone holding it can sign a policy."
        );
    }
    exit_codes::SUCCESS
}

fn run_init(dir: &Path, json: bool) -> i32 {
    let signed_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let path = match write_policy_template(dir, &signed_at) {
        Ok(path) => path,
        Err(e) => return emit_error(json, exit_codes::GENERAL_ERROR, &chain(&e)),
    };
    if json {
        print_json(&serde_json::json!({
            "path": path.display().to_string(),
            "signed_at": signed_at,
        }));
    } else {
        println!("{}", path.display());
        crate::note!("  Edit it, then sign the folder with `apollia-os policy sign`.");
    }
    exit_codes::SUCCESS
}

fn run_sign(key_path: &Path, publisher: &str, dir: &Path, json: bool) -> i32 {
    let key = match read_private_key_file(key_path) {
        Ok(key) => key,
        Err(e) => return emit_error(json, exit_codes::GENERAL_ERROR, &chain(&e)),
    };
    let signed = match sign_policy_dir(dir, &key, publisher) {
        Ok(signed) => signed,
        Err(e) => return emit_error(json, exit_codes::GENERAL_ERROR, &chain(&e)),
    };
    if json {
        print_json(&serde_json::json!({
            "dir": dir.display().to_string(),
            "key_fingerprint": signed.key_fingerprint,
            "publisher": signed.publisher,
            "signed_at": signed.signed_at.to_string(),
        }));
    } else {
        println!("{}", signed.key_fingerprint);
        crate::note!(
            "  signed {} as {}, dated {}",
            dir.display(),
            signed.publisher,
            signed.signed_at
        );
    }
    exit_codes::SUCCESS
}

/// The exit code of `policy verify` for a verdict.
///
/// Every refusal is exit 1, as for a `.apo` archive whose signature does not
/// hold; the reason travels in the output, not in the code.
pub(crate) fn verify_exit_code(verdict: &PolicyVerdict) -> i32 {
    match verdict {
        PolicyVerdict::Valid(_) => exit_codes::SUCCESS,
        PolicyVerdict::Absent | PolicyVerdict::Invalid(_) => exit_codes::GENERAL_ERROR,
    }
}

/// Whether `dir` is the folder the runtime reads, so its rights are checked.
fn is_runtime_policy_dir(dir: &Path, runtime_dir: &Path) -> bool {
    match (dir.canonicalize(), runtime_dir.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => dir == runtime_dir,
    }
}

fn run_verify(dir: &Path, json: bool) -> i32 {
    let location = policy_location();
    let rights_checked = is_runtime_policy_dir(dir, &location.dir);
    let verdict = if rights_checked {
        load_policy(dir, location.owner)
    } else {
        verify_policy_dir(dir)
    };
    let code = verify_exit_code(&verdict);
    let (reason, detail) = match &verdict {
        PolicyVerdict::Valid(_) => (None, None),
        PolicyVerdict::Absent => (
            Some("missing"),
            Some(format!("{} does not exist", dir.display())),
        ),
        PolicyVerdict::Invalid(invalid) => {
            (Some(invalid.reason.as_str()), Some(invalid.detail.clone()))
        }
    };
    if json {
        let mut doc = serde_json::json!({
            "dir": dir.display().to_string(),
            "verdict": verdict.kind(),
            "reason": reason,
            "rights_checked": rights_checked,
        });
        if let PolicyVerdict::Valid(valid) = &verdict {
            doc["key_fingerprint"] = valid.key_fingerprint.clone().into();
            doc["publisher"] = valid.publisher.clone().into();
            doc["signed_at"] = valid.signed_at.to_string().into();
        }
        if let Some(detail) = &detail {
            doc["detail"] = detail.clone().into();
        }
        print_json(&doc);
        return code;
    }
    match &verdict {
        PolicyVerdict::Valid(valid) => {
            println!("valid");
            crate::note!("  key {}", valid.key_fingerprint);
            crate::note!("  signed by {} on {}", valid.publisher, valid.signed_at);
            if !rights_checked {
                crate::note!(
                    "  Rights not checked: they are checked in the system policy folder only."
                );
            }
        }
        _ => {
            let reason = reason.unwrap_or("invalid");
            let detail = detail.unwrap_or_default();
            eprintln!("Error: refused ({reason}): {detail}");
        }
    }
    code
}

/// The message of `e` followed by its sources, on one line.
fn chain(e: &dyn std::error::Error) -> String {
    let mut message = e.to_string();
    let mut source = e.source();
    while let Some(inner) = source {
        message.push_str(": ");
        message.push_str(&inner.to_string());
        source = inner.source();
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_exit_code_refusals_are_exit_one() {
        // GIVEN an absent folder and a refused one
        let refused = PolicyVerdict::Invalid(apollia_permissions::PolicyInvalid {
            reason: apollia_permissions::PolicyInvalidReason::Signature,
            detail: String::from("does not hold"),
        });
        // WHEN their exit codes are read
        // THEN both are a general error
        assert_eq!(
            verify_exit_code(&PolicyVerdict::Absent),
            exit_codes::GENERAL_ERROR
        );
        assert_eq!(verify_exit_code(&refused), exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn test_chain_carries_the_source() {
        // GIVEN an error with an io source
        let e = apollia_signing::SigningError::KeyFile {
            path: PathBuf::from("/x/org.key"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "gone"),
        };
        // WHEN it is rendered on one line
        let line = chain(&e);
        // THEN both the message and the cause appear
        assert_eq!(line, "private key file /x/org.key: gone");
    }
}
