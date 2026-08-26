//! Code review types, shared between the review agent, the REST route, and the CLI.

use serde::{Deserialize, Serialize};

/// Structured report produced by the `apollia-review` agent.
///
/// Encodes a global quality score (0 = critical failures, 100 = perfect),
/// a list of identified issues, and a textual summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewReport {
    /// Global quality score from 0 (critical) to 100 (perfect).
    pub score: u8,
    /// Individual issues identified during the review.
    pub issues: Vec<ReviewIssue>,
    /// Short textual summary of the review outcome.
    pub summary: String,
}

/// A single issue surfaced during code review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewIssue {
    /// Severity level: `"error"` | `"warning"` | `"info"`.
    pub severity: String,
    /// Analysis category: `"correctness"` | `"convention"` | `"performance"` | `"security"` | `"coverage"`.
    pub category: String,
    /// Human-readable description of the issue.
    pub message: String,
    /// File location in the form `"path/to/file.rs:42"`, when available.
    pub location: Option<String>,
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_report_serialization() {
        // GIVEN a ReviewReport with one issue
        let report = ReviewReport {
            score: 87,
            issues: vec![ReviewIssue {
                severity: "warning".into(),
                category: "convention".into(),
                message: "missing docstring".into(),
                location: Some("src/lib.rs:42".into()),
            }],
            summary: "Good code, a few conventions missing.".into(),
        };

        // WHEN serialized and deserialized
        let json = serde_json::to_string(&report).expect("serialization must succeed");
        let parsed: ReviewReport =
            serde_json::from_str(&json).expect("deserialization must succeed");

        // THEN round-trip is lossless
        assert_eq!(parsed.score, 87);
        assert_eq!(parsed.issues.len(), 1);
        assert_eq!(parsed.issues[0].severity, "warning");
        assert_eq!(parsed.issues[0].category, "convention");
        assert_eq!(parsed.issues[0].location.as_deref(), Some("src/lib.rs:42"));
        assert_eq!(parsed.summary, "Good code, a few conventions missing.");
    }

    #[test]
    fn test_review_report_empty_issues() {
        // GIVEN a clean ReviewReport with no issues
        let report = ReviewReport {
            score: 100,
            issues: Vec::new(),
            summary: "No change to analyse.".into(),
        };

        // WHEN serialized
        let json = serde_json::to_string(&report).expect("serialization must succeed");
        let parsed: ReviewReport =
            serde_json::from_str(&json).expect("deserialization must succeed");

        // THEN score is 100 and issues list is empty
        assert_eq!(parsed.score, 100);
        assert!(parsed.issues.is_empty());
    }

    #[test]
    fn test_review_issue_optional_location() {
        // GIVEN an issue without a file location
        let issue = ReviewIssue {
            severity: "info".into(),
            category: "performance".into(),
            message: "unnecessary clone".into(),
            location: None,
        };

        // WHEN serialized
        let json = serde_json::to_string(&issue).expect("serialization must succeed");
        let parsed: ReviewIssue =
            serde_json::from_str(&json).expect("deserialization must succeed");

        // THEN location is still None
        assert!(parsed.location.is_none());
    }
}
