//! `DELETE /api/v1/connections/{id}`: remove a connection of any kind, and
//! answer what was erased.
//!
//! What goes, by kind:
//!
//! - an MCP server: its session and its configuration in `mcp.db`, the files
//!   installed for it, its sign-in token, the secrets its configuration names
//!   (`${APOLLIA_SECRET:KEY}`, kept under `{server}:KEY`);
//! - a mailbox: its entry in the mailbox list, and its password;
//! - a native account: its token;
//! - a folder: its entry, and the sources made on it with their indexes.
//!
//! For every kind, the tools of the connection granted to agents are revoked
//! (see [`crate::connections::removal`]), each revocation journaled as
//! `AccessChanged`; the state kept for the connection is deleted, and the
//! removal is announced as [`RuntimeEvent::ConnectionRemoved`].
//!
//! With `?dry_run=1` the route erases nothing and answers the same list, for
//! a confirmation. The revocations go first and either all apply or none
//! does, so a refusal leaves the connection whole. What comes after the
//! configuration is erased as far as it can be: what could not be is
//! answered under `left`, and logged.

use std::path::{Path as FsPath, PathBuf};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use apollia_auth::{AuthManager, SecretStore, MCP_OAUTH_SERVICE};
use apollia_connectors::imap::{ImapAccountStore, IMAP_INDEX_FILE, IMAP_SECRET_SERVICE};
use apollia_core::{ConnectionStateStore, RuntimeEvent};
use apollia_mcp::config::{static_secret_user, MCP_STATIC_SECRET_SERVICE};
use apollia_tools::tools::python_executor::venv_dir_component;
use apollia_tools::OverrideField;

use crate::api::openapi::ApiErrorBody;
use crate::api::routes_agent_definitions as definitions;
use crate::api::server::AppState;
use crate::connections::removal::{
    access_items, lists_tools, revocations, AgentLists, RemovedItem, RemovedKind, Revocation,
};
use crate::connections::{Connection, ConnectionKind, INSTALLED_FILES_DIR};
use crate::coordinator::ExecutionBackend;
use crate::governance::{ConnectionEvent, ConnectionGovernance};

use super::agents::{agents_error, find};
use super::{accounts_of, api_err, auth_manager, collect_connections, ApiError};

/// Who the connection journal names for a removal made here.
const ROUTE_ACTOR: &str = "api";

/// Query of `DELETE /api/v1/connections/{id}`.
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RemoveConnectionQuery {
    /// `1` (or `true`) to erase nothing and answer what the removal would
    /// erase.
    #[serde(default)]
    pub dry_run: Option<String>,
}

impl RemoveConnectionQuery {
    fn dry_run(&self) -> bool {
        matches!(self.dry_run.as_deref(), Some("1" | "true"))
    }
}

/// What a removal erased, or would erase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionRemoval {
    /// Identifier of the connection.
    pub connection: String,
    /// Whether nothing was erased, the list saying what would be.
    pub dry_run: bool,
    /// What was erased, or would be.
    pub removed: Vec<RemovedItem>,
    /// What could not be erased; empty on a dry run.
    pub left: Vec<RemovedItem>,
}

/// Every stored agent with the lists of tools written for it.
fn agent_lists<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
) -> Result<Vec<AgentLists>, ApiError> {
    let repo = definitions::open_agents(state).map_err(agents_error)?;
    let stored = repo
        .list_stored()
        .map_err(|e| agents_error(definitions::store_error(e)))?;
    let mut out = Vec::with_capacity(stored.len());
    for agent in stored {
        let resolved = definitions::resolved(state, &repo, &agent.name).map_err(agents_error)?;
        let overrides = repo
            .overrides(&agent.name)
            .map_err(|e| agents_error(definitions::store_error(e)))?
            .into_iter()
            .filter(|entry| lists_tools(entry.field))
            .map(|entry| (entry.field, tools_of(&entry.value)))
            .collect();
        let declared = if resolved.overridden.contains(&OverrideField::Access) {
            Vec::new()
        } else {
            resolved.access.clone()
        };
        out.push(AgentLists {
            id: agent.name,
            by_default: apollia_tools::reads_by_default(&resolved),
            overrides,
            declared,
        });
    }
    Ok(out)
}

fn tools_of(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether `store` may hold something under `(service, user)`: a read that
/// fails counts, so the delete is still tried.
fn may_hold(store: &dyn SecretStore, service: &str, user: &str) -> bool {
    !matches!(store.get(service, user), Ok(None))
}

/// The folder of the files installed for MCP server `name`.
fn installed_files(data_dir: &FsPath, name: &str) -> PathBuf {
    data_dir
        .join(INSTALLED_FILES_DIR)
        .join(venv_dir_component(name))
}

/// What removing MCP server `name` erases besides the agent accesses.
fn mcp_items<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    secrets: &dyn SecretStore,
    name: &str,
) -> Result<Vec<RemovedItem>, ApiError> {
    let config = match &state.mcp_server_repo {
        Some(repo) => repo
            .lock()
            .map_err(|_| {
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "repository lock poisoned",
                )
            })?
            .find_by_name(name)
            .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?,
        None => None,
    };
    let mut items = vec![RemovedItem::new(RemovedKind::Configuration, name)];
    let files = installed_files(&state.data_dir, name);
    if files.exists() {
        items.push(RemovedItem::new(
            RemovedKind::InstalledFiles,
            files.display().to_string(),
        ));
    }
    if may_hold(secrets, MCP_OAUTH_SERVICE, name) {
        items.push(RemovedItem::new(RemovedKind::Token, name));
    }
    for key in config.map(|c| c.static_secret_keys()).unwrap_or_default() {
        if may_hold(
            secrets,
            MCP_STATIC_SECRET_SERVICE,
            &static_secret_user(name, &key),
        ) {
            items.push(RemovedItem::new(RemovedKind::Secret, key));
        }
    }
    Ok(items)
}

/// What removing `connection` erases besides the agent accesses and its
/// state.
async fn kind_items<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    secrets: &dyn SecretStore,
    connection: &Connection,
) -> Result<Vec<RemovedItem>, ApiError> {
    let id = connection.id.as_str();
    match connection.kind {
        ConnectionKind::Mcp => mcp_items(state, secrets, id.trim_start_matches("mcp:")),
        ConnectionKind::Imap => {
            let email = id.trim_start_matches("imap:");
            let mut items = vec![RemovedItem::new(RemovedKind::Configuration, email)];
            if may_hold(secrets, IMAP_SECRET_SERVICE, email) {
                items.push(RemovedItem::new(RemovedKind::Secret, email));
            }
            Ok(items)
        }
        ConnectionKind::Native => Ok(vec![RemovedItem::new(
            RemovedKind::Token,
            connection.account.clone().unwrap_or_default(),
        )]),
        ConnectionKind::Folder => {
            let folder = crate::connections::folder_id_of(id).unwrap_or_default();
            let mut items = vec![RemovedItem::new(RemovedKind::Configuration, folder)];
            if let Some(registry) = state.source_registry.as_ref() {
                let sources = super::folders::dependent_sources(registry, folder)
                    .await
                    .map_err(|(status, Json(body))| api_err(status, body.error))?;
                items.extend(
                    sources
                        .into_iter()
                        .map(|s| RemovedItem::new(RemovedKind::Source, s.id)),
                );
            }
            Ok(items)
        }
    }
}

/// Remove the configuration of `connection`, the session, list entry,
/// account or folder that makes it a connection, and journal it, with `auth`
/// holding the native accounts. Answers what it removes with it and could
/// not: the password of a mailbox.
async fn remove_configuration<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    auth: &AuthManager,
    secrets: Box<dyn SecretStore>,
    connection: &Connection,
) -> Result<Vec<RemovedItem>, ApiError> {
    let id = connection.id.as_str();
    match connection.kind {
        ConnectionKind::Mcp => {
            crate::api::routes_mcp::remove_server_from(state, id.trim_start_matches("mcp:"))
                .await
                .map(|()| Vec::new())
                .map_err(|(status, Json(body))| {
                    api_err(status, body["error"].as_str().unwrap_or_default())
                })
        }
        ConnectionKind::Imap => {
            let store = ImapAccountStore::with_store(state.data_dir.join(IMAP_INDEX_FILE), secrets);
            let removed = store
                .remove(id.trim_start_matches("imap:"))
                .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
            state
                .connection_governance
                .record_or_warn(&ConnectionEvent::Removed {
                    connection_id: id.to_owned(),
                    actor: ROUTE_ACTOR.to_owned(),
                });
            Ok(if removed.password_deleted {
                Vec::new()
            } else {
                vec![RemovedItem::new(RemovedKind::Secret, removed.account.email)]
            })
        }
        ConnectionKind::Native => {
            let provider = id
                .strip_prefix("native:")
                .and_then(|rest| rest.split_once(':'))
                .map(|(provider, _)| provider)
                .unwrap_or_default();
            let provider = apollia_auth::connector_consent::provider_from_id(provider)
                .map_err(|e| api_err(StatusCode::BAD_REQUEST, e))?;
            crate::api::routes_connectors::revoke_and_record(
                &state.connection_governance,
                auth,
                provider,
                connection.account.as_deref().unwrap_or_default(),
            )
            .await
            .map(|()| Vec::new())
        }
        ConnectionKind::Folder => {
            let folder = crate::connections::folder_id_of(id).unwrap_or_default();
            super::folders::remove_with_sources(
                &state.connection_governance,
                &state.data_dir,
                state.source_registry.as_ref(),
                folder,
                true,
            )
            .await
            .map(|_| Vec::new())
            .map_err(|(status, Json(body))| api_err(status, body.error))
        }
    }
}

/// Erase one item that comes after the configuration. Answers whether it
/// is gone.
fn erase(
    item: &RemovedItem,
    name: &str,
    data_dir: &FsPath,
    secrets: &dyn SecretStore,
    governance: &ConnectionGovernance,
) -> bool {
    let outcome = match item.kind {
        RemovedKind::InstalledFiles => {
            match std::fs::remove_dir_all(installed_files(data_dir, name)) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }
        RemovedKind::Token => secrets
            .delete(MCP_OAUTH_SERVICE, name)
            .map_err(|e| e.to_string()),
        RemovedKind::Secret => secrets
            .delete(
                MCP_STATIC_SECRET_SERVICE,
                &static_secret_user(name, &item.name),
            )
            .map(|()| {
                governance.record_or_warn(&ConnectionEvent::SecretRemoved {
                    connection_id: format!("mcp:{name}"),
                    key: item.name.clone(),
                    actor: ROUTE_ACTOR.to_owned(),
                });
            })
            .map_err(|e| e.to_string()),
        _ => Ok(()),
    };
    match outcome {
        Ok(()) => true,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                server = %name,
                kind = ?item.kind,
                error = %error,
                "connections.remove.item_left"
            );
            false
        }
    }
}

/// Write the revocations, each journaled.
fn revoke<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    planned: &[Revocation],
) -> Result<(), ApiError> {
    if planned.is_empty() {
        return Ok(());
    }
    let governance = definitions::governance(state).map_err(agents_error)?;
    let writes: Vec<(String, OverrideField, Vec<String>)> = planned
        .iter()
        .map(|r| (r.agent.clone(), r.field, r.kept.clone()))
        .collect();
    let repo = definitions::open_agents(state).map_err(agents_error)?;
    definitions::writer(&repo, &governance)
        .set_tool_lists_of_many(&writes)
        .map_err(|e| agents_error(definitions::override_error(e)))
}

/// Delete the state kept for connection `id`, and announce the removal.
/// Answers whether a state was kept.
fn forget_state<B: ExecutionBackend + Clone>(state: &AppState<B>, id: &str) -> bool {
    let path = apollia_core::paths::DataFile::System.path(&state.data_dir);
    let forgotten = match ConnectionStateStore::open(&path).and_then(|store| store.delete(id)) {
        Ok(found) => found,
        Err(e) => {
            tracing::event!(tracing::Level::WARN, connection = %id, error = %e, "connections.remove.state_left");
            false
        }
    };
    let _ = state
        .event_sender
        .send(RuntimeEvent::ConnectionRemoved { id: id.to_owned() });
    forgotten
}

/// Whether a state is kept for connection `id`.
fn has_state(data_dir: &FsPath, id: &str) -> bool {
    crate::connections::state::kept_states(data_dir).contains_key(id)
}

/// Remove the connection `id`, with `auth` holding the native accounts, or
/// with `dry_run` only say what would go.
pub(crate) async fn remove<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    auth: &AuthManager,
    id: &str,
    dry_run: bool,
) -> Result<ConnectionRemoval, ApiError> {
    let connections = collect_connections(state, accounts_of(auth).await?).await?;
    let connection = find(&connections, id)?.clone();
    let shared = connection.kind == ConnectionKind::Imap
        && connections
            .iter()
            .any(|other| other.kind == ConnectionKind::Imap && other.id != connection.id);
    let planned = revocations(&connection, shared, &agent_lists(state)?);
    let open = || {
        (state.secret_store)().map_err(|e| {
            api_err(
                StatusCode::SERVICE_UNAVAILABLE,
                format!("secret store unavailable: {e}"),
            )
        })
    };
    let secrets = open()?;
    let items = kind_items(state, secrets.as_ref(), &connection).await?;
    let state_kept = has_state(&state.data_dir, id);
    let mut removed = items.clone();
    removed.extend(access_items(&planned));
    if state_kept {
        removed.push(RemovedItem::new(RemovedKind::State, id));
    }
    if dry_run {
        return Ok(ConnectionRemoval {
            connection: connection.id,
            dry_run,
            removed,
            left: Vec::new(),
        });
    }

    revoke(state, &planned)?;
    let mut left = remove_configuration(state, auth, open()?, &connection).await?;
    let name = id.trim_start_matches("mcp:");
    if connection.kind == ConnectionKind::Mcp {
        for item in items
            .iter()
            .filter(|i| i.kind != RemovedKind::Configuration)
        {
            if !erase(
                item,
                name,
                &state.data_dir,
                secrets.as_ref(),
                &state.connection_governance,
            ) {
                left.push(item.clone());
            }
        }
    }
    if !forget_state(state, id) && state_kept {
        left.push(RemovedItem::new(RemovedKind::State, id));
    }
    removed.retain(|item| !left.contains(item));
    tracing::event!(
        tracing::Level::INFO,
        connection = %id,
        removed = removed.len(),
        left = left.len(),
        "connections.removed"
    );
    Ok(ConnectionRemoval {
        connection: connection.id,
        dry_run,
        removed,
        left,
    })
}

/// `DELETE /api/v1/connections/{id}`: remove a connection of any kind, and
/// answer what was erased; with `?dry_run=1`, erase nothing and answer what
/// would be.
#[utoipa::path(
    delete,
    path = "/api/v1/connections/{id}",
    tag = "connections",
    params(
        ("id" = String, Path, description = "Connection identifier, as `GET /api/v1/connections` lists it"),
        RemoveConnectionQuery,
    ),
    responses(
        (status = 200, description = "Removed, or on a dry run what would be; what was erased, and what could not be", body = ConnectionRemoval),
        (status = 404, description = "Connection not found; nothing erased", body = ApiErrorBody),
        (status = 500, description = "A store could not be read or written", body = ApiErrorBody),
        (status = 503, description = "Secret store or audit journal not available; nothing erased", body = ApiErrorBody),
    )
)]
pub(crate) async fn remove_connection<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Query(query): Query<RemoveConnectionQuery>,
) -> Result<Json<ConnectionRemoval>, ApiError> {
    let auth = auth_manager().await?;
    remove(&state, &auth, &id, query.dry_run()).await.map(Json)
}

#[cfg(test)]
mod tests;
