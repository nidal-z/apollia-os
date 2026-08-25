use serde::{Deserialize, Serialize};

use crate::budget::StepBudgetConfig;

/// Default value for `AgentManifest::max_concurrent_tasks`.
fn default_max_concurrent_tasks() -> u32 {
    1
}

/// Default value for `AgentManifest::execution_mode`.
fn default_execution_mode() -> String {
    "auto".to_string()
}

/// Per-agent memory retention configuration.
///
/// Overrides the global `[memory]` `default_retention_days` from `apollia.toml`.
/// A `None` value for a type means no age-based purge is applied for that type.
/// `auto_purge = true` triggers a single purge pass when the `MemoryManager` starts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Episodic memory retention in days (`None` = inherit global default, no forced expiry).
    pub episodic_retention_days: Option<u32>,
    /// Semantic memory retention in days (`None` = no age-based expiry).
    pub semantic_retention_days: Option<u32>,
    /// Procedural memory retention in days (`None` = no age-based expiry).
    pub procedural_retention_days: Option<u32>,
    /// Trigger a purge pass at `MemoryManager` startup (opt-in, default `false`).
    #[serde(default)]
    pub auto_purge: bool,
}

/// Current version of the persisted format, and the value read for files
/// written before the key existed.
fn default_format_version() -> u32 {
    1
}

/// Declared identity and capabilities of an agent.
///
/// Single source of truth for tool resolution and runtime configuration at
/// agent startup (INITIALIZING state).
/// Returned by the `manifest()` method of each Python agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    /// Version of this on-disk format. A file written before versioning has
    /// no key and reads as format 1; a future format bumps the default and the
    /// reader branches on the value it finds.
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    /// Unique name of the agent within the runtime.
    pub name: String,
    /// Semver version (e.g. "1.0.0").
    pub version: String,
    /// Human-readable description of the agent.
    pub description: String,
    /// Required tools, resolved fail-fast at the INITIALIZING state.
    pub tools_required: Vec<String>,
    /// Optional tools: when absent, the agent enters DEGRADED state rather than failing.
    #[serde(default)]
    pub tools_optional: Vec<String>,
    /// Whether the agent supports streaming mode (default: false).
    #[serde(default)]
    pub supports_streaming: bool,
    /// Whether the agent supports the Agent-to-Agent protocol (default: false).
    #[serde(default)]
    pub supports_a2a: bool,
    /// Whether the agent may use the asynchronous mailbox (`ctx.mail`).
    ///
    /// Opt-in, like other sensitive surfaces: `false` by default, in which case
    /// `ctx.mail` refuses every call. Declared in the Python manifest via
    /// `@agent(mailbox=True)` or `@agent(mailbox=("worker-a", ...))`.
    #[serde(default)]
    pub supports_mailbox: bool,
    /// Optional allowlist of recipients this agent may message.
    ///
    /// `None` means "any registered agent" (once `supports_mailbox` is true).
    /// A `Some(list)` restricts `ctx.mail.send` to the named recipients.
    #[serde(default)]
    pub mailbox_allowlist: Option<Vec<String>>,
    /// Primary memory namespace of the agent.
    ///
    /// Declared statically in the Python manifest. When the agent runs in a
    /// project context, the runtime prefixes it automatically with the
    /// `project_id`: effective namespace = `"{project_id}:{memory_namespace}"`.
    ///
    /// If `None`, the agent has no access to persistent memory.
    #[serde(default)]
    pub memory_namespace: Option<String>,
    /// Shared memory namespaces accessible for reading.
    #[serde(default)]
    pub shared_memory_namespaces: Vec<String>,
    /// Maximum number of concurrent tasks (default: 1).
    #[serde(default = "default_max_concurrent_tasks")]
    pub max_concurrent_tasks: u32,
    /// Override of the runtime default step budget (None = use the default).
    #[serde(default)]
    pub step_budget: Option<StepBudgetConfig>,
    /// Network allowlist (None = no network access permitted).
    #[serde(default)]
    pub network_allowlist: Option<Vec<String>>,
    /// Explicitly allows the use of tools marked `dangerous=true`.
    /// `false` by default: dangerous tools are blocked unless explicitly opted in.
    #[serde(default)]
    pub dangerous_tools_allowed: bool,
    /// Free-form tags for routing and discovery.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Declarative skills of the agent (used to build the A2A card).
    #[serde(default)]
    pub skills: Vec<AgentSkill>,
    /// ORIA execution mode: `"auto"` | `"direct"` | `"orchestrated"`.
    ///
    /// - `"auto"` (default): the `classify()` heuristic decides.
    /// - `"direct"`: forces Direct mode regardless of the manifest.
    /// - `"orchestrated"`: forces Orchestrated mode regardless of the manifest.
    ///
    /// Any unknown value is treated as `"auto"` without error.
    #[serde(default = "default_execution_mode")]
    pub execution_mode: String,
    /// System prompt given to ORIA to plan orchestrated execution.
    ///
    /// Required when `execution_mode = "orchestrated"`.
    /// Ignored for other modes.
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Tools requiring human approval before execution in Orchestrated mode.
    ///
    /// Declared by the agent in its `manifest()`. The runtime refuses to run a step
    /// using these tools without explicit approval.
    /// Empty by default: no tool requires approval.
    #[serde(default)]
    pub tools_requiring_approval: Vec<String>,
    /// Name of the LLM backend to use for this agent.
    ///
    /// Must match the `name` field of an entry in `llm_backends` (SQLite system.db).
    /// If absent or `None`, the runtime uses the backend marked `is_default = true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub llm_backend: Option<String>,
    /// pip packages to install in this agent's Python venv.
    ///
    /// Standard pip syntax: `"openpyxl>=3.1.0"`, `"pandas==2.1.4"`, `"requests"`.
    /// Installed once at `INITIALIZING` via `PythonExecutor::setup_venv()`.
    /// Empty by default: agents without third-party Python dependencies.
    #[serde(default)]
    pub packages: Vec<String>,
    /// Per-agent memory retention policy (`None` = global `[memory]` defaults apply).
    ///
    /// When present, overrides the global retention thresholds for this agent's namespace.
    /// `auto_purge = true` inside this config triggers a purge at runtime startup.
    #[serde(default)]
    pub memory_config: Option<MemoryConfig>,
    /// Semantic role of the agent in the system.
    ///
    /// - `"worker"`    : operational agent, called by other agents via A2A, stateless.
    /// - `"assistant"` : human interlocutor, multi-turn, orchestrates workers.
    /// - `"system"`    : internal infrastructure (onboarding, supervision, etc.).
    /// - `None`        : not declared, agents predating the v2 contract.
    ///
    /// The UI uses this field to categorize agents. Since `supports_a2a` is true
    /// for both populations, it is not enough to distinguish them.
    #[serde(default)]
    pub agent_type: Option<String>,

    // ── User documentation (v2 contract) ───────────────────────────────────────
    //
    // These three fields are optional and intended for agent developers.
    // They allow documenting the agent directly in the Python manifest, without
    // an external file that could drift from the implementation.
    // The Apollia UI displays them in the agent detail panel.
    //
    // Approach: declarative, colocated with the code, no free prose.
    // Comparable references: Microsoft Copilot (conversation_starters),
    // Hugging Face Model Card (intended use, limitations), MCP (description).
    // No existing standard offers these three fields in a structured way, which
    // is an Apollia differentiator.
    /// Example prompts illustrating typical uses of the agent.
    ///
    /// Displayed in the UI as clickable quick-start chips.
    /// Guideline: 2 to 5 examples, phrased as real user requests.
    /// Empty by default: agents without examples do not show this section.
    #[serde(default)]
    pub examples: Vec<String>,

    /// Explicit limitations of the agent: what it refuses or cannot do.
    ///
    /// Displayed in the detail panel to set realistic expectations.
    /// Guideline: 2 to 4 points, phrased in the first person or the infinitive.
    /// Empty by default: agents without declared limitations do not show this
    /// section.
    #[serde(default)]
    pub limitations: Vec<String>,

    /// Note about configuration required before first use.
    ///
    /// Displayed in the UI only when not `None`. Must clearly indicate what the
    /// user needs to set up (files, tools, complementary agents, permissions).
    /// Avoid lists, a short paragraph is enough.
    /// `None` means no prerequisite: the agent starts immediately.
    #[serde(default)]
    pub setup_notes: Option<String>,

    /// Name of the source Python class of the agent (e.g. `"VeilleIaAgent"`,
    /// `"ApolliaGuide"`, `"DocxWorker"`).
    ///
    /// The Python class is the source of truth for the agent type. Filled by the
    /// AIP validator from `self.__class__.__name__`. `None` for agents built
    /// outside the PyO3 pipeline (tests, fixtures).
    #[serde(default)]
    pub agent_class: Option<String>,

    /// Allows the agent to write to the global `__user__` namespace via
    /// `ctx.memory.remember_user()`. False by default. Reading is always
    /// available through the `recall()` fallback.
    ///
    /// Reserved for system agents that legitimately own the user profile
    /// (e.g. `onboarding-agent`).
    #[serde(default)]
    pub user_memory_write: bool,

    /// Logical names of the YAML datasources declared via `@agent(datasources=...)`.
    ///
    /// Each name must match a file `<agent_dir>/datasources/<name>.yaml`.
    /// The runtime loads these files at boot and exposes them via `ctx.datasources.get(name)`.
    /// An undeclared datasource raises `FileNotFoundError` on the Python side
    /// even if the file exists on disk (least-privilege principle).
    /// Empty by default: agents without datasources.
    #[serde(default)]
    pub datasources: Vec<String>,

    /// Logical names of the Jinja2 templates declared via `@agent(templates=...)`.
    ///
    /// Each name must match a file `<agent_dir>/templates/<name>.{j2,jinja2,jinja}`.
    /// The runtime compiles these templates at boot and exposes them via
    /// `ctx.templates.render(name, **context)`.
    /// Empty by default: agents without templates.
    #[serde(default)]
    pub templates: Vec<String>,

    /// Logical names of the secrets declared via `@agent(secrets=...)`.
    ///
    /// Strict allowlist of credential keys the agent may read from
    /// `ctx.secrets.get(key)`. The runtime resolves each key via the encrypted
    /// [`ToolCredentialStore`](apollia_tools::ToolCredentialStore)
    /// (AES-256-GCM) using the tool namespace `"agent"`.
    ///
    /// **Gating**: an undeclared key always returns `None` on the Python side,
    /// even if the value exists in the store. A direct consequence of the
    /// least-privilege principle.
    ///
    /// **Read only**: the agent cannot write a secret. Operators manage them via
    /// `apollia-os tools secret set ...` or the desktop UI.
    /// Empty by default: agents without configured secrets.
    #[serde(default)]
    pub secrets: Vec<String>,

    /// Shell commands to run as a verification pass after a completed run.
    ///
    /// Each entry is a full shell command string (e.g. `"cargo test -p my-agent"`).
    /// Commands are executed in order by the verification loop via an injected
    /// invoker. Empty by default: no verification commands declared. A
    /// project-config fallback is applied by the loop when this field is empty.
    #[serde(default)]
    pub check_commands: Vec<String>,
}

/// Declarative skill of an agent.
///
/// Used to build the A2A card when `supports_a2a = true`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkill {
    /// Unique identifier of the skill.
    pub id: String,
    /// Human-readable name of the skill. If absent in the source manifest,
    /// the runtime may derive it from `id`.
    #[serde(default)]
    pub name: String,
    /// Description of what the skill does.
    #[serde(default)]
    pub description: String,
    /// Supported input modes (e.g. ["text", "data"]). Empty by default to stay
    /// backward compatible with legacy TOML manifests; the `@skill` decorator
    /// on the Python SDK side publishes `["data"]`.
    #[serde(default)]
    pub input_modes: Vec<String>,
    /// Supported output modes (e.g. ["text", "file"]). Empty by default.
    #[serde(default)]
    pub output_modes: Vec<String>,
    /// Examples of valid payloads, propagated to the LLM-facing tool descriptor
    /// to help mid-market and smaller models build a correct call on the first
    /// try. Stored as `Vec<Value>` (each entry is a JSON object). Empty by
    /// default.
    #[serde(default)]
    pub examples: Vec<serde_json::Value>,
    /// Schema of the fields expected in the A2A payload.
    ///
    /// Apollia format (custom, simpler to write on the Python side than full
    /// JSON Schema): `{ "<field>": { "type": "...", "description": "...",
    /// "required": true|false } }`. Converted on the runtime side to canonical
    /// JSON Schema when exposing the skill to the LLM (see
    /// `crates/apollia-runtime/src/chat/a2a_tools.rs`). `None` = no published
    /// contract (the caller must guess, which is an anti-pattern).
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_manifest_serialization() {
        // GIVEN
        let manifest = AgentManifest {
            format_version: default_format_version(),
            name: "devis-agent".into(),
            version: "1.0.0".into(),
            description: "Génère des devis".into(),
            tools_required: vec!["file_io".into(), "bash_executor".into()],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".to_string(),
            supports_mailbox: false,
            mailbox_allowlist: None,
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            user_memory_write: false,
            datasources: vec![],
            templates: vec![],
            secrets: vec![],
            check_commands: vec![],
        };
        // WHEN
        let json = serde_json::to_string(&manifest).expect("serialization failed");
        // THEN
        assert!(json.contains("devis-agent"));
        assert!(json.contains("file_io"));
    }

    #[test]
    fn test_manifest_optional_defaults() {
        // GIVEN / WHEN
        let manifest = AgentManifest {
            format_version: default_format_version(),
            name: "agent".into(),
            version: "1.0.0".into(),
            description: "desc".into(),
            tools_required: vec!["file_io".into()],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".to_string(),
            supports_mailbox: false,
            mailbox_allowlist: None,
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            user_memory_write: false,
            datasources: vec![],
            templates: vec![],
            secrets: vec![],
            check_commands: vec![],
        };
        // THEN
        assert_eq!(manifest.max_concurrent_tasks, 1);
        assert!(!manifest.supports_streaming);
        assert!(manifest.memory_namespace.is_none());
    }

    #[test]
    fn test_tools_requiring_approval_default_empty() {
        // GIVEN a manifest JSON without the tools_requiring_approval field
        let json = r#"{"name":"test","version":"0.1.0","description":"desc","tools_required":[]}"#;
        // WHEN deserialized
        let manifest: AgentManifest = serde_json::from_str(json).expect("deserialize must succeed");
        // THEN tools_requiring_approval is empty
        assert!(manifest.tools_requiring_approval.is_empty());
    }

    #[test]
    fn test_tools_requiring_approval_parsed() {
        // GIVEN a manifest JSON with tools_requiring_approval
        let json = r#"{
            "name":"test","version":"0.1.0","description":"desc","tools_required":[],
            "tools_requiring_approval":["smtp","http_client"]
        }"#;
        // WHEN deserialized
        let manifest: AgentManifest = serde_json::from_str(json).expect("deserialize must succeed");
        // THEN the list is parsed correctly
        assert_eq!(
            manifest.tools_requiring_approval,
            vec!["smtp", "http_client"]
        );
    }

    // GIVEN a manifest JSON with "llm_backend": "local-code"
    // WHEN deserialized
    // THEN llm_backend == Some("local-code")
    #[test]
    fn test_llm_backend_present_extracts_some() {
        let json = serde_json::json!({
            "name": "test-agent",
            "version": "1.0.0",
            "description": "test",
            "tools_required": [],
            "llm_backend": "local-code"
        });
        let manifest: AgentManifest = serde_json::from_value(json).unwrap();
        assert_eq!(manifest.llm_backend, Some("local-code".to_string()));
    }

    // GIVEN a manifest JSON without the "llm_backend" key
    // WHEN deserialized
    // THEN llm_backend == None
    #[test]
    fn test_llm_backend_absent_is_none() {
        let json = serde_json::json!({
            "name": "test-agent",
            "version": "1.0.0",
            "description": "test",
            "tools_required": []
        });
        let manifest: AgentManifest = serde_json::from_value(json).unwrap();
        assert!(manifest.llm_backend.is_none());
    }

    // GIVEN a manifest JSON with "llm_backend": null
    // WHEN deserialized
    // THEN llm_backend == None
    #[test]
    fn test_llm_backend_null_is_none() {
        let json = serde_json::json!({
            "name": "test-agent",
            "version": "1.0.0",
            "description": "test",
            "tools_required": [],
            "llm_backend": null
        });
        let manifest: AgentManifest = serde_json::from_value(json).unwrap();
        assert!(manifest.llm_backend.is_none());
    }

    // GIVEN a manifest with llm_backend = Some(...)
    // WHEN serialized to JSON
    // THEN the field appears in the JSON
    #[test]
    fn test_llm_backend_serialized_when_some() {
        let json = serde_json::json!({
            "name": "test-agent",
            "version": "1.0.0",
            "description": "test",
            "tools_required": [],
            "llm_backend": "mistral-small"
        });
        let manifest: AgentManifest = serde_json::from_value(json).unwrap();
        let output = serde_json::to_string(&manifest).unwrap();
        assert!(output.contains("mistral-small"));
    }

    // GIVEN a manifest with llm_backend = None
    // WHEN serialized to JSON
    // THEN the field does not appear (skip_serializing_if)
    #[test]
    fn test_llm_backend_absent_from_json_when_none() {
        let json = serde_json::json!({
            "name": "test-agent",
            "version": "1.0.0",
            "description": "test",
            "tools_required": []
        });
        let manifest: AgentManifest = serde_json::from_value(json).unwrap();
        let output = serde_json::to_string(&manifest).unwrap();
        assert!(!output.contains("llm_backend"));
    }

    #[test]
    fn test_tools_requiring_approval_roundtrip() {
        // GIVEN an AgentManifest with tools_requiring_approval
        let manifest = AgentManifest {
            format_version: default_format_version(),
            name: "roundtrip-agent".into(),
            version: "1.0.0".into(),
            description: "test".into(),
            tools_required: vec![],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: None,
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
            execution_mode: "auto".into(),
            supports_mailbox: false,
            mailbox_allowlist: None,
            system_prompt: None,
            tools_requiring_approval: vec!["smtp".into(), "bash_executor".into()],
            llm_backend: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
            agent_class: None,
            user_memory_write: false,
            datasources: vec![],
            templates: vec![],
            secrets: vec![],
            check_commands: vec![],
        };
        // WHEN serde roundtrip JSON
        let json = serde_json::to_string(&manifest).expect("serialize must succeed");
        let restored: AgentManifest =
            serde_json::from_str(&json).expect("deserialize must succeed");
        // THEN no loss
        assert_eq!(
            restored.tools_requiring_approval,
            vec!["smtp", "bash_executor"]
        );
    }

    #[test]
    fn test_packages_field_default() {
        // GIVEN a manifest JSON without a packages field
        let json = r#"{"name":"test","version":"1.0.0","description":"","tools_required":[]}"#;
        // WHEN deserialized
        let manifest: AgentManifest = serde_json::from_str(json).expect("deserialize must succeed");
        // THEN packages = vec![]
        assert!(manifest.packages.is_empty());
    }

    #[test]
    fn test_packages_field_populated() {
        // GIVEN a manifest JSON with packages
        let json = r#"{"name":"excel","version":"0.1.0","description":"","tools_required":[],"packages":["openpyxl>=3.1.0"]}"#;
        // WHEN deserialized
        let manifest: AgentManifest = serde_json::from_str(json).expect("deserialize must succeed");
        // THEN packages contains the value
        assert_eq!(manifest.packages, vec!["openpyxl>=3.1.0"]);
    }

    #[test]
    fn test_packages_field_roundtrip() {
        // GIVEN a manifest JSON with packages
        let json = r#"{"name":"pandas-agent","version":"0.1.0","description":"","tools_required":[],"packages":["pandas>=2.0.0"]}"#;
        // WHEN serialized then deserialized
        let manifest: AgentManifest = serde_json::from_str(json).expect("deserialize must succeed");
        let serialized = serde_json::to_string(&manifest).expect("serialize must succeed");
        let restored: AgentManifest =
            serde_json::from_str(&serialized).expect("deserialize roundtrip must succeed");
        // THEN packages is preserved
        assert_eq!(restored.packages, vec!["pandas>=2.0.0"]);
    }

    #[test]
    fn test_memory_config_default() {
        // GIVEN
        let config = MemoryConfig::default();
        // THEN auto_purge is false and all retention fields are None
        assert!(!config.auto_purge);
        assert!(config.episodic_retention_days.is_none());
        assert!(config.semantic_retention_days.is_none());
        assert!(config.procedural_retention_days.is_none());
    }

    #[test]
    fn test_memory_config_absent_from_manifest_is_none() {
        // GIVEN a manifest JSON without memory_config
        let json = r#"{"name":"agent","version":"1.0.0","description":"","tools_required":[]}"#;
        // WHEN deserialized
        let manifest: AgentManifest = serde_json::from_str(json).expect("deserialize");
        // THEN memory_config is None
        assert!(manifest.memory_config.is_none());
    }

    #[test]
    fn test_memory_config_roundtrip() {
        // GIVEN a manifest JSON with memory_config
        let json = serde_json::json!({
            "name": "long-running",
            "version": "1.0.0",
            "description": "agent with retention config",
            "tools_required": [],
            "memory_config": {
                "episodic_retention_days": 7,
                "semantic_retention_days": null,
                "procedural_retention_days": null,
                "auto_purge": true
            }
        });
        // WHEN deserialized and re-serialized
        let manifest: AgentManifest = serde_json::from_value(json).expect("deserialize");
        let cfg = manifest
            .memory_config
            .as_ref()
            .expect("memory_config present");
        assert_eq!(cfg.episodic_retention_days, Some(7));
        assert!(cfg.semantic_retention_days.is_none());
        assert!(cfg.procedural_retention_days.is_none());
        assert!(cfg.auto_purge);
        // AND roundtrip preserves values
        let serialized = serde_json::to_string(&manifest).expect("serialize");
        let restored: AgentManifest = serde_json::from_str(&serialized).expect("roundtrip");
        let cfg2 = restored
            .memory_config
            .as_ref()
            .expect("memory_config preserved");
        assert_eq!(cfg2.episodic_retention_days, Some(7));
        assert!(cfg2.auto_purge);
    }

    #[test]
    fn test_check_commands_defaults_empty_when_absent() {
        // GIVEN a manifest JSON without a check_commands field
        let json = serde_json::json!({
            "name": "agent",
            "version": "1.0.0",
            "description": "desc",
            "tools_required": ["file_io"]
        });
        // WHEN deserialized
        let manifest: AgentManifest = serde_json::from_value(json).expect("deserialize");
        // THEN check_commands defaults to an empty vector (retro-compatible)
        assert!(manifest.check_commands.is_empty());
    }

    #[test]
    fn test_check_commands_roundtrip_preserves_values() {
        // GIVEN a manifest JSON declaring two check commands
        let json = serde_json::json!({
            "name": "agent",
            "version": "1.0.0",
            "description": "desc",
            "tools_required": ["bash_executor"],
            "check_commands": ["cargo test -p my-agent", "cargo clippy"]
        });
        // WHEN deserialized and re-serialized
        let manifest: AgentManifest = serde_json::from_value(json).expect("deserialize");
        let serialized = serde_json::to_string(&manifest).expect("serialize");
        let restored: AgentManifest = serde_json::from_str(&serialized).expect("roundtrip");
        // THEN the declared commands survive the roundtrip in order
        assert_eq!(
            restored.check_commands,
            vec![
                "cargo test -p my-agent".to_string(),
                "cargo clippy".to_string()
            ]
        );
    }
}
