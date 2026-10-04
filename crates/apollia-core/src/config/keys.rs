//! The registry of the keys of `apollia.toml`.
//!
//! One entry per key: its dotted path, its type, its default, its
//! documentation, its reload class, whether the organization policy can fix
//! it, and whether it is withdrawn. The registry is written by hand and held
//! to the structures by a test: the leaf paths of a serialized
//! [`ApolliaConfig`](super::ApolliaConfig) must be exactly the keys of this
//! table that are not withdrawn. A story that adds a key to a section adds its
//! line here, or the test fails naming it.
//!
//! A withdrawn key is one the product no longer reads. A file that still
//! carries it is accepted: [`validate`](super::validate) sets it aside with a
//! `config.key.retired` warning, so an installation written for an earlier
//! version keeps starting.

/// The TOML type of a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    /// `true` or `false`.
    Boolean,
    /// A whole number.
    Integer,
    /// A decimal number.
    Float,
    /// A string.
    String,
    /// A string naming a file or a directory; `~` stands for the home folder.
    Path,
    /// One of a closed list of strings.
    Enum(&'static [&'static str]),
    /// An array of strings.
    StringArray,
    /// An array of tables (`[[...]]`).
    TableArray,
    /// A table whose own keys are free (a map) or optional as a whole.
    Table,
}

/// When a new value of a key takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadClass {
    /// Applied by the running engine, without a restart.
    Reloadable,
    /// Fixes a resource opened at startup: applied at the next start.
    Restart,
}

/// One key of `apollia.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigKey {
    /// Dotted path from the root of the file.
    pub path: &'static str,
    /// TOML type.
    pub ty: KeyType,
    /// Default, as TOML would spell it, or `None` when the key has none (an
    /// optional key, or one the section requires).
    pub default: Option<&'static str>,
    /// What the key does, in English.
    pub doc: &'static str,
    /// When a new value takes effect.
    pub reload: ReloadClass,
    /// Whether the organization policy can fix the value.
    pub lockable: bool,
    /// For a withdrawn key, what the operator does instead; `None` for a live
    /// key.
    pub withdrawn: Option<&'static str>,
}

const fn key(
    path: &'static str,
    ty: KeyType,
    default: Option<&'static str>,
    reload: ReloadClass,
    doc: &'static str,
) -> ConfigKey {
    ConfigKey {
        path,
        ty,
        default,
        doc,
        reload,
        lockable: false,
        withdrawn: None,
    }
}

const fn locked(mut k: ConfigKey) -> ConfigKey {
    k.lockable = true;
    k
}

const fn withdrawn(path: &'static str, detail: &'static str) -> ConfigKey {
    ConfigKey {
        path,
        ty: KeyType::Table,
        default: None,
        doc: "Withdrawn: the product no longer reads it.",
        reload: ReloadClass::Reloadable,
        lockable: false,
        withdrawn: Some(detail),
    }
}

use KeyType::{Boolean, Enum, Float, Integer, Path, StringArray, Table, TableArray};
use ReloadClass::{Reloadable, Restart};

/// Every key of `apollia.toml`, live and withdrawn.
pub const KEYS: &[ConfigKey] = &[
    // [llm]
    key("llm.format_version", Integer, Some("1"), Restart, "Version of the [llm] format; read as 1 when absent."),
    key("llm.default", KeyType::String, None, Restart, "Name of the backend used when none is asked for; must name an entry of llm.backends."),
    key("llm.backends", TableArray, None, Restart, "OpenAI-compatible backends, migrated into the engine's database on a first start; required with llm.default."),
    key("llm.observability.log_token_usage", Boolean, Some("true"), Restart, "Log the tokens consumed by each model call."),
    key("llm.observability.log_latency", Boolean, Some("true"), Restart, "Log the latency of each model call."),
    key("llm.observability.log_cost", Boolean, Some("false"), Restart, "Log the estimated cost of each cloud model call."),
    key("llm.observability.debug_log_prompt", Boolean, Some("false"), Restart, "Log the full prompt at trace level."),
    key("llm.routing", Table, None, Restart, "Backends answering precise and fast requests, and the hybrid routing ceiling."),
    key("llm.pricing_overrides", Table, Some("{}"), Restart, "Prices per million tokens, keyed by model identifier."),
    key("llm.cost_alert_threshold_usd", Float, None, Restart, "Cost in USD above which a token budget update is flagged."),
    key("llm.runner.backend", KeyType::String, Some("\"auto\""), Restart, "Speech-to-text runner binary: auto, cpu, cuda, metal or rocm."),
    locked(key("llm.guardrails.level", Enum(&["disabled", "lenient", "balanced", "strict", "ceiling"]), Some("\"balanced\""), Reloadable, "How strictly a local model launch is held to the machine's memory; an unknown level holds to balanced.")),
    locked(key("llm.guardrails.ceiling_gb", Float, None, Reloadable, "Largest estimate, in gigabytes, the ceiling level launches.")),
    locked(key("llm.residency.idle_unload_minutes", Integer, Some("60"), Reloadable, "Minutes without a request after which a local model nothing holds is unloaded; 0 never unloads.")),
    withdrawn("llm.vertex", "the Google Vertex AI client was removed; declare an OpenAI-compatible server instead"),
    // [api]
    key("api.bind", KeyType::String, Some("\"127.0.0.1\""), Restart, "Address the daemon's TCP listener binds to."),
    key("api.port", Integer, Some("7771"), Restart, "TCP port of the native API."),
    key("api.require_token", Boolean, Some("true"), Restart, "Require a bearer token on every TCP request to the daemon."),
    key("api.unix_socket", Path, Some("\"~/.apollia/runtime.sock\""), Restart, "Local socket of the embedded runtime; its folder must exist."),
    key("api.tls_cert", Path, None, Restart, "PEM certificate chain for TLS on the daemon's TCP listener; set with api.tls_key."),
    key("api.tls_key", Path, None, Restart, "PEM private key matching api.tls_cert."),
    // [runtime]
    key("runtime.eventbus_capacity", Integer, Some("1024"), Restart, "Events buffered by the event bus before a slow reader lags (64 to 65536)."),
    key("runtime.mailbox_capacity", Integer, Some("100"), Restart, "Messages buffered by an actor mailbox (10 to 10000)."),
    key("runtime.mailbox_visibility_timeout_secs", Integer, Some("60"), Restart, "Seconds a leased agent message stays invisible before redelivery."),
    key("runtime.mailbox_message_ttl_secs", Integer, Some("86400"), Restart, "Seconds an undelivered agent message is kept."),
    key("runtime.mailbox_send_quota_per_run", Integer, Some("50"), Restart, "Agent messages one run may send."),
    key("runtime.mailbox_max_payload_bytes", Integer, Some("65536"), Restart, "Largest agent message payload, in bytes."),
    key("runtime.mailbox_audit_full_payload", Boolean, Some("false"), Restart, "Copy the full agent message payload into the audit journal."),
    key("runtime.startup_timeout_secs", Integer, Some("300"), Restart, "Seconds the engine may take to start."),
    key("runtime.agent_idle_unload_secs", Integer, Some("600"), Restart, "Seconds without a task after which an agent module is unloaded."),
    // [hitl]
    key("hitl.timeout_hours", Integer, None, Reloadable, "Hours after which an unanswered approval cancels its task (1 to 168); none waits forever."),
    key("hitl.scan_interval_secs", Integer, Some("60"), Reloadable, "Seconds between two scans for expired approvals (10 to 3600)."),
    // [tools]
    key("tools.max_output_chars", Integer, Some("30000"), Reloadable, "Largest tool output passed to the model, in bytes (10 to 1000000)."),
    key("tools.file_path_extraction_pattern", KeyType::String, None, Reloadable, "Regular expression extracting file paths from a command output."),
    key("tools.disabled", StringArray, Some("[]"), Reloadable, "Native tools removed from every agent."),
    key("tools.web_search.backend", Enum(&["auto", "duck_duck_go", "brave"]), Some("\"auto\""), Reloadable, "Search engine of the web_search tool."),
    key("tools.web_search.require_configured", Boolean, Some("false"), Reloadable, "Refuse a search when the chosen engine is not configured."),
    key("tools.web_search.brave.api_key_env_var", KeyType::String, Some("\"BRAVE_SEARCH_API_KEY\""), Reloadable, "Environment variable holding the Brave Search key."),
    key("tools.web_search.brave.timeout_secs", Integer, Some("15"), Reloadable, "Seconds a Brave Search request may take (1 to 120)."),
    key("tools.web_search.brave.max_results", Integer, Some("10"), Reloadable, "Results asked of Brave Search (1 to 20)."),
    key("tools.web_search.duckduckgo.timeout_secs", Integer, Some("15"), Reloadable, "Seconds a DuckDuckGo request may take (1 to 120)."),
    key("tools.web_search.duckduckgo.max_response_kb", Integer, Some("1024"), Reloadable, "Largest DuckDuckGo response read, in KiB (16 to 16384)."),
    key("tools.web_read.timeout_secs", Integer, Some("20"), Reloadable, "Seconds a web_read request may take (1 to 120)."),
    key("tools.web_read.max_response_kb", Integer, Some("2048"), Reloadable, "Largest page web_read reads, in KiB (64 to 32768)."),
    key("tools.web_read.ssrf_guard", Boolean, Some("true"), Reloadable, "Refuse an address on the local network or the machine itself."),
    key("tools.python_interpreter", Path, None, Reloadable, "Python interpreter used instead of the bundled one."),
    // [mcp]
    locked(key("mcp.approval_ttl_hours", Integer, Some("24"), Reloadable, "Hours an MCP tool approval stays valid; 0 never expires (0 to 8760).")),
    locked(key("mcp.tool_loading", Enum(&["eager", "deferred"]), Some("\"deferred\""), Reloadable, "Load every MCP tool schema up front, or on first use.")),
    locked(key("mcp.tool_search_limit", Integer, Some("20"), Reloadable, "Results returned by the tool_search tool (1 to 500).")),
    locked(key("mcp.server_enabled", Boolean, Some("false"), Reloadable, "Let an MCP client reach the agents and workflows marked exposed, through apollia-os mcp server.")),
    // [hooks]
    key("hooks.handlers", TableArray, Some("[]"), Reloadable, "Lifecycle hook handlers, a command or an HTTP endpoint each."),
    // [chat]
    key("chat.plan_mode_default", Boolean, Some("false"), Reloadable, "Start every new conversation in plan mode."),
    key("chat.default_workspace", Path, None, Reloadable, "Working folder of a conversation outside a project."),
    key("chat.tool_turn_temperature", Float, None, Reloadable, "Model temperature of a turn that offers tools; none means 0.3."),
    // [filesystem]
    key("filesystem.journal.enabled", Boolean, Some("true"), Reloadable, "Keep a reversible journal of file changes."),
    key("filesystem.journal.max_sessions", Integer, Some("50"), Reloadable, "Journal sessions kept (1 to 10000)."),
    key("filesystem.journal.root", Path, Some("\"~/.apollia/journal\""), Reloadable, "Folder of the reversible journal."),
    key("filesystem.trusted_paths", StringArray, Some("[\"~\"]"), Reloadable, "Folders an agent may work in without asking."),
    // [models]
    locked(key("models.registry_url", KeyType::String, Some("\"https://registry.apollia.fr/models/stable/index.json\""), Reloadable, "Address of the signed index of model cards.")),
    locked(key("models.registry_mirror_url", KeyType::String, Some("\"https://github.com/Apollia-OS/apollia-os/releases/latest/download/models-stable-index.json\""), Reloadable, "Address tried when the index does not answer; empty disables it.")),
    locked(key("models.registry_enabled", Boolean, Some("true"), Reloadable, "Read the online index of model cards.")),
    locked(key("models.dir", Path, Some("\"\""), Reloadable, "Folder the language models, their projectors and the embedding models are kept in; empty means <data dir>/models. Changing it moves no file.")),
    locked(key("models.import.require_verified", Boolean, Some("false"), Reloadable, "Refuse an imported model file that matches no signed card.")),
    // [workflows]
    key("workflows.max_steps", Integer, Some("200"), Restart, "Steps a workflow run may take."),
    key("workflows.max_tool_calls", Integer, Some("400"), Restart, "Tool calls a workflow run may make."),
    key("workflows.wall_clock_secs", Integer, Some("3600"), Restart, "Seconds a workflow run may last."),
    // [desktop]
    locked(key("desktop.developer_mode", Boolean, Some("false"), Reloadable, "Show the advanced surfaces of the application.")),
    locked(key("desktop.update_channel", Enum(&["stable", "preview"]), Some("\"stable\""), Reloadable, "Update channel the application reads its update manifest from.")),
    locked(key("desktop.background_service_allowed", Boolean, Some("true"), Reloadable, "Allow the application to install the engine as a service of the user's session.")),
    // [server]
    locked(key("server.enabled", Boolean, Some("false"), Restart, "Start the compatible server (OpenAI and Anthropic formats) with the engine.")),
    locked(key("server.port", Integer, Some("7772"), Restart, "TCP port of the compatible server, on the loopback interface.")),
    locked(key("server.require_auth", Boolean, Some("false"), Restart, "Require a token of the compatible server on every request; read when the listener opens.")),
    // [engines]
    locked(key("engines.log_level", Enum(&["error", "warning", "info", "debug"]), Some("\"debug\""), Reloadable, "Log level of the local engine, applied when a model is next loaded.")),
    // [author]
    locked(key("author.name", KeyType::String, Some("\"\""), Reloadable, "Publisher name of the agents exported from this machine; empty asks at export.")),
    // [observability]
    key("observability.max_input_bytes", Integer, Some("32768"), Reloadable, "Largest run input kept, in bytes."),
    key("observability.max_output_bytes", Integer, Some("32768"), Reloadable, "Largest run output kept, in bytes."),
    key("observability.max_tool_output_bytes", Integer, Some("10240"), Reloadable, "Largest tool output kept, in bytes."),
    key("observability.capture_thoughts", Boolean, Some("true"), Reloadable, "Keep the model's reasoning in the run history."),
    key("observability.capture_tool_args", Boolean, Some("true"), Reloadable, "Keep tool arguments in the run history."),
    key("observability.capture_tool_outputs", Boolean, Some("true"), Reloadable, "Keep tool outputs in the run history."),
    key("observability.capture_agent_logs", Boolean, Some("true"), Reloadable, "Keep agent logs in the run history."),
    key("observability.retention_days", Integer, Some("90"), Reloadable, "Days the run history is kept."),
    // [ui]
    key("ui.link_preview_enabled", Boolean, Some("false"), Reloadable, "Fetch the preview of a link shown in a conversation."),
    // Withdrawn sections.
    withdrawn("memory", "nothing reads it; remove the section"),
    withdrawn("budget", "nothing reads it; remove the section"),
    withdrawn("a2a", "nothing reads it; remove the section"),
    withdrawn("oria", "nothing reads it; remove the section"),
    withdrawn("registry", "nothing reads it; remove the section"),
    withdrawn("permissions", "nothing reads it; remove the section"),
    withdrawn("triggers", "use the desktop app or the API to manage it"),
    withdrawn("notifications", "use the desktop app or the API to manage it"),
    withdrawn("stt", "use the desktop app or the API to manage it"),
];

/// The entry of `path`, live or withdrawn.
pub fn lookup(path: &str) -> Option<&'static ConfigKey> {
    KEYS.iter().find(|k| k.path == path)
}

/// The withdrawn entry that covers `path` (the key itself or a table above
/// it), if any.
pub fn withdrawn_covering(path: &str) -> Option<&'static ConfigKey> {
    KEYS.iter()
        .filter(|k| k.withdrawn.is_some())
        .find(|k| super::scan::covers(k.path, path))
}

/// `true` when `path` names a live key, or lies under a live key whose own
/// keys are free (a map such as `llm.pricing_overrides`, or an optional table
/// such as `llm.routing`).
pub fn is_live_path(path: &str) -> bool {
    KEYS.iter()
        .filter(|k| k.withdrawn.is_none())
        .any(|k| k.path == path || (k.ty == KeyType::Table && super::scan::covers(k.path, path)))
}

/// Every withdrawn entry, in registry order.
pub fn withdrawn_keys() -> impl Iterator<Item = &'static ConfigKey> {
    KEYS.iter().filter(|k| k.withdrawn.is_some())
}
