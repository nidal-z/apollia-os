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
    /// What the tool does, in one of the action families shared by native
    /// tools, connectors and MCP tools. Computed when the descriptor is built,
    /// never by the model. Absent from a payload, it reads as
    /// [`ActionFamily::Unclassified`], which is treated as sensitive.
    #[serde(default)]
    pub family: ActionFamily,
    /// Where [`Self::family`] comes from.
    #[serde(default)]
    pub family_source: FamilySource,
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

/// The action family of a tool: what it does to the user's data or machine.
///
/// The five families are the vocabulary the agent sheet and the governance
/// layer share. `Unclassified` is the state of a tool nobody has classified
/// yet and is treated as sensitive; `Exempt` is reserved to a tool that acts on
/// nothing but the conversation itself (`ask_user`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionFamily {
    /// Reads data the user explicitly granted, or public data.
    Read,
    /// Reads personal data held by a connected account (mail, calendar, files).
    ReadPersonal,
    /// Creates, modifies or deletes data.
    WriteOrDelete,
    /// Sends something outside the machine (a message, a request).
    SendExternal,
    /// Executes code the model wrote.
    ExecuteCode,
    /// Not classified. Treated as sensitive.
    #[default]
    Unclassified,
    /// Outside the families: acts on the conversation only.
    Exempt,
}

impl ActionFamily {
    /// The family a standing consent covers for this tool, `None` for a tool
    /// that is unclassified or exempt, which no standing consent can cover.
    pub fn consent_family(self) -> Option<apollia_permissions::ConsentFamily> {
        use apollia_permissions::ConsentFamily;
        match self {
            ActionFamily::Read => Some(ConsentFamily::Read),
            ActionFamily::ReadPersonal => Some(ConsentFamily::ReadPersonal),
            ActionFamily::WriteOrDelete => Some(ConsentFamily::WriteOrDelete),
            ActionFamily::SendExternal => Some(ConsentFamily::SendExternal),
            ActionFamily::ExecuteCode => Some(ConsentFamily::ExecuteCode),
            ActionFamily::Unclassified | ActionFamily::Exempt => None,
        }
    }
}

/// Origin of a descriptor's [`ActionFamily`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FamilySource {
    /// Deduced from the hints the tool's server declares.
    Server,
    /// Taken from the classification of the curated selection.
    Selection,
    /// No source: the family is `Unclassified`.
    #[default]
    #[serde(rename = "none")]
    Unsourced,
    /// Fixed in Apollia's own code (native tools, native connectors).
    Builtin,
    /// Declared by the package that ships the tool, in its `apollia.toml`.
    Package,
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

/// Upper bound of the [`ToolDescriptor::risk_score`] scale.
///
/// The scale is 0 to 10 and [`ApprovalRiskLevel::from_risk_score`] reads it that
/// way. A descriptor written on some other scale keeps its own meaning and loses
/// the one every reader assumes, so the bound is checked rather than described.
pub const MAX_RISK_SCORE: u8 = 10;

impl ToolDescriptor {
    /// Validates the internal consistency of this descriptor.
    ///
    /// Called at registration time by `ToolRegistry`. Purely synchronous, no I/O.
    ///
    /// # Errors
    ///
    /// Returns `ToolDescriptorError::EmptyName` if `name` is empty.
    /// Returns `ToolDescriptorError::InvalidInputSchema` if `input_schema` is `Null`.
    /// Returns `ToolDescriptorError::FullProfileRequiresExecuteCode` if
    /// `sandbox_profile` is `Full` but `family` is not [`ActionFamily::ExecuteCode`].
    /// Returns `ToolDescriptorError::RiskScoreOutOfRange` if `risk_score` is above
    /// [`MAX_RISK_SCORE`].
    pub fn validate(&self) -> Result<(), ToolDescriptorError> {
        if self.name.is_empty() {
            return Err(ToolDescriptorError::EmptyName);
        }

        if self.input_schema.is_null() {
            return Err(ToolDescriptorError::InvalidInputSchema);
        }

        if self.sandbox_profile.requires_dangerous_flag()
            && self.family != ActionFamily::ExecuteCode
        {
            return Err(ToolDescriptorError::FullProfileRequiresExecuteCode);
        }

        if self.risk_score > MAX_RISK_SCORE {
            return Err(ToolDescriptorError::RiskScoreOutOfRange {
                score: self.risk_score,
            });
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
    /// Python tool shipped by an installed package, `pkg:<package>/<tool>`.
    /// Its module is loaded on the first call, never at registration.
    Package {
        /// Name of the package that ships the tool.
        package: String,
        /// Python module, relative to the package root (`tools/invoices.py`).
        module: String,
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
    /// `sandbox_profile` is `Full` but `family` is not `ExecuteCode`.
    #[error("SandboxProfile::Full requires the execute_code family")]
    FullProfileRequiresExecuteCode,
    /// `input_schema` is `serde_json::Value::Null`.
    #[error("input_schema must not be null")]
    InvalidInputSchema,
    /// `risk_score` is above [`MAX_RISK_SCORE`].
    #[error("risk_score {score} is outside the 0-{MAX_RISK_SCORE} scale")]
    RiskScoreOutOfRange {
        /// The declared score, as written in the descriptor.
        score: u8,
    },
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
            family: ActionFamily::ExecuteCode,
            family_source: FamilySource::Builtin,
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
    fn test_full_profile_outside_execute_code_fails_validation() {
        // GIVEN a Full-profile descriptor whose family is not execute_code
        let mut descriptor = make_valid_descriptor();
        descriptor.sandbox_profile = SandboxProfile::Full;
        descriptor.family = ActionFamily::WriteOrDelete;
        // WHEN
        let result = descriptor.validate();
        // THEN
        assert!(matches!(
            result,
            Err(ToolDescriptorError::FullProfileRequiresExecuteCode)
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
    fn test_full_profile_with_execute_code_family_passes_validation() {
        // GIVEN: Full profile is allowed for the execute_code family
        let mut descriptor = make_valid_descriptor();
        descriptor.sandbox_profile = SandboxProfile::Full;
        descriptor.family = ActionFamily::ExecuteCode;
        // WHEN / THEN
        assert!(descriptor.validate().is_ok());
    }

    #[test]
    fn validate_refuses_a_risk_score_off_the_scale() {
        // GIVEN a descriptor whose risk_score was written on a 0-100 scale
        let mut d = make_valid_descriptor();
        d.risk_score = 60;

        // WHEN it is validated
        let result = d.validate();

        // THEN it is refused, carrying the offending value
        assert!(
            matches!(
                result,
                Err(ToolDescriptorError::RiskScoreOutOfRange { score: 60 })
            ),
            "expected RiskScoreOutOfRange, got {result:?}"
        );
    }

    #[test]
    fn validate_accepts_the_top_of_the_scale() {
        // GIVEN a descriptor at the maximum of the declared scale
        let mut d = make_valid_descriptor();
        d.risk_score = MAX_RISK_SCORE;

        // WHEN it is validated
        // THEN it passes: the bound refuses what is above it, not what reaches it
        assert!(d.validate().is_ok());
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
            "tags": []
        }"#;
        // WHEN
        let d: ToolDescriptor = serde_json::from_str(json).expect("deserialisation failed");
        // THEN: fields absent in JSON fall back to default values
        assert!(!d.is_read_only);
        assert_eq!(d.risk_score, 0);
        assert!(d.approval_risk_level.is_none());
        assert!(d.impact_description.is_none());
        assert!(!d.reject_reason_required);
        assert_eq!(d.family, ActionFamily::Unclassified);
        assert_eq!(d.family_source, FamilySource::Unsourced);
    }

    #[test]
    fn consent_family_keeps_the_wire_name_and_drops_the_unconsentable() {
        // GIVEN every action family
        let families = [
            ActionFamily::Read,
            ActionFamily::ReadPersonal,
            ActionFamily::WriteOrDelete,
            ActionFamily::SendExternal,
            ActionFamily::ExecuteCode,
            ActionFamily::Unclassified,
            ActionFamily::Exempt,
        ];
        for family in families {
            // WHEN it is turned into the family a standing consent covers
            let consent = family.consent_family();
            // THEN the five families keep their wire name, and the
            // unclassified and exempt ones have none
            let wire = serde_json::to_value(family).expect("serialise");
            match family {
                ActionFamily::Unclassified | ActionFamily::Exempt => assert!(consent.is_none()),
                _ => assert_eq!(consent.map(|c| c.as_str()), wire.as_str()),
            }
        }
    }

    #[test]
    fn every_action_family_round_trips_through_serde() {
        // GIVEN every family with the wire identifier it must carry
        let cases = [
            (ActionFamily::Read, "read"),
            (ActionFamily::ReadPersonal, "read_personal"),
            (ActionFamily::WriteOrDelete, "write_or_delete"),
            (ActionFamily::SendExternal, "send_external"),
            (ActionFamily::ExecuteCode, "execute_code"),
            (ActionFamily::Unclassified, "unclassified"),
            (ActionFamily::Exempt, "exempt"),
        ];
        for (family, wire) in cases {
            // WHEN it is serialised then read back
            let json = serde_json::to_value(family).expect("serialisation failed");
            let back: ActionFamily = serde_json::from_value(json.clone()).expect("read back");
            // THEN the wire identifier is the documented one and nothing is lost
            assert_eq!(json, wire);
            assert_eq!(back, family);
        }
    }

    #[test]
    fn every_family_source_round_trips_through_serde() {
        // GIVEN every source with the wire identifier it must carry
        let cases = [
            (FamilySource::Server, "server"),
            (FamilySource::Selection, "selection"),
            (FamilySource::Unsourced, "none"),
            (FamilySource::Builtin, "builtin"),
        ];
        for (source, wire) in cases {
            // WHEN it is serialised then read back
            let json = serde_json::to_value(source).expect("serialisation failed");
            let back: FamilySource = serde_json::from_value(json.clone()).expect("read back");
            // THEN the wire identifier is the documented one and nothing is lost
            assert_eq!(json, wire);
            assert_eq!(back, source);
        }
    }

    #[test]
    fn a_descriptor_carries_its_family_and_source_on_the_wire() {
        // GIVEN a descriptor classified as sending outside, from the selection
        let mut d = make_valid_descriptor();
        d.family = ActionFamily::SendExternal;
        d.family_source = FamilySource::Selection;
        // WHEN it is serialised then read back
        let json = serde_json::to_value(&d).expect("serialisation failed");
        let back: ToolDescriptor = serde_json::from_value(json.clone()).expect("read back");
        // THEN both fields travel, under their documented names
        assert_eq!(json["family"], "send_external");
        assert_eq!(json["family_source"], "selection");
        assert_eq!(back.family, ActionFamily::SendExternal);
        assert_eq!(back.family_source, FamilySource::Selection);
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
        let json = serde_json::to_value(&d).expect("serialisation failed");
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
        let json = serde_json::to_string(&d).expect("serialisation failed");
        let d2: ToolDescriptor = serde_json::from_str(&json).expect("deserialisation failed");
        // THEN
        assert!(d2.is_read_only);
        assert_eq!(d2.risk_score, 2);
    }
}
