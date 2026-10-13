//! The draft of an agent, checked against what the machine holds.
//!
//! The Agents screen creates an agent from a short form: the agent workshop,
//! a workflow shipped with Apollia, turns the form into a sheet (a name, a
//! mission, examples, accesses, sources), and the person reads that sheet
//! over in a form before saving it. This module is the part the runtime
//! keeps for itself: what a sheet may hold, and what a proposed sheet becomes
//! once checked. Nothing here writes: an agent exists only once the person
//! saves the form.
//!
//! The draft holds only what exists on the machine. An access the tool
//! registry does not hold, a source nobody added and a model nobody declared
//! are left out and named under `unavailable`. An access of a sensitive
//! family is kept, unchecked: granting it is a choice the person makes in the
//! form. The default access, which an agent created without code is granted
//! when nobody chooses for it (the reading family, then what
//! `[tools.new_agent]` adds), is added to every draft, checked and marked as
//! a default: a sensitive access is checked there only because the setting
//! grants it to every new agent.
//!
//! This module is stateless: it reads a [`DraftOffer`] from the registries,
//! and checks a [`ProposedSheet`] against one.

use apollia_tools::descriptor::ActionFamily;
use apollia_tools::{DefaultAccess, ToolRegistryHandle};
use serde::{Deserialize, Serialize};

/// The sources every agent may be given, whatever the registry holds.
const BUILT_IN_SOURCES: [(&str, &str); 2] = [
    ("profile", "the profile of the person"),
    ("memory", "the memory the agent keeps across its runs"),
];

/// Longest name a draft keeps.
const MAX_NAME_CHARS: usize = 80;

/// Most examples a draft keeps.
const MAX_EXAMPLES: usize = 6;

/// One access that exists on the machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct OfferedAccess {
    /// The identifier an agent is granted it by.
    pub id: String,
    /// What it does, in one line.
    pub description: String,
    /// Its action family, as the journal names it.
    pub family: String,
    /// Whether its family is anything but plain reading.
    pub sensitive: bool,
    /// Whether an agent created without code is granted it by default: a
    /// tool of a family the default grants, or one the default names.
    pub by_default: bool,
}

/// One source that exists on the machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct OfferedSource {
    /// The identifier an agent's source list names it by.
    pub id: String,
    /// The name the person sees.
    pub label: String,
}

/// What a draft may hold: the accesses, the sources and the models that exist
/// on the machine when it is read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct DraftOffer {
    /// The tools of the registry, the disabled ones left out, and the code
    /// context, which a checked code folder grants rather than an access.
    pub accesses: Vec<OfferedAccess>,
    /// The profile, the memory, then the sources of the registry.
    pub sources: Vec<OfferedSource>,
    /// The models an agent can be assigned, by the name the router routes by.
    pub models: Vec<String>,
}

/// Whether an access of `family` is left unchecked in a draft: every family
/// but plain reading, an unclassified tool among them.
pub fn is_sensitive(family: ActionFamily) -> bool {
    !matches!(family, ActionFamily::Read | ActionFamily::Exempt)
}

impl DraftOffer {
    /// Reads the offer: the tools `registry` holds without the `disabled`
    /// ones, those `default` grants marked as such, the built-in sources
    /// then `sources`, and `models`. A registry that cannot be listed offers
    /// no access: a draft never proposes what could not be checked.
    pub async fn read(
        registry: &ToolRegistryHandle,
        disabled: &[String],
        default: &DefaultAccess,
        sources: Vec<OfferedSource>,
        models: Vec<String>,
    ) -> Self {
        let descriptors = match registry.list().await {
            Ok(descriptors) => descriptors,
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %error,
                    detail = "the draft proposes no access",
                    "chat.agent_draft.registry.unavailable"
                );
                Vec::new()
            }
        };
        let mut accesses: Vec<OfferedAccess> = descriptors
            .into_iter()
            .filter(|d| !disabled.contains(&d.name) && d.name != apollia_tools::CODE_CONTEXT_TOOL)
            .map(|d| OfferedAccess {
                sensitive: is_sensitive(d.family),
                family: d.family.as_str().to_owned(),
                description: d
                    .description
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_owned(),
                by_default: default.families.contains(&d.family) || default.tools.contains(&d.name),
                id: d.name,
            })
            .collect();
        accesses.sort_by(|a, b| a.id.cmp(&b.id));
        accesses.dedup_by(|a, b| a.id == b.id);
        let mut offered: Vec<OfferedSource> = BUILT_IN_SOURCES
            .iter()
            .map(|(id, label)| OfferedSource {
                id: (*id).to_owned(),
                label: (*label).to_owned(),
            })
            .collect();
        offered.extend(
            sources
                .into_iter()
                .filter(|source| !BUILT_IN_SOURCES.iter().any(|(id, _)| *id == source.id)),
        );
        Self {
            accesses,
            sources: offered,
            models,
        }
    }

    fn access(&self, id: &str) -> Option<&OfferedAccess> {
        self.accesses.iter().find(|access| access.id == id)
    }
}

/// One access of a draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DraftAccess {
    /// The identifier the agent would be granted it by.
    pub id: String,
    /// Whether the form opens with it granted. A sensitive access is only
    /// when the default grants it to every new agent.
    pub checked: bool,
    /// Whether its family is anything but plain reading.
    pub sensitive: bool,
    /// Whether it is there because an agent created without code is granted
    /// it by default, rather than because the sheet asked for it.
    pub by_default: bool,
}

/// The draft of an agent, as the form opens it. Nothing of it is stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AgentDraft {
    /// Display name.
    pub name: String,
    /// What the agent is for, used as its instructions.
    pub mission: String,
    /// Example requests.
    pub examples: Vec<String>,
    /// The model proposed, `None` for the default model of the machine.
    pub model: Option<String>,
    /// The accesses proposed, then the reading family, each one existing on
    /// the machine.
    pub access: Vec<DraftAccess>,
    /// The sources proposed, each one existing on the machine.
    pub sources: Vec<String>,
    /// What the sheet asked for and the machine does not hold: accesses,
    /// sources (`source:<id>`) and a model (`model:<name>`).
    pub unavailable: Vec<String>,
}

/// A sheet as it is proposed, before it is checked: every list optional.
#[derive(Debug, Clone, Default, Deserialize, utoipa::ToSchema)]
pub struct ProposedSheet {
    /// Display name.
    pub name: String,
    /// What the agent is for, used as its instructions.
    pub mission: String,
    /// Example requests.
    #[serde(default)]
    pub examples: Vec<String>,
    /// A model to assign; absent for the default model of the machine.
    #[serde(default)]
    pub model: Option<String>,
    /// Identifiers of the accesses asked for.
    #[serde(default)]
    pub access: Vec<String>,
    /// Identifiers of the sources asked for.
    #[serde(default)]
    pub sources: Vec<String>,
}

/// Why a proposed sheet yields no draft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DraftRefusal {
    /// The sheet has no name.
    #[error("field 'name' is empty")]
    EmptyName,
    /// The sheet has no mission.
    #[error("field 'mission' is empty")]
    EmptyMission,
}

fn trimmed_unique(items: Vec<String>) -> Vec<String> {
    let mut seen: Vec<String> = Vec::with_capacity(items.len());
    for item in items {
        let item = item.trim();
        if !item.is_empty() && !seen.iter().any(|known| known == item) {
            seen.push(item.to_owned());
        }
    }
    seen
}

/// Checks the proposed `sheet` against `offer` and answers the draft the
/// form opens.
///
/// # Errors
///
/// [`DraftRefusal`] for a sheet without a name or without a mission.
pub fn check_draft(offer: &DraftOffer, sheet: ProposedSheet) -> Result<AgentDraft, DraftRefusal> {
    let args = sheet;
    let name: String = args.name.trim().chars().take(MAX_NAME_CHARS).collect();
    if name.is_empty() {
        return Err(DraftRefusal::EmptyName);
    }
    let mission = args.mission.trim().to_owned();
    if mission.is_empty() {
        return Err(DraftRefusal::EmptyMission);
    }
    let mut examples = trimmed_unique(args.examples);
    examples.truncate(MAX_EXAMPLES);

    let mut unavailable = Vec::new();
    let mut access = Vec::new();
    for id in trimmed_unique(args.access) {
        match offer.access(&id) {
            Some(offered) => {
                // A sensitive access the sheet asked for stays the
                // choice of the person, unless the default already grants it.
                let granted_by_default = offered.sensitive && offered.by_default;
                access.push(DraftAccess {
                    id,
                    checked: !offered.sensitive || granted_by_default,
                    sensitive: offered.sensitive,
                    by_default: granted_by_default,
                });
            }
            None => unavailable.push(id),
        }
    }
    for offered in offer.accesses.iter().filter(|offered| offered.by_default) {
        if !access.iter().any(|listed| listed.id == offered.id) {
            access.push(DraftAccess {
                id: offered.id.clone(),
                checked: true,
                sensitive: offered.sensitive,
                by_default: true,
            });
        }
    }
    let mut sources = Vec::new();
    for id in trimmed_unique(args.sources) {
        if offer.sources.iter().any(|source| source.id == id) {
            sources.push(id);
        } else {
            unavailable.push(format!("source:{id}"));
        }
    }
    let model = args
        .model
        .map(|model| model.trim().to_owned())
        .filter(|model| !model.is_empty());
    let model = match model {
        Some(model) if offer.models.contains(&model) => Some(model),
        Some(model) => {
            unavailable.push(format!("model:{model}"));
            None
        }
        None => None,
    };

    Ok(AgentDraft {
        name,
        mission,
        examples,
        model,
        access,
        sources,
        unavailable,
    })
}

#[cfg(test)]
mod tests;
