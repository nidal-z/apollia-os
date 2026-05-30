//! Operation specification: declarative description of a connector operation.

use serde::Serialize;

/// Approval policy for an operation.
///
/// Surfaced to `apollia-tools::governance`: operations marked
/// [`AlwaysRequireApproval`](ApprovalPolicy::AlwaysRequireApproval) trigger
/// a HITL prompt before execution, even when the agent is otherwise trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPolicy {
    /// Auto-approve. Suitable for read-only operations with no side effects.
    AutoApprove,
    /// Require HITL approval before each invocation. Default for writes.
    AlwaysRequireApproval,
    /// Require HITL approval AND a confirmation phrase typed by the user.
    /// Used for destructive irreversible operations (delete, drop).
    ConfirmPhrase,
}

/// Declarative description of one operation exposed by a connector.
///
/// At runtime startup, `apollia-tools::registry` iterates each connector's
/// operations and registers them as native tools. The `input_schema` and
/// `output_schema` are JSON Schemas (dialect `2020-12`) consumed by the agent
/// runtime to validate calls.
///
/// Operations are described declaratively here; the actual handler is wired
/// separately by the runtime (a function pointer or closure dispatching to the
/// owning connector). This keeps `OperationSpec` cheap to clone and
/// `Serialize` friendly for telemetry and the registry UI.
///
/// `Serialize` only: operation specs are constructed in Rust with static
/// string slices and never round-tripped from JSON.
#[derive(Debug, Clone, Serialize)]
pub struct OperationSpec {
    /// Stable tool id (e.g. `"gmail.send"`).
    pub id: &'static str,
    /// Logical service this operation belongs to (e.g. `"gmail"`).
    pub service: &'static str,
    /// Action verb (e.g. `"send"`, `"search"`).
    pub action: &'static str,
    /// OAuth scope groups (catalog identifiers in `apollia-auth`) the active
    /// account must hold. Empty if the operation is purely local.
    pub scopes_required: &'static [&'static str],
    /// Default HITL approval policy.
    pub approval: ApprovalPolicy,
    /// JSON Schema describing the input arguments (`2020-12` dialect).
    pub input_schema: serde_json::Value,
    /// JSON Schema describing the success output (`2020-12` dialect).
    pub output_schema: serde_json::Value,
    /// Human-readable description for the agent. Follows the canonical
    /// template (verb + when-to-use + inputs + approval + side effects) so
    /// the LLM can decide correctly when to invoke this operation.
    pub description: &'static str,
}

impl OperationSpec {
    /// True when this operation is read-only based on its approval policy.
    pub fn is_read_only(&self) -> bool {
        self.approval == ApprovalPolicy::AutoApprove
    }

    /// True when this operation requires HITL approval before execution.
    pub fn requires_approval(&self) -> bool {
        matches!(
            self.approval,
            ApprovalPolicy::AlwaysRequireApproval | ApprovalPolicy::ConfirmPhrase
        )
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(approval: ApprovalPolicy) -> OperationSpec {
        OperationSpec {
            id: "gmail.send",
            service: "gmail",
            action: "send",
            scopes_required: &["mail.send"],
            approval,
            input_schema: serde_json::json!({}),
            output_schema: serde_json::json!({}),
            description: "stub",
        }
    }

    #[test]
    fn test_auto_approve_is_read_only() {
        assert!(fixture(ApprovalPolicy::AutoApprove).is_read_only());
        assert!(!fixture(ApprovalPolicy::AutoApprove).requires_approval());
    }

    #[test]
    fn test_always_require_approval_is_not_read_only() {
        let op = fixture(ApprovalPolicy::AlwaysRequireApproval);
        assert!(!op.is_read_only());
        assert!(op.requires_approval());
    }

    #[test]
    fn test_confirm_phrase_requires_approval() {
        let op = fixture(ApprovalPolicy::ConfirmPhrase);
        assert!(!op.is_read_only());
        assert!(op.requires_approval());
    }

    #[test]
    fn test_serialization_emits_snake_case_approval() {
        // GIVEN an operation with ConfirmPhrase policy
        let op = fixture(ApprovalPolicy::ConfirmPhrase);
        // WHEN we serialize
        let json = serde_json::to_value(&op).expect("serialize");
        // THEN the approval field is emitted in snake_case (no PascalCase variant leaking)
        assert_eq!(json["approval"], "confirm_phrase");
        assert_eq!(json["id"], "gmail.send");
    }
}
