//! Semver compatibility check between the required and advertised versions of an A2A skill.
//!
//! Applies a simple policy aligned with semver.org:
//! - **MAJOR mismatch**: breaking change; the Worker is not compatible.
//! - **MINOR mismatch**: potential API additions; compatible but emits a warning
//!   if `advertised.minor < required.minor` (the Worker is older than what
//!   the Director requires).
//! - **PATCH mismatch**: pure fixes; no warning.
//!
//! A warning is emitted for MINOR mismatches (semver minor). MAJOR mismatches
//! are treated as incompatible.

use serde::{Deserialize, Serialize};

/// Semver version split into three components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemVer {
    /// Major component.
    pub major: u32,
    /// Minor component.
    pub minor: u32,
    /// Patch component.
    pub patch: u32,
}

impl SemVer {
    /// Parses a semver string `"MAJOR.MINOR.PATCH"`. Tolerates a `v` prefix
    /// and a `-pre` / `+meta` suffix (ignored).
    ///
    /// Returns `None` if the format is not usable.
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim().trim_start_matches('v');
        let core = trimmed.split(['-', '+']).next().unwrap_or(trimmed);
        let mut parts = core.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next().unwrap_or("0").parse().ok()?;
        Some(Self {
            major,
            minor,
            patch,
        })
    }
}

/// Severity level of an incompatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatSeverity {
    /// No usable alternative: the Director should avoid this Worker.
    Incompatible,
    /// The Worker works but some capabilities may be missing.
    Warning,
}

/// Structured A2A compatibility warning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2ACompatibilityWarning {
    /// Identifier of the affected skill.
    pub skill_id: String,
    /// Advertised name of the Worker Agent.
    pub agent_name: String,
    /// Semver version required by the Director.
    pub required_version: String,
    /// Semver version advertised by the Worker.
    pub advertised_version: String,
    /// Severity of the mismatch.
    pub severity: CompatSeverity,
    /// Human-readable message describing the gap.
    pub message: String,
    /// Name of a compatible alternative Worker, if available.
    pub alternative_agent: Option<String>,
}

/// Compares a required version against an advertised version and returns a
/// structured warning if a relevant mismatch is detected.
///
/// Returns `None` if the versions are perfectly compatible or if parsing
/// fails (in which case the lenient behavior applies: no warning, so as not
/// to block poorly versioned Workers).
pub fn check_compatibility(
    skill_id: &str,
    agent_name: &str,
    required_version: &str,
    advertised_version: &str,
) -> Option<A2ACompatibilityWarning> {
    let req = SemVer::parse(required_version)?;
    let adv = SemVer::parse(advertised_version)?;

    if req.major != adv.major {
        return Some(A2ACompatibilityWarning {
            skill_id: skill_id.to_string(),
            agent_name: agent_name.to_string(),
            required_version: required_version.to_string(),
            advertised_version: advertised_version.to_string(),
            severity: CompatSeverity::Incompatible,
            message: format!(
                "major version mismatch: required {}.x, worker advertises {}.x",
                req.major, adv.major
            ),
            alternative_agent: None,
        });
    }

    // Minor mismatch: warn when the Worker is older than what the Director requests.
    if adv.minor < req.minor {
        return Some(A2ACompatibilityWarning {
            skill_id: skill_id.to_string(),
            agent_name: agent_name.to_string(),
            required_version: required_version.to_string(),
            advertised_version: advertised_version.to_string(),
            severity: CompatSeverity::Warning,
            message: format!(
                "worker advertises {}.{}.x but director requires at least {}.{}.x",
                adv.major, adv.minor, req.major, req.minor
            ),
            alternative_agent: None,
        });
    }

    None
}

/// Attaches a compatible alternative Worker to the warning, if available.
///
/// `alternatives` is the list of `(agent_name, advertised_version)` pairs among
/// which a candidate satisfying `required_version` is sought.
pub fn with_alternative(
    mut warning: A2ACompatibilityWarning,
    alternatives: &[(String, String)],
) -> A2ACompatibilityWarning {
    let Some(req) = SemVer::parse(&warning.required_version) else {
        return warning;
    };
    for (name, ver) in alternatives {
        if name == &warning.agent_name {
            continue;
        }
        let Some(v) = SemVer::parse(ver) else {
            continue;
        };
        if v.major == req.major && v.minor >= req.minor {
            warning.alternative_agent = Some(name.clone());
            break;
        }
    }
    warning
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_parse_basic() {
        // GIVEN a plain three-component version string
        // WHEN SemVer::parse reads it
        // THEN major, minor and patch come back as written
        assert_eq!(
            SemVer::parse("1.2.3"),
            Some(SemVer {
                major: 1,
                minor: 2,
                patch: 3
            })
        );
    }

    #[test]
    fn test_semver_parse_with_prefix_and_prerelease() {
        // GIVEN a version carrying a v prefix and a prerelease suffix
        // WHEN SemVer::parse reads it
        // THEN both decorations are dropped and the three components remain
        assert_eq!(
            SemVer::parse("v2.5.0-beta.1"),
            Some(SemVer {
                major: 2,
                minor: 5,
                patch: 0
            })
        );
    }

    #[test]
    fn test_semver_parse_invalid() {
        // GIVEN a string that is not a version
        // WHEN SemVer::parse reads it
        // THEN nothing is returned rather than a zeroed version
        assert!(SemVer::parse("not-a-version").is_none());
    }

    #[test]
    fn test_identical_versions_no_warning() {
        // GIVEN required == advertised
        // WHEN compatibility is checked
        let w = check_compatibility("s", "a", "1.2.3", "1.2.3");
        // THEN no warning
        assert!(w.is_none());
    }

    #[test]
    fn test_patch_mismatch_no_warning() {
        // GIVEN a simple patch diff
        // WHEN compatibility is checked
        let w = check_compatibility("s", "a", "1.2.3", "1.2.9");
        // THEN no warning (patch is considered compatible)
        assert!(w.is_none());
    }

    #[test]
    fn test_advertised_newer_minor_no_warning() {
        // GIVEN a worker newer on the minor component
        // WHEN compatibility is checked
        let w = check_compatibility("s", "a", "1.2.0", "1.5.0");
        // THEN no warning: the worker is backward compatible.
        assert!(w.is_none());
    }

    #[test]
    fn test_advertised_older_minor_emits_warning() {
        // GIVEN the director requires 1.5 but the worker advertises 1.2
        // WHEN compatibility is checked
        let w = check_compatibility("read-excel", "worker-a", "1.5.0", "1.2.0")
            .expect("expected a warning");

        // THEN severity = Warning
        assert_eq!(w.severity, CompatSeverity::Warning);
        assert_eq!(w.skill_id, "read-excel");
        assert_eq!(w.agent_name, "worker-a");
        assert!(w.message.contains("1.2"));
        assert!(w.message.contains("1.5"));
    }

    #[test]
    fn test_major_mismatch_is_incompatible() {
        // GIVEN a divergent major
        // WHEN compatibility is checked
        let w = check_compatibility("s", "a", "2.0.0", "1.9.0").unwrap();
        // THEN severity = Incompatible
        assert_eq!(w.severity, CompatSeverity::Incompatible);
    }

    #[test]
    fn test_with_alternative_finds_compatible_worker() {
        // GIVEN a warning and a compatible alternative Worker
        let w = check_compatibility("s", "worker-a", "1.5.0", "1.2.0").unwrap();
        let alts = vec![
            ("worker-a".to_string(), "1.2.0".to_string()),
            ("worker-b".to_string(), "1.6.0".to_string()),
            ("worker-c".to_string(), "0.9.0".to_string()),
        ];
        // WHEN the alternatives are searched for a compatible worker
        let enriched = with_alternative(w, &alts);

        // THEN worker-b is proposed
        assert_eq!(enriched.alternative_agent.as_deref(), Some("worker-b"));
    }

    #[test]
    fn test_with_alternative_none_when_no_match() {
        // GIVEN no compatible alternative
        let w = check_compatibility("s", "worker-a", "1.5.0", "1.2.0").unwrap();
        let alts = vec![("worker-c".to_string(), "0.9.0".to_string())];
        // WHEN the alternatives are searched for a compatible worker
        let enriched = with_alternative(w, &alts);

        // THEN no alternative
        assert!(enriched.alternative_agent.is_none());
    }
}
