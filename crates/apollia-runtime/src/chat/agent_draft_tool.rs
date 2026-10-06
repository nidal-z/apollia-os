//! The `propose_agent` built-in tool.
//!
//! Offered to the Apollia agent in one kind of conversation only: the one the
//! Agents screen opens to create an agent by describing it. The model calls
//! it with what it understood (a name, a mission, examples, a model, accesses,
//! sources) and the tool answers with a draft the interface opens in the form
//! of an agent sheet. The tool writes nothing: an agent exists only once the
//! person saves the form.
//!
//! The draft holds only what exists on the machine. An access the tool
//! registry does not hold, a source nobody added and a model nobody declared
//! are left out and named under `unavailable`, so the model can say so. An
//! access of a sensitive family is kept, unchecked: granting it is a choice
//! the person makes in the form. The default access, which an agent created
//! without code is granted when nobody chooses for it (the reading family,
//! then what `[tools.new_agent]` adds), is added to every draft, checked and
//! marked as a default: a sensitive access is checked there only because the
//! setting grants it to every new agent.
//!
//! This module is stateless: it parses the arguments, checks them against a
//! [`DraftOffer`] read when the exchange starts, and shapes the answer.

use apollia_llm::types::ToolSpec;
use apollia_tools::descriptor::ActionFamily;
use apollia_tools::{DefaultAccess, ToolRegistryHandle};
use serde::{Deserialize, Serialize};

/// Name under which the tool is advertised to the model and matched in the loop.
pub const AGENT_DRAFT_TOOL_NAME: &str = "propose_agent";

/// The sources every agent may be given, whatever the registry holds.
const BUILT_IN_SOURCES: [(&str, &str); 2] = [
    ("profile", "the profile of the person"),
    ("memory", "the memory the agent keeps across its runs"),
];

/// Longest name a draft keeps.
const MAX_NAME_CHARS: usize = 80;

/// Most examples a draft keeps.
const MAX_EXAMPLES: usize = 6;

/// Whether a conversation created with `tools` is the one that drafts an
/// agent: its tool list names the draft tool. A conversation created without
/// a list is given the tools of the registry, which never holds this one.
pub fn is_draft_session(tools: &[String]) -> bool {
    tools.iter().any(|tool| tool == AGENT_DRAFT_TOOL_NAME)
}

/// One access that exists on the machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferedAccess {
    /// The identifier an agent is granted it by.
    pub id: String,
    /// What it does, in one line.
    pub description: String,
    /// Whether its family is anything but plain reading.
    pub sensitive: bool,
    /// Whether an agent created without code is granted it by default: a
    /// tool of a family the default grants, or one the default names.
    pub by_default: bool,
}

/// One source that exists on the machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferedSource {
    /// The identifier an agent's source list names it by.
    pub id: String,
    /// The name the person sees.
    pub label: String,
}

/// What a draft may hold: the accesses, the sources and the models that exist
/// on the machine when the exchange starts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
                by_default: default.families.contains(&d.family) || default.tools.contains(&d.name),
                id: d.name,
                description: d.description,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftAccess {
    /// The identifier the agent would be granted it by.
    pub id: String,
    /// Whether the form opens with it granted. A sensitive access is only
    /// when the default grants it to every new agent.
    pub checked: bool,
    /// Whether its family is anything but plain reading.
    pub sensitive: bool,
    /// Whether it is there because an agent created without code is granted
    /// it by default, rather than because the conversation asked for it.
    pub by_default: bool,
}

/// The draft of an agent, as the form opens it. Nothing of it is stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// What the model asked for and the machine does not hold: accesses,
    /// sources (`source:<id>`) and a model (`model:<name>`).
    pub unavailable: Vec<String>,
}

/// Result returned to the model after a `propose_agent` call.
#[derive(Debug, Serialize)]
pub struct AgentDraftResult {
    /// Whether a draft was produced.
    pub ok: bool,
    /// Why not, when `ok` is false; omitted otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The draft, when `ok` is true; omitted otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<AgentDraft>,
    /// What to tell the person next; omitted on a failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl AgentDraftResult {
    fn failure(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
            draft: None,
            note: None,
        }
    }
}

/// The arguments of a call, every list optional.
#[derive(Debug, Deserialize)]
struct DraftArguments {
    name: String,
    mission: String,
    #[serde(default)]
    examples: Vec<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    access: Vec<String>,
    #[serde(default)]
    sources: Vec<String>,
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

/// Execute the `propose_agent` built-in tool.
///
/// Parses `raw_args` and answers with the draft `offer` allows. A payload
/// without a name or a mission yields `ok: false` with an explanatory
/// `error`; the caller injects the serialized result as the tool message and
/// the loop continues.
pub fn run_agent_draft(offer: &DraftOffer, raw_args: &serde_json::Value) -> AgentDraftResult {
    let args: DraftArguments = match serde_json::from_value(raw_args.clone()) {
        Ok(args) => args,
        Err(e) => return AgentDraftResult::failure(format!("invalid payload: {e}")),
    };
    let name: String = args.name.trim().chars().take(MAX_NAME_CHARS).collect();
    if name.is_empty() {
        return AgentDraftResult::failure("invalid payload: 'name' is empty");
    }
    let mission = args.mission.trim().to_owned();
    if mission.is_empty() {
        return AgentDraftResult::failure("invalid payload: 'mission' is empty");
    }
    let mut examples = trimmed_unique(args.examples);
    examples.truncate(MAX_EXAMPLES);

    let mut unavailable = Vec::new();
    let mut access = Vec::new();
    for id in trimmed_unique(args.access) {
        match offer.access(&id) {
            Some(offered) => {
                // A sensitive access the conversation asked for stays the
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

    let note = if unavailable.is_empty() {
        "The draft is shown to the person, who reviews it in a form. Nothing is saved yet."
            .to_owned()
    } else {
        "The draft is shown to the person, who reviews it in a form. Nothing is saved yet. \
         Tell the person that the entries under 'unavailable' do not exist on this machine \
         and were left out."
            .to_owned()
    };
    AgentDraftResult {
        ok: true,
        error: None,
        draft: Some(AgentDraft {
            name,
            mission,
            examples,
            model,
            access,
            sources,
            unavailable,
        }),
        note: Some(note),
    }
}

/// Build the [`ToolSpec`] advertised to the model for `propose_agent`.
#[must_use]
pub fn agent_draft_spec() -> ToolSpec {
    ToolSpec {
        name: AGENT_DRAFT_TOOL_NAME.to_string(),
        description: "Propose the draft of the agent the person describes. Nothing is saved: \
                      the person reviews the draft in a form. Call it as soon as you know what \
                      the agent is for, and again each time the person changes something, with \
                      the complete draft every time."
            .to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "required": ["name", "mission"],
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Short display name of the agent."
                },
                "mission": {
                    "type": "string",
                    "description": "What the agent is for, written as its instructions."
                },
                "examples": {
                    "type": "array",
                    "description": "Example requests the person would send it.",
                    "items": { "type": "string" }
                },
                "model": {
                    "type": "string",
                    "description": "Name of a model from the list of the instructions. Omit to keep the default model."
                },
                "access": {
                    "type": "array",
                    "description": "Identifiers of accesses from the list of the instructions, and no other.",
                    "items": { "type": "string" }
                },
                "sources": {
                    "type": "array",
                    "description": "Identifiers of sources from the list of the instructions, and no other.",
                    "items": { "type": "string" }
                }
            }
        }),
    }
}

/// The instructions of the conversation that drafts an agent: what to ask,
/// when to call the tool, and what exists on the machine.
pub fn creation_prompt(offer: &DraftOffer) -> String {
    let mut prompt = String::from(
        "You are Apollia. The person wants to create an agent without writing code, by \
         describing it to you.\n\
         Ask one short question at a time, four at most, to learn what the agent is for, \
         which requests it will receive, what it must reach and what it must consult. \
         Answer in the language of the person.\n\
         As soon as you know what the agent is for, call `propose_agent` with the complete \
         draft, and call it again each time the person changes something. The draft is \
         shown beside the conversation. Nothing is saved: the person reviews the draft in \
         a form and saves it there.\n\
         Propose only the accesses, sources and models listed below. When the person asks \
         for something that is not listed, say that it does not exist on this machine yet \
         and that it can be added in Connections, and leave it out of the draft. An access \
         marked sensitive is proposed unchecked: say that the person decides in the form. \
         The accesses marked default are added to every draft, checked: do not list them \
         yourself.\n",
    );
    prompt.push_str("\n## Accesses on this machine\n");
    if offer.accesses.is_empty() {
        prompt.push_str("None.\n");
    }
    for access in &offer.accesses {
        let description = access.description.lines().next().unwrap_or_default().trim();
        let mark = match (access.by_default, access.sensitive) {
            (true, true) => " (default, sensitive)",
            (true, false) => " (default)",
            (false, true) => " (sensitive)",
            (false, false) => "",
        };
        prompt.push_str(&format!("- `{}`{mark}: {description}\n", access.id));
    }
    prompt.push_str("\n## Sources on this machine\n");
    for source in &offer.sources {
        prompt.push_str(&format!("- `{}`: {}\n", source.id, source.label));
    }
    prompt.push_str("\n## Models on this machine\n");
    if offer.models.is_empty() {
        prompt.push_str("Only the default model.\n");
    }
    for model in &offer.models {
        prompt.push_str(&format!("- `{model}`\n"));
    }
    prompt
}

#[cfg(test)]
mod tests;
