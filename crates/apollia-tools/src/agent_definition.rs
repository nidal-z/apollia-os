//! An agent as its package defines it, the user's overrides, and the agent
//! that results from the two.
//!
//! The definition is the `[[agents]]` entry of an `apollia.toml`, kept as JSON
//! (`id`, `name`, `mission`, `examples`, `defaults.access`, `defaults.sources`,
//! `defaults.model`). It is never written by the user: an update replaces it
//! whole. What the user changes is an override, one per field, stored apart;
//! the resolved agent is the definition, then each override in place of the
//! field it names.
//!
//! The model of an agent is not an override: it lives in `agent_model` of
//! `system.db` and is passed to [`resolve`] as read there.

use apollia_core::model_preset::AgentModel;
use apollia_core::AgentManifest;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

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
}

/// Every overridable field, in display order.
pub const OVERRIDE_FIELDS: [OverrideField; 5] = [
    OverrideField::Name,
    OverrideField::Mission,
    OverrideField::Examples,
    OverrideField::Access,
    OverrideField::Sources,
];

/// A value that does not fit the field it is written to.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum OverrideValueError {
    /// The field name is not one of [`OVERRIDE_FIELDS`].
    #[error("unknown field '{0}', expected one of name, mission, examples, access, sources")]
    UnknownField(String),
    /// The field takes a string.
    #[error("field '{0}' takes a string")]
    ExpectedString(&'static str),
    /// The field takes a list of strings.
    #[error("field '{0}' takes a list of strings")]
    ExpectedList(&'static str),
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
    fn pointer(self) -> &'static str {
        match self {
            OverrideField::Name => "/name",
            OverrideField::Mission => "/mission",
            OverrideField::Examples => "/examples",
            OverrideField::Access => "/defaults/access",
            OverrideField::Sources => "/defaults/sources",
        }
    }

    /// Checks that `value` has the shape of the field.
    ///
    /// # Errors
    ///
    /// [`OverrideValueError::ExpectedString`] or
    /// [`OverrideValueError::ExpectedList`].
    pub fn check(self, value: &Value) -> Result<(), OverrideValueError> {
        if self.is_list() {
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
    /// Fields whose value comes from an override rather than the definition.
    pub overridden: Vec<OverrideField>,
    /// The definition the overrides apply to, unchanged.
    pub definition: Value,
}

/// Resolves `stored` with its `overrides`, and the `model` read from
/// `agent_model`.
///
/// A field without an override takes the definition's value; a field the
/// definition lacks reads as empty.
pub fn resolve(
    stored: &StoredAgent,
    overrides: &[AgentOverride],
    model: Option<AgentModel>,
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
        overridden,
        definition: stored.definition.clone(),
    }
}

/// The definition of an agent that runs Python code, read from the manifest
/// its decorator declares.
///
/// The mission is the declared system prompt, or the description when there
/// is none; the access is the required then optional tools; the sources are
/// the declared data sources.
pub fn definition_from_manifest(manifest: &AgentManifest) -> Value {
    let mission = manifest
        .system_prompt
        .as_deref()
        .filter(|prompt| !prompt.trim().is_empty())
        .unwrap_or(&manifest.description);
    let mut access: Vec<&str> = Vec::new();
    for tool in manifest
        .tools_required
        .iter()
        .chain(manifest.tools_optional.iter())
    {
        if !access.contains(&tool.as_str()) {
            access.push(tool);
        }
    }
    let mut defaults = Map::new();
    if !access.is_empty() {
        defaults.insert("access".into(), json!(access));
    }
    if !manifest.datasources.is_empty() {
        defaults.insert("sources".into(), json!(manifest.datasources));
    }
    let mut definition = Map::new();
    definition.insert("id".into(), json!(manifest.name));
    definition.insert("name".into(), json!(manifest.name));
    definition.insert("mission".into(), json!(mission));
    if !manifest.examples.is_empty() {
        definition.insert("examples".into(), json!(manifest.examples));
    }
    definition.insert("defaults".into(), Value::Object(defaults));
    Value::Object(definition)
}

/// The definition of a stored agent whose manifest JSON may be unreadable:
/// read through [`definition_from_manifest`] when it parses, otherwise the
/// identifier alone, so a damaged row still migrates.
pub(crate) fn definition_from_manifest_json(name: &str, manifest_json: &str) -> Value {
    match serde_json::from_str::<AgentManifest>(manifest_json) {
        Ok(manifest) => definition_from_manifest(&manifest),
        Err(e) => {
            tracing::event!(
                tracing::Level::WARN,
                agent = %name,
                error = %e,
                "agents_db.definition.manifest_unreadable"
            );
            json!({ "id": name, "name": name, "mission": "", "defaults": {} })
        }
    }
}

#[cfg(test)]
mod tests;
