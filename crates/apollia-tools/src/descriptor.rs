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
}

impl ToolDescriptor {
    /// Validates the internal consistency of this descriptor.
    ///
    /// Called at registration time by `ToolRegistry`. Purely synchronous — no I/O.
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
    /// Standard input/output — subprocess spawned by the runtime.
    Stdio,
    /// HTTP — server runs independently, contacted via HTTP.
    Http,
    /// WebSocket — persistent bidirectional connection.
    WebSocket,
}

/// Errors produced by [`ToolDescriptor::validate`].
#[derive(Debug, Error, PartialEq, Eq)]
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
        }
    }

    #[test]
    fn test_ac1_tool_descriptor_serialization_roundtrip() {
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
    fn test_ac2_mcp_server_transport_variants() {
        // GIVEN / WHEN
        let transports = [
            McpTransport::Stdio,
            McpTransport::Http,
            McpTransport::WebSocket,
        ];
        // THEN — all variants are constructible and serializable
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
    fn test_ac3_full_profile_without_dangerous_flag_fails_validation() {
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
    fn test_ac4_empty_name_fails_validation() {
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
    fn test_ac5_null_input_schema_fails_validation() {
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
        // GIVEN — Full profile is allowed when dangerous=true
        let mut descriptor = make_valid_descriptor();
        descriptor.sandbox_profile = SandboxProfile::Full;
        descriptor.dangerous = true;
        // WHEN / THEN
        assert!(descriptor.validate().is_ok());
    }
}
