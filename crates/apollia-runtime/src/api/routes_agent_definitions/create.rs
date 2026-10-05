//! `POST /api/v1/agent-definitions`: an agent created without code.
//!
//! The body is the sheet the person validated: a name, a mission, examples,
//! accesses and sources. The agent is stored with the origin `interface`,
//! under an identifier derived from its name. An absent access list leaves
//! the agent on what an agent created without code reads by default; a list,
//! even empty, is the choice of the person and replaces that default. The
//! model is not part of the body: it is assigned as for any agent, through
//! `PUT /api/v1/llm/agents/{agent}/model`.
//!
//! The creation is recorded in the audit journal, then one entry per access
//! granted; without a journal the agent is not created.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use apollia_tools::{AgentOrigin, AgentRepository, StoredAgent};

use super::{
    check_sources, governance, open_agents, resolved, shown, store_error, ResolvedAgentResponse,
    API_ACTOR,
};
use crate::api::routes_llm::servers::{api_err, ApiError};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::governance::{AccessChange, GovernanceContract, GovernanceEvent};

/// Version of the definition of an agent created in the interface.
const INTERFACE_AGENT_VERSION: &str = "1";

/// Longest identifier derived from a name, before a numeric suffix.
const MAX_ID_CHARS: usize = 48;

/// How many identifiers are tried before a name is refused as taken.
const MAX_ID_ATTEMPTS: u32 = 99;

/// Body of `POST /api/v1/agent-definitions`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateAgentRequest {
    /// Display name. The identifier of the agent is derived from it.
    pub name: String,
    /// What the agent is for, used as its instructions.
    pub mission: String,
    /// Example requests.
    #[serde(default)]
    pub examples: Vec<String>,
    /// Accesses granted. Absent, the agent reads by default, as any agent
    /// created without code; a list, even empty, replaces that default.
    #[serde(default)]
    pub access: Option<Vec<String>>,
    /// Sources checked: identifiers of `GET /api/v1/sources`, `profile` or
    /// `memory`.
    #[serde(default)]
    pub sources: Vec<String>,
}

/// The ASCII letters a Latin letter with a diacritic reads as, in an
/// identifier.
fn folded(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{e0}' | '\u{e1}' | '\u{e2}' | '\u{e3}' | '\u{e4}' | '\u{e5}' => "a",
        '\u{e7}' => "c",
        '\u{e8}' | '\u{e9}' | '\u{ea}' | '\u{eb}' => "e",
        '\u{ec}' | '\u{ed}' | '\u{ee}' | '\u{ef}' => "i",
        '\u{f1}' => "n",
        '\u{f2}' | '\u{f3}' | '\u{f4}' | '\u{f5}' | '\u{f6}' => "o",
        '\u{f9}' | '\u{fa}' | '\u{fb}' | '\u{fc}' => "u",
        '\u{fd}' | '\u{ff}' => "y",
        '\u{e6}' => "ae",
        '\u{153}' => "oe",
        '\u{df}' => "ss",
        _ => return None,
    })
}

/// The identifier a display name reads as: lowercase ASCII letters and
/// digits, every run of anything else one hyphen, `agent` when nothing is
/// left.
pub(super) fn identifier_of(name: &str) -> String {
    let mut id = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            id.push(c);
        } else if let Some(letters) = folded(c) {
            id.push_str(letters);
        } else if !id.is_empty() && !id.ends_with('-') {
            id.push('-');
        }
        if id.len() >= MAX_ID_CHARS {
            break;
        }
    }
    let id = id.trim_end_matches('-');
    if id.is_empty() {
        "agent".to_owned()
    } else {
        id.to_owned()
    }
}

/// The first identifier derived from `name` that no stored agent holds:
/// the identifier itself, then `-2`, `-3`, and so on.
fn free_identifier(repo: &AgentRepository, name: &str) -> Result<String, ApiError> {
    let base = identifier_of(name);
    for attempt in 1..=MAX_ID_ATTEMPTS {
        let candidate = if attempt == 1 {
            base.clone()
        } else {
            format!("{base}-{attempt}")
        };
        if repo.stored(&candidate).map_err(store_error)?.is_none() {
            return Ok(candidate);
        }
    }
    Err(api_err(
        StatusCode::CONFLICT,
        format!("too many agents are named '{base}': choose another name"),
    ))
}

fn trimmed(items: Vec<String>) -> Vec<String> {
    let mut kept: Vec<String> = Vec::with_capacity(items.len());
    for item in items {
        let item = item.trim();
        if !item.is_empty() && !kept.iter().any(|known| known == item) {
            kept.push(item.to_owned());
        }
    }
    kept
}

/// `POST /api/v1/agent-definitions`: create an agent without code.
#[utoipa::path(
    post,
    path = "/api/v1/agent-definitions",
    tag = "agents",
    request_body = CreateAgentRequest,
    responses(
        (status = 201, description = "Agent created; the agent in force", body = ResolvedAgentResponse),
        (status = 409, description = "Too many agents hold identifiers derived from this name", body = crate::api::openapi::ApiErrorBody),
        (status = 422, description = "An empty name or mission, or a source the registry does not hold", body = crate::api::openapi::ApiErrorBody),
        (status = 503, description = "Audit journal, or the source registry a sources list needs, not available", body = crate::api::openapi::ApiErrorBody),
    )
)]
pub async fn create_agent_definition<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<(StatusCode, Json<ResolvedAgentResponse>), ApiError> {
    let name = req.name.trim().to_owned();
    if name.is_empty() {
        return Err(api_err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "field 'name' is empty",
        ));
    }
    let mission = req.mission.trim().to_owned();
    if mission.is_empty() {
        return Err(api_err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "field 'mission' is empty",
        ));
    }
    let examples = trimmed(req.examples);
    let access = req.access.map(trimmed);
    let sources = trimmed(req.sources);
    check_sources(&state, &json!(sources)).await?;
    let governance = governance(&state)?;

    let repo = open_agents(&state)?;
    let id = free_identifier(&repo, &name)?;
    let mut defaults = Map::new();
    if let Some(access) = &access {
        defaults.insert("access".to_owned(), json!(access));
    }
    if !sources.is_empty() {
        defaults.insert("sources".to_owned(), json!(sources));
    }
    let now = chrono::Utc::now().to_rfc3339();
    repo.save_definition(&StoredAgent {
        name: id.clone(),
        origin: AgentOrigin::Interface,
        package: None,
        version: INTERFACE_AGENT_VERSION.to_owned(),
        definition: json!({
            "id": id,
            "name": name,
            "mission": mission,
            "examples": examples,
            "defaults": Value::Object(defaults),
        }),
        enabled: true,
        installed_at: now.clone(),
        updated_at: now,
    })
    .map_err(store_error)?;
    let recorded = |event: GovernanceEvent| {
        governance
            .record(&event)
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))
    };
    recorded(GovernanceEvent::Installed {
        agent_id: id.clone(),
        version: INTERFACE_AGENT_VERSION.to_owned(),
        signed: false,
        actor: API_ACTOR.to_owned(),
    })?;
    for access_id in access.iter().flatten() {
        recorded(GovernanceEvent::AccessChanged {
            agent_id: id.clone(),
            access_id: access_id.clone(),
            change: AccessChange::Granted,
            actor: API_ACTOR.to_owned(),
            reason: None,
        })?;
    }
    tracing::event!(
        tracing::Level::INFO,
        agent = %id,
        origin = "interface",
        "agents.definition.created"
    );
    let _ = state
        .event_sender
        .send(apollia_core::events::RuntimeEvent::AgentInstalled {
            name: id.clone(),
            version: INTERFACE_AGENT_VERSION.to_owned(),
        });
    let agent = resolved(&state, &repo, &id)?;
    drop(repo);
    Ok((StatusCode::CREATED, Json(shown(&state, agent).await)))
}
