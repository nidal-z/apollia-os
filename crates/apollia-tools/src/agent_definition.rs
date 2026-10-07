//! An agent as its package defines it, the user's overrides, and the agent
//! that results from the two.
//!
//! The definition is the `[[agents]]` entry of an `apollia.toml`, kept as JSON
//! (`id`, `name`, `mission`, `examples`, `defaults.access`, `defaults.sources`,
//! `defaults.model`, `defaults.autonomy`). It is never written by the user: an update replaces it
//! whole. What the user changes is an override, one per field, stored apart;
//! the resolved agent is the definition, then each override in place of the
//! field it names.
//!
//! The model of an agent is not an override: it lives in `agent_model` of
//! `system.db` and is passed to [`resolve`] as read there.
//!
//! The autonomy tier is an override like the mission: the definition declares
//! it, the user may replace it, and when neither says anything the global
//! default passed to [`resolve`] applies.
//!
//! So is the planning mode (`defaults.planning`): whether the agent plans a
//! task before acting on it. When neither says anything it is
//! [`PlanningMode::Auto`].
//!
//! The profile an agent runs with is an override and nothing else: a
//! definition never names one, the user designates it. An empty designation,
//! or one naming a profile deleted since, reads as the default profile.

use apollia_core::model_preset::AgentModel;
use apollia_core::{AgentManifest, AutonomyLevel};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// Identifier of the Apollia agent, the built-in chat.
pub const APOLLIA_AGENT_ID: &str = apollia_permissions::APOLLIA_AGENT_ID;

/// Version of the definition of the Apollia agent.
pub const APOLLIA_DEFINITION_VERSION: &str = "1";

/// The definition of the Apollia agent: its name, and nothing the user has
/// to undo. No mission leaves the built-in prompt of its tier in force.
pub fn apollia_definition() -> Value {
    json!({
        "id": APOLLIA_AGENT_ID,
        "name": "Apollia",
        "mission": "",
        "defaults": {},
    })
}

/// Whether `name` is an agent of the product itself, which the user changes
/// but never uninstalls.
pub fn is_built_in_agent(name: &str) -> bool {
    name == APOLLIA_AGENT_ID
}

/// Whether an agent plans a task before acting on it.
///
/// A plan is a list of steps written first, run level by level, checked, and
/// kept in the plan cache for the next identical task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanningMode {
    /// Never: the agent acts step by step, choosing each tool as it goes.
    Never,
    /// When the task reads as a multi-step one.
    #[default]
    Auto,
    /// Always.
    Always,
}

impl PlanningMode {
    /// Every mode, in display order.
    pub const ALL: [PlanningMode; 3] = [
        PlanningMode::Never,
        PlanningMode::Auto,
        PlanningMode::Always,
    ];

    /// Stored and wire form.
    pub fn as_str(self) -> &'static str {
        match self {
            PlanningMode::Never => "never",
            PlanningMode::Auto => "auto",
            PlanningMode::Always => "always",
        }
    }

    /// Reads the stored form; `None` for any other value.
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.as_str() == raw)
    }
}

/// Where an agent comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentOrigin {
    /// Installed from a package or an agent file.
    Package,
    /// Created in the interface, without code.
    Interface,
}

impl AgentOrigin {
    /// Stored form.
    pub fn as_str(self) -> &'static str {
        match self {
            AgentOrigin::Package => "package",
            AgentOrigin::Interface => "interface",
        }
    }

    /// Reads the stored form; an unknown value reads as [`AgentOrigin::Package`],
    /// the only origin a row could carry before the column existed.
    pub fn parse(raw: &str) -> Self {
        match raw {
            "interface" => AgentOrigin::Interface,
            _ => AgentOrigin::Package,
        }
    }
}

/// A field of the definition the user can override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverrideField {
    /// Display name.
    Name,
    /// What the agent is for, used as its instructions.
    Mission,
    /// Example requests.
    Examples,
    /// Tools granted to the agent.
    Access,
    /// Sources checked for the agent.
    Sources,
    /// Autonomy tier: budget, verification, critic.
    Autonomy,
    /// Whether the agent plans a task before acting on it.
    Planning,
    /// Identifier of the user profile the agent runs with.
    Profile,
}

/// Every overridable field, in display order.
pub const OVERRIDE_FIELDS: [OverrideField; 8] = [
    OverrideField::Name,
    OverrideField::Mission,
    OverrideField::Examples,
    OverrideField::Access,
    OverrideField::Sources,
    OverrideField::Autonomy,
    OverrideField::Planning,
    OverrideField::Profile,
];

/// A value that does not fit the field it is written to.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum OverrideValueError {
    /// The field name is not one of [`OVERRIDE_FIELDS`].
    #[error(
        "unknown field '{0}', expected one of name, mission, examples, access, sources, autonomy, planning, profile"
    )]
    UnknownField(String),
    /// The field takes a string.
    #[error("field '{0}' takes a string")]
    ExpectedString(&'static str),
    /// The field takes a list of strings.
    #[error("field '{0}' takes a list of strings")]
    ExpectedList(&'static str),
    /// The autonomy field takes one of the four tiers.
    #[error(
        "field 'autonomy' takes one of assisted, supervised, bounded_autonomous, long_autonomous"
    )]
    ExpectedAutonomyLevel,
    /// The planning field takes one of the three modes.
    #[error("field 'planning' takes one of never, auto, always")]
    ExpectedPlanningMode,
}

impl OverrideField {
    /// Stored and wire form.
    pub fn as_str(self) -> &'static str {
        match self {
            OverrideField::Name => "name",
            OverrideField::Mission => "mission",
            OverrideField::Examples => "examples",
            OverrideField::Access => "access",
            OverrideField::Sources => "sources",
            OverrideField::Autonomy => "autonomy",
            OverrideField::Planning => "planning",
            OverrideField::Profile => "profile",
        }
    }

    /// Reads a field name.
    ///
    /// # Errors
    ///
    /// [`OverrideValueError::UnknownField`] for any other name.
    pub fn parse(raw: &str) -> Result<Self, OverrideValueError> {
        OVERRIDE_FIELDS
            .into_iter()
            .find(|field| field.as_str() == raw)
            .ok_or_else(|| OverrideValueError::UnknownField(raw.to_owned()))
    }

    /// Whether the field holds a list of strings rather than one string.
    pub fn is_list(self) -> bool {
        matches!(
            self,
            OverrideField::Examples | OverrideField::Access | OverrideField::Sources
        )
    }

    /// JSON pointer of the field in a definition.
    pub(crate) fn pointer(self) -> &'static str {
        match self {
            OverrideField::Name => "/name",
            OverrideField::Mission => "/mission",
            OverrideField::Examples => "/examples",
            OverrideField::Access => "/defaults/access",
            OverrideField::Sources => "/defaults/sources",
            OverrideField::Autonomy => "/defaults/autonomy",
            OverrideField::Planning => "/defaults/planning",
            OverrideField::Profile => "/defaults/profile",
        }
    }

    /// Checks that `value` has the shape of the field.
    ///
    /// # Errors
    ///
    /// [`OverrideValueError::ExpectedString`],
    /// [`OverrideValueError::ExpectedList`],
    /// [`OverrideValueError::ExpectedAutonomyLevel`] or
    /// [`OverrideValueError::ExpectedPlanningMode`].
    pub fn check(self, value: &Value) -> Result<(), OverrideValueError> {
        if self == OverrideField::Autonomy {
            if autonomy_of(value).is_none() {
                return Err(OverrideValueError::ExpectedAutonomyLevel);
            }
        } else if self == OverrideField::Planning {
            if planning_of(value).is_none() {
                return Err(OverrideValueError::ExpectedPlanningMode);
            }
        } else if self.is_list() {
            let is_list_of_strings = value
                .as_array()
                .is_some_and(|items| items.iter().all(Value::is_string));
            if !is_list_of_strings {
                return Err(OverrideValueError::ExpectedList(self.as_str()));
            }
        } else if !value.is_string() {
            return Err(OverrideValueError::ExpectedString(self.as_str()));
        }
        Ok(())
    }
}

/// The tier a value names, if it names one.
fn autonomy_of(value: &Value) -> Option<AutonomyLevel> {
    value.as_str().and_then(|raw| raw.parse().ok())
}

/// The planning mode a value names, if it names one.
fn planning_of(value: &Value) -> Option<PlanningMode> {
    value.as_str().and_then(PlanningMode::parse)
}

/// An agent as stored: its package definition and where it comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredAgent {
    /// Identifier, unique on the machine.
    pub name: String,
    /// Where the agent comes from.
    pub origin: AgentOrigin,
    /// Package the agent came with, when it came with one.
    pub package: Option<String>,
    /// Version of the definition.
    pub version: String,
    /// The definition, as its package declares it.
    pub definition: Value,
    /// Whether the agent is active.
    pub enabled: bool,
    /// Installation timestamp.
    pub installed_at: String,
    /// Last-update timestamp.
    pub updated_at: String,
}

/// One override: the user's value for one field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentOverride {
    /// Overridden field.
    pub field: OverrideField,
    /// The user's value, of the field's shape.
    pub value: Value,
}

/// The agent in force: the definition, then the overrides, field by field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedAgent {
    /// Identifier.
    pub id: String,
    /// Where the agent comes from.
    pub origin: AgentOrigin,
    /// Package the agent came with.
    pub package: Option<String>,
    /// Version of the definition.
    pub version: String,
    /// Whether the agent is active.
    pub enabled: bool,
    /// Display name.
    pub name: String,
    /// What the agent is for.
    pub mission: String,
    /// Example requests.
    pub examples: Vec<String>,
    /// Tools granted.
    pub access: Vec<String>,
    /// Sources checked.
    pub sources: Vec<String>,
    /// The model assigned to the agent in `system.db`, if any.
    pub model: Option<AgentModel>,
    /// Autonomy tier: the user's override, else the definition's, else the
    /// global default.
    pub autonomy: AutonomyLevel,
    /// Whether the agent plans first: the user's override, else the
    /// definition's, else [`PlanningMode::Auto`].
    pub planning: PlanningMode,
    /// Identifier of the profile the user designated for the agent; `None`
    /// runs it with the default profile.
    pub profile: Option<String>,
    /// Fields whose value comes from an override rather than the definition.
    pub overridden: Vec<OverrideField>,
    /// The definition the overrides apply to, unchanged.
    pub definition: Value,
}

/// Resolves `stored` with its `overrides`, the `model` read from
/// `agent_model`, and the global `default_autonomy`.
///
/// A field without an override takes the definition's value; a field the
/// definition lacks reads as empty, an autonomy tier neither names reads
/// as `default_autonomy`, and a planning mode neither names reads as
/// [`PlanningMode::Auto`].
pub fn resolve(
    stored: &StoredAgent,
    overrides: &[AgentOverride],
    model: Option<AgentModel>,
    default_autonomy: AutonomyLevel,
) -> ResolvedAgent {
    let value_of = |field: OverrideField| -> Option<&Value> {
        overrides
            .iter()
            .find(|o| o.field == field)
            .map(|o| &o.value)
            .or_else(|| stored.definition.pointer(field.pointer()))
    };
    let text = |field: OverrideField| -> String {
        value_of(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let list = |field: OverrideField| -> Vec<String> {
        value_of(field)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut overridden: Vec<OverrideField> = overrides.iter().map(|o| o.field).collect();
    overridden.sort();
    overridden.dedup();
    let name = text(OverrideField::Name);
    ResolvedAgent {
        id: stored.name.clone(),
        origin: stored.origin,
        package: stored.package.clone(),
        version: stored.version.clone(),
        enabled: stored.enabled,
        name: if name.is_empty() {
            stored.name.clone()
        } else {
            name
        },
        mission: text(OverrideField::Mission),
        examples: list(OverrideField::Examples),
        access: list(OverrideField::Access),
        sources: list(OverrideField::Sources),
        model,
        autonomy: value_of(OverrideField::Autonomy)
            .and_then(autonomy_of)
            .unwrap_or(default_autonomy),
        planning: value_of(OverrideField::Planning)
            .and_then(planning_of)
            .unwrap_or_default(),
        profile: overrides
            .iter()
            .find(|o| o.field == OverrideField::Profile)
            .and_then(|o| o.value.as_str())
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_owned),
        overridden,
        definition: stored.definition.clone(),
    }
}

/// What the registry holds for the stored agent `agent`: its identifier, its
/// version, its mission as description, its examples and the autonomy tier
/// its definition declares. Everything else is the default: what the agent
/// may reach is its resolved access, read when a run starts, never this.
pub fn manifest_of(agent: &StoredAgent) -> AgentManifest {
    let text = |pointer: &str| {
        agent
            .definition
            .pointer(pointer)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let examples = agent
        .definition
        .pointer(OverrideField::Examples.pointer())
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    AgentManifest {
        format_version: 1,
        name: agent.name.clone(),
        version: agent.version.clone(),
        description: text(OverrideField::Mission.pointer()),
        supports_streaming: false,
        supports_a2a: false,
        supports_mailbox: false,
        mailbox_allowlist: None,
        memory_namespace: None,
        shared_memory_namespaces: Vec::new(),
        max_concurrent_tasks: 1,
        step_budget: None,
        tags: Vec::new(),
        skills: Vec::new(),
        execution_mode: "auto".to_owned(),
        system_prompt: None,
        model: None,
        autonomy_level: agent
            .definition
            .pointer(OverrideField::Autonomy.pointer())
            .and_then(autonomy_of),
        packages: Vec::new(),
        memory_config: None,
        agent_type: None,
        examples,
        limitations: Vec::new(),
        setup_notes: None,
        agent_class: None,
        user_memory_write: false,
        datasources: Vec::new(),
        templates: Vec::new(),
        secrets: Vec::new(),
        check_commands: Vec::new(),
    }
}

/// The definition of an agent stored before definitions existed, read from
/// the manifest JSON its row carried.
///
/// This is the shape the manifest had when the `agents` table was created,
/// read key by key and frozen here: the mission is the system prompt, or the
/// description when there is none; the access is the required then the
/// optional tools; the autonomy is the declared tier. A JSON that does not
/// parse leaves the identifier alone, so a damaged row still migrates.
pub(crate) fn definition_from_legacy_manifest_json(name: &str, manifest_json: &str) -> Value {
    let manifest: Value = match serde_json::from_str(manifest_json) {
        Ok(manifest) => manifest,
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                agent = %name,
                error = %e,
                "agents_db.definition.manifest_unreadable"
            );
            return json!({ "id": name, "name": name, "mission": "", "defaults": {} });
        }
    };
    let text = |key: &str| manifest.get(key).and_then(Value::as_str);
    let mission = text("system_prompt")
        .filter(|prompt| !prompt.trim().is_empty())
        .or_else(|| text("description"))
        .unwrap_or_default();
    let strings = |key: &str| -> Vec<&str> {
        manifest
            .get(key)
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    };
    let mut access: Vec<&str> = Vec::new();
    for tool in strings("tools_required")
        .into_iter()
        .chain(strings("tools_optional"))
    {
        if !access.contains(&tool) {
            access.push(tool);
        }
    }
    let mut defaults = Map::new();
    if !access.is_empty() {
        defaults.insert("access".into(), json!(access));
    }
    if let Some(level) = manifest.get("autonomy_level").and_then(autonomy_of) {
        defaults.insert("autonomy".into(), json!(level.as_str()));
    }
    let agent_name = text("name").unwrap_or(name);
    let mut definition = Map::new();
    definition.insert("id".into(), json!(agent_name));
    definition.insert("name".into(), json!(agent_name));
    definition.insert("mission".into(), json!(mission));
    let examples = strings("examples");
    if !examples.is_empty() {
        definition.insert("examples".into(), json!(examples));
    }
    definition.insert("defaults".into(), Value::Object(defaults));
    Value::Object(definition)
}

#[cfg(test)]
mod tests;
