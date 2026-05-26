//! `apollia-os version` — print binary version information.
//!
//! Complements clap's built-in `--version` flag with a `--json` output that
//! surfaces compile-time metadata useful for bug reports and CI logs.

use crate::exit_codes;

/// Execute the `version` command. Always exits with `SUCCESS`.
pub fn run(json: bool) -> i32 {
    let version = env!("CARGO_PKG_VERSION");
    let target = built_target();
    let profile = built_profile();
    let features = active_features();

    if json {
        let report = serde_json::json!({
            "name": "apollia-os",
            "version": version,
            "target": target,
            "profile": profile,
            "features": features,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    } else {
        println!("apollia-os {version}");
        println!("  target   {target}");
        println!("  profile  {profile}");
        if !features.is_empty() {
            println!("  features {}", features.join(", "));
        }
    }
    exit_codes::SUCCESS
}

fn built_target() -> &'static str {
    // CARGO_CFG_TARGET_* env vars are only set at build time of *build scripts*,
    // not normal compilation; we expose a best-effort static string.
    std::env::consts::OS
}

fn built_profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

fn active_features() -> Vec<&'static str> {
    let mut f = Vec::new();
    #[cfg(feature = "cloud")]
    f.push("cloud");
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_text_returns_success() {
        // GIVEN text mode.
        // WHEN run.
        // THEN exits with code 0.
        assert_eq!(run(false), exit_codes::SUCCESS);
    }

    #[test]
    fn version_json_returns_success() {
        assert_eq!(run(true), exit_codes::SUCCESS);
    }

    #[test]
    fn profile_consistent_with_debug_assertions() {
        let p = built_profile();
        if cfg!(debug_assertions) {
            assert_eq!(p, "debug");
        } else {
            assert_eq!(p, "release");
        }
    }
}
