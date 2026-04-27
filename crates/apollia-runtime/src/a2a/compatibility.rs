//! Vérification de compatibilité semver entre versions requise et advertised d'un skill A2A.
//!
//! Applique une politique simple alignée sur semver.org :
//! - **MAJOR mismatch** : breaking change ; le Worker n'est pas compatible.
//! - **MINOR mismatch** : potentielles API additions ; compatible mais émet un warning
//!   si `advertised.minor < required.minor` (le Worker est plus vieux que ce que
//!   le Director réclame).
//! - **PATCH mismatch** : purement fixes ; aucun warning.
//!
//! précise que l'on émet un warning pour les mismatch MINOR (semver
//! minor). Les mismatch MAJOR sont considérés comme incompatibles.

use serde::{Deserialize, Serialize};

/// Version semver triée en trois composantes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemVer {
    /// Composante majeure.
    pub major: u32,
    /// Composante mineure.
    pub minor: u32,
    /// Composante patch.
    pub patch: u32,
}

impl SemVer {
    /// Parse une chaîne semver `"MAJOR.MINOR.PATCH"`. Tolère un préfixe `v`
    /// et un suffixe `-pre` / `+meta` (ignorés).
    ///
    /// Retourne `None` si le format n'est pas exploitable.
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

/// Niveau de gravité d'une incompatibilité.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatSeverity {
    /// Aucune alternative n'est utilisable — le Director devrait éviter ce Worker.
    Incompatible,
    /// Le Worker fonctionne mais certaines capacités peuvent manquer.
    Warning,
}

/// Warning structuré de compatibilité A2A.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2ACompatibilityWarning {
    /// Identifiant du skill concerné.
    pub skill_id: String,
    /// Nom du Worker Agent advertised.
    pub agent_name: String,
    /// Version semver requise par le Director.
    pub required_version: String,
    /// Version semver advertised par le Worker.
    pub advertised_version: String,
    /// Gravité du mismatch.
    pub severity: CompatSeverity,
    /// Message humain décrivant l'écart.
    pub message: String,
    /// Nom d'un Worker alternatif compatible, si disponible.
    pub alternative_agent: Option<String>,
}

/// Compare une version requise à une version advertised et retourne un warning
/// structuré si un mismatch pertinent est détecté.
///
/// Retourne `None` si les versions sont parfaitement compatibles ou si le parsing
/// échoue (dans ce cas, on considère l'ancien comportement — pas de warning pour
/// ne pas bloquer des Workers mal versionnés).
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

    // Minor mismatch : warn quand le Worker est plus ancien que ce que le Director demande.
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

/// Attache un Worker alternatif compatible au warning, si disponible.
///
/// `alternatives` est la liste des paires `(agent_name, advertised_version)` parmi
/// lesquelles on cherche un candidat qui satisferait `required_version`.
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
        assert!(SemVer::parse("not-a-version").is_none());
    }

    #[test]
    fn test_identical_versions_no_warning() {
        // GIVEN required == advertised
        let w = check_compatibility("s", "a", "1.2.3", "1.2.3");
        // THEN pas de warning
        assert!(w.is_none());
    }

    #[test]
    fn test_patch_mismatch_no_warning() {
        // GIVEN un simple diff patch
        let w = check_compatibility("s", "a", "1.2.3", "1.2.9");
        // THEN aucun warning (patch est considéré comme compatible)
        assert!(w.is_none());
    }

    #[test]
    fn test_advertised_newer_minor_no_warning() {
        // GIVEN worker plus récent en minor
        let w = check_compatibility("s", "a", "1.2.0", "1.5.0");
        // THEN pas de warning — le worker est compatible rétro-actif.
        assert!(w.is_none());
    }

    #[test]
    fn test_advertised_older_minor_emits_warning() {
        // GIVEN director demande 1.5 mais worker advertises 1.2
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
        // GIVEN major divergent
        let w = check_compatibility("s", "a", "2.0.0", "1.9.0").unwrap();
        // THEN severity = Incompatible
        assert_eq!(w.severity, CompatSeverity::Incompatible);
    }

    #[test]
    fn test_with_alternative_finds_compatible_worker() {
        // GIVEN un warning et un Worker alternatif compatible
        let w = check_compatibility("s", "worker-a", "1.5.0", "1.2.0").unwrap();
        let alts = vec![
            ("worker-a".to_string(), "1.2.0".to_string()),
            ("worker-b".to_string(), "1.6.0".to_string()),
            ("worker-c".to_string(), "0.9.0".to_string()),
        ];
        let enriched = with_alternative(w, &alts);

        // THEN worker-b est proposé
        assert_eq!(enriched.alternative_agent.as_deref(), Some("worker-b"));
    }

    #[test]
    fn test_with_alternative_none_when_no_match() {
        // GIVEN aucun alternatif compatible
        let w = check_compatibility("s", "worker-a", "1.5.0", "1.2.0").unwrap();
        let alts = vec![("worker-c".to_string(), "0.9.0".to_string())];
        let enriched = with_alternative(w, &alts);

        // THEN pas d'alternative
        assert!(enriched.alternative_agent.is_none());
    }
}
