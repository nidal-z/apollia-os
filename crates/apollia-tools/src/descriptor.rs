use apollia_core::SandboxProfile;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Describes a tool available in the Apollia OS catalogue.
///
/// Aligned with the MCP schema for native interoperability with the ecosystem
/// (16K+ MCP servers). See `docs/Briques-Tool-Registry.md` section 2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// Unique tool identifier (e.g. `bash_executor`). Must be non-empty.
    pub name: String,
    /// Semver version string (e.g. `1.0.0`).
    pub version: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// Implementation kind: native Rust, external MCP server, or custom Python.
    pub kind: ToolKind,
    /// JSON Schema describing the tool's input parameters. Must not be Null.
    pub input_schema: serde_json::Value,
    /// Optional JSON Schema describing the tool's output.
    pub output_schema: Option<serde_json::Value>,
    /// Linux namespace isolation profile applied at execution time.
    pub sandbox_profile: SandboxProfile,
    /// Arbitrary tags for filtering and discovery.
    pub tags: Vec<String>,
    /// Must be `true` when `sandbox_profile` is `SandboxProfile::Full`.
    /// Forces an explicit opt-in to unrestricted execution.
    pub dangerous: bool,
    /// If `true`, this tool does not modify any state and may be executed concurrently
    /// alongside other read-only tools. Defaults to `false`: if this flag is omitted,
    /// the tool is treated as mutating and never parallelised.
    #[serde(default)]
    pub is_read_only: bool,
    /// Risk score from 0 (pure read) to 10 (full system modification).
    /// Used for audit trail enrichment and approval-gate decisions.
    #[serde(default)]
    pub risk_score: u8,
    /// Coarse-grained risk level surfaced in the approval UI
    /// (badge colour, nested-approval treatment).
    /// If `None`, the UI derives a level from `risk_score` (0-3 low, 4-6 medium,
    /// 7-8 high, 9-10 critical).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_risk_level: Option<ApprovalRiskLevel>,
    /// Human-readable, pre-rendered impact description displayed inside the
    /// approval card (e.g. "Deletes the file permanently").
    /// Falls back to a generic sentence derived from the manifest name if
    /// `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impact_description: Option<String>,
    /// When `true`, the frontend must require a textual reject reason before
    /// resolving the approval. Defaults to `false`.
    #[serde(default)]
    pub reject_reason_required: bool,
}

/// Coarse-grained risk classification surfaced in approval UIs.
///
/// Ordered from least to most impactful. The desktop shell colours the badge
/// (green/yellow/orange/red) and requires extra confirmation for `Critical`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalRiskLevel {
    /// Pure read or idempotent action with negligible blast radius.
    Low,
    /// Mutating action whose effect is easily reversible.
    Medium,
    /// Mutating action that touches user data, credentials, or network state.
    High,
    /// Destructive or irreversible action (data deletion, privileged exec, …).
    Critical,
}

impl ApprovalRiskLevel {
    /// Best-effort classification from the `risk_score` field when no explicit
    /// level is declared by the tool author.
    #[must_use]
    pub fn from_risk_score(score: u8) -> Self {
        match score {
            0..=3 => Self::Low,
            4..=6 => Self::Medium,
            7..=8 => Self::High,
            _ => Self::Critical,
        }
    }
}

impl ToolDescriptor {
    /// Validates the internal consistency of this descriptor.
    ///
    /// Called at registration time by `ToolRegistry`. Purely synchronous, no I/O.
    ///
    /// # Errors
    ///
    /// Returns `ToolDescriptorError::EmptyName` if `name` is empty.
    /// Returns `ToolDescriptorError::InvalidInputSchema` if `input_schema` is `Null`.
    /// Returns `ToolDescriptorError::FullProfileRequiresDangerous` if `sandbox_profile`
    /// is `Full` but `dangerous` is `false`.
    pub fn validate(&self) -> Result<(), ToolDescriptorError> {
        if self.name.is_empty() {
            return Err(ToolDescriptorError::EmptyName);
        }

        if self.input_schema.is_null() {
            return Err(ToolDescriptorError::InvalidInputSchema);
        }

        if self.sandbox_profile.requires_dangerous_flag() && !self.dangerous {
            return Err(ToolDescriptorError::FullProfileRequiresDangerous);
        }

        Ok(())
    }
}

/// Tool implementation kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolKind {
    /// Implemented in Rust inside `apollia-tools`. Zero external process spawn.
    Native,
    /// External MCP server reachable via stdio, HTTP, or WebSocket.
    McpServer {
        /// URL or path identifying the server (e.g. `http://localhost:3000`).
        server_url: String,
        /// Wire protocol used to communicate with the server.
        transport: McpTransport,
        /// Name of the specific tool exposed by that server.
        tool_name: String,
    },
    /// Python tool registered by the agent author at runtime.
    Custom {
        /// Dotted Python module path (e.g. `my_package.tools`).
        module_path: String,
        /// Class name inside that module implementing the tool contract.
        class_name: String,
    },
}

/// Wire transport protocol for an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    /// Standard input/output: subprocess spawned by the runtime.
    Stdio,
    /// HTTP: server runs independently, contacted via HTTP.
    Http,
    /// WebSocket: persistent bidirectional connection.
    WebSocket,
}

/// Errors produced by [`ToolDescriptor::validate`].
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ToolDescriptorError {
    /// `name` field is an empty string.
    #[error("tool name must not be empty")]
    EmptyName,
    /// `version` does not conform to expected format.
    #[error("invalid version string: {0}")]
    InvalidVersion(String),
    /// `sandbox_profile` is `Full` but `dangerous` is `false`.
    #[error("SandboxProfile::Full requires dangerous=true")]
    FullProfileRequiresDangerous,
    /// `input_schema` is `serde_json::Value::Null`.
    #[error("input_schema must not be null")]
    InvalidInputSchema,
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::SandboxProfile;
    use serde_json::json;

    fn make_valid_descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: "bash_executor".to_string(),
            version: "1.0.0".to_string(),
            description: "Execute shell commands".to_string(),
            kind: ToolKind::Native,
            input_schema: json!({ "type": "object" }),
            output_schema: None,
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec!["shell".to_string()],
            dangerous: false,
            is_read_only: false,
            risk_score: 0,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }

    #[test]
    fn test_tool_descriptor_serialization_roundtrip() {
        // GIVEN
        let descriptor = make_valid_descriptor();
        // WHEN
        let json = serde_json::to_string(&descriptor).expect("serialization failed");
        let roundtrip: ToolDescriptor =
            serde_json::from_str(&json).expect("deserialization failed");
        // THEN
        assert_eq!(descriptor.name, roundtrip.name);
        assert_eq!(descriptor.version, roundtrip.version);
    }

    #[test]
    fn test_mcp_server_transport_variants() {
        // GIVEN / WHEN
        let transports = [
            McpTransport::Stdio,
            McpTransport::Http,
            McpTransport::WebSocket,
        ];
        // THEN: all variants are constructible and serializable
        for transport in transports {
            let kind = ToolKind::McpServer {
                server_url: "http://localhost:3000".to_string(),
                transport,
                tool_name: "search".to_string(),
            };
            let json = serde_json::to_string(&kind).expect("serialization failed");
            assert!(!json.is_empty());
        }
    }

    #[test]
    fn test_full_profile_without_dangerous_flag_fails_validation() {
        // GIVEN
        let mut descriptor = make_valid_descriptor();
        descriptor.sandbox_profile = SandboxProfile::Full;
        descriptor.dangerous = false;
        // WHEN
        let result = descriptor.validate();
        // THEN
        assert!(matches!(
            result,
            Err(ToolDescriptorError::FullProfileRequiresDangerous)
        ));
    }

    #[test]
    fn test_empty_name_fails_validation() {
        // GIVEN
        let mut descriptor = make_valid_descriptor();
        descriptor.name = "".to_string();
        // WHEN / THEN
        assert!(matches!(
            descriptor.validate(),
            Err(ToolDescriptorError::EmptyName)
        ));
    }

    #[test]
    fn test_null_input_schema_fails_validation() {
        // GIVEN
        let mut descriptor = make_valid_descriptor();
        descriptor.input_schema = serde_json::Value::Null;
        // WHEN / THEN
        assert!(matches!(
            descriptor.validate(),
            Err(ToolDescriptorError::InvalidInputSchema)
        ));
    }

    #[test]
    fn test_valid_descriptor_passes_validation() {
        // GIVEN
        let descriptor = make_valid_descriptor();
        // WHEN / THEN
        assert!(descriptor.validate().is_ok());
    }

    #[test]
    fn test_full_profile_with_dangerous_flag_passes_validation() {
        // GIVEN: Full profile is allowed when dangerous=true
        let mut descriptor = make_valid_descriptor();
        descriptor.sandbox_profile = SandboxProfile::Full;
        descriptor.dangerous = true;
        // WHEN / THEN
        assert!(descriptor.validate().is_ok());
    }

    #[test]
    fn test_is_read_only_defaults_to_false() {
        // GIVEN: descriptor built without specifying is_read_only
        let d = make_valid_descriptor();
        // WHEN the flag is read
        // THEN: conservative default, never parallelised if flag omitted
        assert!(!d.is_read_only);
    }

    #[test]
    fn test_risk_score_defaults_to_zero() {
        // GIVEN a descriptor built without naming a risk score
        let d = make_valid_descriptor();
        // WHEN the score is read
        // THEN it defaults to zero, so a tool is harmless until it says otherwise
        assert_eq!(d.risk_score, 0);
    }

    #[test]
    fn test_is_read_only_serde_default() {
        // GIVEN: JSON without is_read_only / risk_score fields
        let json = r#"{
            "name": "file_read", "version": "1.0.0", "description": "reads a file",
            "kind": {"type": "native"},
            "input_schema": {"type": "object"},
            "sandbox_profile": "read_only",
            "tags": [], "dangerous": false
        }"#;
        // WHEN
        let d: ToolDescriptor = serde_json::from_str(json).expect("deserialisation échouée");
        // THEN: fields absent in JSON fall back to default values
        assert!(!d.is_read_only);
        assert_eq!(d.risk_score, 0);
        assert!(d.approval_risk_level.is_none());
        assert!(d.impact_description.is_none());
        assert!(!d.reject_reason_required);
    }

    #[test]
    fn test_approval_risk_level_from_score() {
        // GIVEN / WHEN / THEN: monotonic mapping across the score domain
        assert_eq!(
            ApprovalRiskLevel::from_risk_score(0),
            ApprovalRiskLevel::Low
        );
        assert_eq!(
            ApprovalRiskLevel::from_risk_score(3),
            ApprovalRiskLevel::Low
        );
        assert_eq!(
            ApprovalRiskLevel::from_risk_score(4),
            ApprovalRiskLevel::Medium
        );
        assert_eq!(
            ApprovalRiskLevel::from_risk_score(6),
            ApprovalRiskLevel::Medium
        );
        assert_eq!(
            ApprovalRiskLevel::from_risk_score(7),
            ApprovalRiskLevel::High
        );
        assert_eq!(
            ApprovalRiskLevel::from_risk_score(9),
            ApprovalRiskLevel::Critical
        );
        assert_eq!(
            ApprovalRiskLevel::from_risk_score(10),
            ApprovalRiskLevel::Critical
        );
    }

    #[test]
    fn test_approval_fields_serialize_when_set() {
        // GIVEN: descriptor with explicit approval fields
        let mut d = make_valid_descriptor();
        d.approval_risk_level = Some(ApprovalRiskLevel::High);
        d.impact_description = Some("Deletes the file permanently".to_string());
        d.reject_reason_required = true;
        // WHEN
        let json = serde_json::to_value(&d).expect("serialisation échouée");
        // THEN
        assert_eq!(json["approval_risk_level"], "high");
        assert_eq!(json["impact_description"], "Deletes the file permanently");
        assert_eq!(json["reject_reason_required"], true);
    }

    #[test]
    fn test_is_read_only_serialization_roundtrip() {
        // GIVEN: read-only tool
        let mut d = make_valid_descriptor();
        d.is_read_only = true;
        d.risk_score = 2;
        // WHEN
        let json = serde_json::to_string(&d).expect("serialisation échouée");
        let d2: ToolDescriptor = serde_json::from_str(&json).expect("déserialisation échouée");
        // THEN
        assert!(d2.is_read_only);
        assert_eq!(d2.risk_score, 2);
    }
}
