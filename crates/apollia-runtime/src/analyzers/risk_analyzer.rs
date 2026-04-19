//! Static risk analyzer (US-SP42-045 — Pattern P9, always-on layer).
//!
//! Maps an `action_type` (e.g. `"tool:bash"`, `"memory_write_bulk"`,
//! `"a2a:external"`) to a structured [`RiskAnalysis`] with categories,
//! severity 0..10 and mitigation tips. No LLM is involved — coût : 0.
//!
//! Opt-in narration (`consequences_if_approved` / `consequences_if_rejected`)
//! is produced by [`MetaRoutine::GenerateAskUserConsequences`] via the meta
//! orchestrator and is grafted on top of this analyser's output by the
//! runtime before the HITL card is surfaced.

use apollia_core::hitl_request::RiskAnalysis;

/// Static `(needle, categories, severity, mitigations)` table.
///
/// `needle` is matched case-insensitively as a substring against the
/// `action_type` string. The first match wins; entries are ordered roughly
/// from most-specific to least-specific.
const STATIC_MAPPINGS: &[(&str, &[&str], u8, &[&str])] = &[
    // ── Destructive shell / filesystem ──
    (
        "tool:bash",
        &["shell", "destructive"],
        8,
        &[
            "Inspect the command before approving.",
            "Prefer a dry-run if available.",
        ],
    ),
    (
        "tool:shell",
        &["shell", "destructive"],
        8,
        &["Inspect the command before approving."],
    ),
    (
        "filesystem:delete",
        &["destructive", "filesystem"],
        9,
        &["Verify the path is correct.", "Check for a backup."],
    ),
    (
        "filesystem:write",
        &["filesystem"],
        5,
        &["Confirm the target path is within the workspace."],
    ),
    // ── Memory ──
    (
        "memory_write_bulk",
        &["memory_write", "bulk"],
        7,
        &[
            "Bulk writes can overwhelm semantic memory.",
            "Review a sample of the entries being written.",
        ],
    ),
    (
        "memory_write",
        &["memory_write"],
        4,
        &["The agent will persist this entry across sessions."],
    ),
    // ── Network / external ──
    (
        "a2a:external",
        &["external_a2a", "network"],
        7,
        &[
            "This calls an external agent — data may leave the machine.",
            "Check the target worker's identity.",
        ],
    ),
    (
        "a2a:",
        &["a2a"],
        4,
        &["The agent will delegate to another worker."],
    ),
    (
        "network:post",
        &["network", "write"],
        6,
        &["Outbound network write — verify the endpoint."],
    ),
    (
        "network:get",
        &["network"],
        3,
        &["Read-only network fetch — low risk."],
    ),
    // ── Generic tool call ──
    (
        "tool:",
        &["tool_call"],
        4,
        &["Inspect the tool arguments before approving."],
    ),
    // ── Ask-user / pure-info ──
    (
        "ask_user",
        &["information"],
        2,
        &["The agent is pausing for clarification."],
    ),
];

/// Classify an `action_type` string into a [`RiskAnalysis`].
///
/// `action_type` is a free-form slug supplied by the caller (typically the
/// runtime suspension point). When no needle matches, returns the generic
/// [`RiskAnalysis::unknown`] fallback.
pub fn analyze(action_type: &str) -> RiskAnalysis {
    let lower = action_type.to_ascii_lowercase();
    for (needle, categories, severity, mitigations) in STATIC_MAPPINGS {
        if lower.contains(needle) {
            return RiskAnalysis::new(
                categories.iter().copied(),
                *severity,
                mitigations.iter().copied(),
            );
        }
    }
    RiskAnalysis::unknown()
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::hitl_request::ImpactLevel;

    /// GIVEN a destructive shell action
    /// WHEN analyze()
    /// THEN severity >= 7 and a shell category is present.
    #[test]
    fn bash_is_high_severity() {
        let ra = analyze("tool:bash");
        assert!(ra.severity >= 7);
        assert!(ra.categories.iter().any(|c| c == "shell"));
        assert!(!ra.mitigations.is_empty());
    }

    /// GIVEN a filesystem delete action
    /// WHEN analyze()
    /// THEN severity derives to Critical via ImpactLevel.
    #[test]
    fn filesystem_delete_is_critical() {
        let ra = analyze("filesystem:delete");
        assert_eq!(ImpactLevel::from_severity(ra.severity), ImpactLevel::Critical);
    }

    /// GIVEN a bulk memory-write action
    /// WHEN analyze()
    /// THEN `memory_write` category is present and severity is High.
    #[test]
    fn memory_write_bulk_is_high_severity() {
        let ra = analyze("memory_write_bulk");
        assert!(ra.categories.iter().any(|c| c == "memory_write"));
        assert_eq!(ImpactLevel::from_severity(ra.severity), ImpactLevel::High);
    }

    /// GIVEN an external A2A call
    /// WHEN analyze()
    /// THEN `external_a2a` + `network` categories present.
    #[test]
    fn a2a_external_flags_network_category() {
        let ra = analyze("a2a:external:excel-worker");
        assert!(ra.categories.iter().any(|c| c == "external_a2a"));
        assert!(ra.categories.iter().any(|c| c == "network"));
    }

    /// GIVEN a pure ask-user pause (no side-effects)
    /// WHEN analyze()
    /// THEN severity is Low.
    #[test]
    fn ask_user_is_low_severity() {
        let ra = analyze("ask_user");
        assert_eq!(ImpactLevel::from_severity(ra.severity), ImpactLevel::Low);
    }

    /// GIVEN an unrecognised action_type
    /// WHEN analyze()
    /// THEN the unknown fallback is used (severity 5, non-empty).
    #[test]
    fn unknown_action_type_falls_back() {
        let ra = analyze("flux-capacitor:unknown");
        assert_eq!(ra, RiskAnalysis::unknown());
        assert_eq!(ra.severity, 5);
    }

    /// GIVEN casing variations of the same needle
    /// WHEN analyze()
    /// THEN matching is case-insensitive.
    #[test]
    fn matching_is_case_insensitive() {
        let ra_lower = analyze("tool:bash");
        let ra_upper = analyze("TOOL:BASH");
        assert_eq!(ra_lower, ra_upper);
    }
}
