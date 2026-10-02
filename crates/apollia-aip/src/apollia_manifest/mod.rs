//! The `apollia.toml` package manifest: strict schema and single source.
//!
//! A package delivers any combination of declarative agents, Python tools and
//! Python workflows, together with the connections it needs and the MCP
//! servers it brings. This module holds the types of that manifest, its
//! parser and its validator. Every table is declared with
//! `#[serde(deny_unknown_fields)]`: a key the schema does not know is refused,
//! and the refusal names the full path of the key (for example
//! `agents[0].defaults.model.temprature`), so that what a developer writes is
//! exactly what Apollia reads.
//!
//! [`parse`] reads and validates in one call; [`ApolliaManifest::validate`]
//! re-checks a value built in memory.

mod key_path;
mod validate;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use apollia_core::AutonomyLevel;
use apollia_tools::PlanningMode;
use serde::{Deserialize, Serialize};

pub use validate::is_valid_identifier;

/// File name of the manifest at the root of a package.
pub const MANIFEST_FILE_NAME: &str = "apollia.toml";

/// Prefix of a tool delivered by a package in the shared tool registry:
/// `pkg:<package>/<tool>`.
pub const PACKAGE_TOOL_PREFIX: &str = "pkg:";

/// Identifier of the user profile in an agent's default `sources`.
pub const PROFILE_SOURCE: &str = "profile";

/// Errors returned by [`parse`] and [`ApolliaManifest::validate`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ApolliaManifestError {
    /// The text is not valid TOML, or does not match the schema. `path` is the
    /// dotted path of the offending key (`""` for the document root).
    #[error("apollia.toml: {message} at `{path}`")]
    Schema { path: String, message: String },

    /// A key the schema does not know. `path` is the full dotted path of the
    /// key itself.
    #[error("apollia.toml: unknown key `{path}`")]
    UnknownKey { path: String },

    /// The package declares no agent, no workflow and no tool.
    #[error("apollia.toml: the package declares no agent, no workflow and no tool")]
    EmptyPackage,

    /// Two items of the package share one identifier.
    #[error("apollia.toml: identifier `{id}` is declared more than once")]
    DuplicateId { id: String },

    /// An identifier holds a character outside `[a-z0-9_-]`, or is empty.
    #[error("apollia.toml: `{field}` value `{value}` must match [a-z0-9_-]+")]
    InvalidIdentifier { field: String, value: String },

    /// `[package].version` is not semver.
    #[error("apollia.toml: package version `{value}` is not semver")]
    InvalidVersion { value: String },

    /// `[package].sdk` is not a semver requirement.
    #[error("apollia.toml: sdk requirement `{value}` is not a semver requirement")]
    InvalidSdkRequirement { value: String },

    /// An agent's default access names a `pkg:` tool this package does not
    /// deliver.
    #[error("apollia.toml: agent `{agent}` grants `{access}`, which is not a tool of package `{package}`")]
    UnknownPackageTool {
        agent: String,
        access: String,
        package: String,
    },

    /// An MCP server name holds a character outside `[a-z0-9_-]`.
    #[error("apollia.toml: MCP server name `{name}` must match [a-z0-9_-]+")]
    InvalidMcpServerName { name: String },

    /// A proposed trigger targets neither an agent nor a workflow of the
    /// package.
    #[error("apollia.toml: trigger `{trigger}` targets `{target}`, which is no agent or workflow of the package")]
    UnknownTriggerTarget { trigger: String, target: String },

    /// The manifest could not be written back to TOML.
    #[error("apollia.toml: cannot serialise: {message}")]
    Serialize { message: String },
}

/// The whole `apollia.toml` document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApolliaManifest {
    /// `[package]`: identity of the package.
    pub package: PackageSection,
    /// `[[agents]]`: declarative agents. Optional: a package may deliver only
    /// tools or workflows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<AgentDecl>,
    /// `[[tools]]`: Python tools, each exposed as `pkg:<package>/<name>`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDecl>,
    /// `[[workflows]]`: Python workflows with declared steps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workflows: Vec<WorkflowDecl>,
    /// `[[requires]]`: connections the package needs, by type.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<RequiredConnection>,
    /// `[[mcp_servers]]`: MCP servers the package brings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<McpServerDecl>,
    /// `[pip]`: Python dependencies installed in the package environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pip: Option<PipSection>,
    /// `[[triggers]]`: triggers the package proposes; installing does not
    /// arm them on its own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<TriggerProposal>,
}

/// `[package]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageSection {
    /// Package identifier, `[a-z0-9_-]+`. Prefix of its tools:
    /// `pkg:<name>/<tool>`.
    pub name: String,
    /// Semver version of the package (`1.2.0`).
    pub version: String,
    /// Publisher shown at install time.
    pub publisher: String,
    /// One-paragraph description of what the package delivers.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Semver requirement on the Apollia SDK the package was built against
    /// (`">=0.3, <0.4"`).
    pub sdk: String,
}

/// `[[agents]]`: a declarative agent. No Python code is involved: the agent is
/// data, run by the built-in agent engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDecl {
    /// Identifier, `[a-z0-9_-]+`, unique in the package.
    pub id: String,
    /// Display name.
    pub name: String,
    /// What the agent is for, in the words of its author. Used as its
    /// instructions.
    pub mission: String,
    /// Example requests shown on the agent card.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
    /// Package defaults; the user's local overrides take precedence.
    #[serde(default)]
    pub defaults: AgentDefaults,
}

/// `[agents.defaults]`: values the package recommends for an agent. Each is a
/// default, never a constraint: a local override wins.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDefaults {
    /// Tools granted by default: a native tool name, `mcp:<server>/<tool>`, or
    /// `pkg:<package>/<tool>`, where a `pkg:` entry must name a tool of this
    /// package.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub access: Vec<String>,
    /// Sources checked by default.
    ///
    /// This is the package level of the per-agent source state that the
    /// runtime resolves (package default, then the user's override, unchecked
    /// when neither says anything). The entry [`PROFILE_SOURCE`] (`"profile"`)
    /// is the one the `ctx.profile` read guard consults: when it is absent
    /// and the user has not checked it, the agent cannot read the profile.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    /// Model the package recommends, read as the package level of the request
    /// settings resolution (model publisher defaults, then this level, then
    /// the user's preset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelDefaultsDecl>,
    /// Autonomy tier the package recommends: the budget, the verification and
    /// the critic the agent runs with. The user may override it; when neither
    /// says anything, the global default applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autonomy: Option<AutonomyLevel>,
    /// Whether the agent plans a task before acting on it: `never`, `auto`
    /// or `always`. The user may override it; when neither says anything, the
    /// agent plans when the task reads as a multi-step one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planning: Option<PlanningMode>,
}

/// `[agents.defaults.model]`: the shape the SDK manifest carries too, so a
/// package declares its model defaults one way wherever they are read.
pub use apollia_core::manifest::AgentModelDefaults as ModelDefaultsDecl;

/// `[agents.defaults.model.request]`: request settings. The keys are those of
/// `apollia_llm::model_defaults::ModelDefaults` (`temperature`, `top_p`,
/// `top_k`, `repetition_penalty`) and of `apollia_llm::types::CompletionRequest`
/// (`max_tokens`, `seed`), none invented.
pub use apollia_core::manifest::PackageRequestSettings as RequestSettings;

/// Action family of a tool or a workflow: what calling it does to the world.
///
/// Five families, shared by every tool whatever its origin. Every family but
/// [`ActionFamily::Read`] is sensitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionFamily {
    /// Read data that is not personal.
    Read,
    /// Read personal data (mail, calendar, a user's documents).
    ReadPersonal,
    /// Write or delete.
    WriteOrDelete,
    /// Send something outside the machine.
    SendExternal,
    /// Execute code.
    ExecuteCode,
}

impl ActionFamily {
    /// `true` for every family except [`ActionFamily::Read`].
    pub fn is_sensitive(self) -> bool {
        !matches!(self, ActionFamily::Read)
    }
}

/// `[[tools]]`: a Python tool. The descriptor is written by the SDK when the
/// package is built, so that reading the manifest never runs the code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolDecl {
    /// Identifier, `[a-z0-9_-]+`, unique in the package.
    pub name: String,
    /// Python module, relative to the package root (`tools/invoices.py`).
    pub module: String,
    /// What the tool does, for the model and for the agent card.
    pub description: String,
    /// Action family.
    pub action: ActionFamily,
    /// JSON Schema of the inputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<serde_json::Value>,
}

impl ToolDecl {
    /// `true` when the tool's family is sensitive.
    pub fn is_sensitive(&self) -> bool {
        self.action.is_sensitive()
    }
}

/// `[[workflows]]`: a Python workflow with steps declared ahead of time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowDecl {
    /// Identifier, `[a-z0-9_-]+`, unique in the package.
    pub name: String,
    /// Python module, relative to the package root.
    pub module: String,
    /// What the workflow does.
    pub description: String,
    /// Action family: the most sensitive thing the workflow does.
    pub action: ActionFamily,
    /// JSON Schema of the inputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<serde_json::Value>,
    /// Names of the steps, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<String>,
}

impl WorkflowDecl {
    /// `true` when the workflow's family is sensitive.
    pub fn is_sensitive(&self) -> bool {
        self.action.is_sensitive()
    }
}

/// `[[requires]]`: a connection the package needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredConnection {
    /// Connection type, as the connection catalogue names it (`gmail`).
    #[serde(rename = "type")]
    pub kind: String,
    /// Why the package needs it, shown when it is missing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

/// `[[mcp_servers]]`: an MCP server the package brings.
///
/// Same keys, defaults and meaning as `apollia_mcp::config::McpServerConfig`,
/// minus `format_version`, which versions the `mcp.toml` file and not a
/// package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpServerDecl {
    /// Server name, `[a-z0-9_-]+`; prefix of its tools: `mcp:<name>/<tool>`.
    pub name: String,
    /// Executable to spawn; empty for network transports.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    /// Arguments forwarded to the command.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Environment of the server process; `${VAR}` placeholders allowed.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// `stdio`, `streamable-http` or `sse`.
    #[serde(default = "default_transport")]
    pub transport: String,
    /// Server URL for network transports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Every call requires approval.
    #[serde(default)]
    pub requires_approval: bool,
    /// `initialize` handshake timeout, in seconds.
    #[serde(default = "default_init_timeout")]
    pub init_timeout_secs: u64,
    /// `tools/call` timeout, in seconds.
    #[serde(default = "default_call_timeout")]
    pub call_timeout_secs: u64,
    /// Cap on one server response, in bytes.
    #[serde(default = "default_max_response_bytes")]
    pub max_response_bytes: u64,
    /// Cap on the tools retained from `tools/list`.
    #[serde(default = "default_max_tools")]
    pub max_tools: u32,
    /// Tags applied to every tool of the server.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

fn default_transport() -> String {
    "stdio".to_string()
}

fn default_init_timeout() -> u64 {
    30
}

fn default_call_timeout() -> u64 {
    60
}

fn default_max_response_bytes() -> u64 {
    8 * 1024 * 1024
}

fn default_max_tools() -> u32 {
    256
}

/// `[pip]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipSection {
    /// Requirements, pinned (`pkg==X.Y.Z`) by recommendation.
    #[serde(default)]
    pub packages: Vec<String>,
}

/// `[[triggers]]`: a trigger the package proposes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerProposal {
    /// Identifier, `[a-z0-9_-]+`, unique among the triggers.
    pub id: String,
    /// Identifier of the agent or workflow of this package that it starts.
    pub target: String,
    /// Message (for an agent) sent at each fire; may be empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub input: String,
    /// When it fires.
    pub source: TriggerSource,
}

/// `[triggers.source]`, tagged by `type`.
///
/// A webhook carries no secret here: a secret in a distributed file is no
/// secret, it is set at install time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TriggerSource {
    /// Cron expression (`"0 8 * * MON"`).
    Cron { schedule: String },
    /// Fixed interval (`"30m"`, `"1h"`, `"1d"`).
    Interval { every: String },
    /// A folder the user picks at install time.
    FileWatch {
        #[serde(default)]
        recursive: bool,
    },
    /// An HTTP webhook.
    Webhook {},
}

/// Parse and validate an `apollia.toml` document.
///
/// # Errors
///
/// [`ApolliaManifestError::UnknownKey`] for a key the schema does not know,
/// with its full path; [`ApolliaManifestError::Schema`] for any other
/// mismatch; one of the validation variants when a rule of
/// [`ApolliaManifest::validate`] fails.
pub fn parse(text: &str) -> Result<ApolliaManifest, ApolliaManifestError> {
    let manifest: ApolliaManifest =
        toml::from_str(text).map_err(|e| key_path::schema_error(text, &e))?;
    manifest.validate()?;
    Ok(manifest)
}

impl ApolliaManifest {
    /// Write the manifest back to TOML.
    ///
    /// # Errors
    ///
    /// [`ApolliaManifestError::Serialize`] when a value has no TOML form (an
    /// input schema holding a JSON `null`, for instance).
    pub fn to_toml(&self) -> Result<String, ApolliaManifestError> {
        toml::to_string(self).map_err(|e| ApolliaManifestError::Serialize {
            message: e.to_string(),
        })
    }
}
