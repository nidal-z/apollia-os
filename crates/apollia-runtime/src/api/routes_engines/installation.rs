//! The followed installation of an engine: started by a request that answers
//! at once, read while it runs, cancelled or forgotten.
//!
//! Every answer is the state of the installation. A refusal is a state too,
//! with its code, and never an error status: the screen that started the
//! installation reads why it stopped from the same place it reads its
//! progress.

use std::path::{Path, PathBuf};

use axum::extract::State;
use axum::Json;

use crate::api::routes_engines::{EngineImportRequest, EngineInstallRequest};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::engine_store::install_job::{self, InstallKind, InstallationState};
use crate::engine_store::{self, EngineStore, EngineStoreSettings};

/// The store and its settings, or the refusal recorded as the outcome.
fn open(
    data_dir: &Path,
    kind: InstallKind,
) -> Result<(EngineStore, EngineStoreSettings), InstallationState> {
    let root = engine_store::store_root(data_dir);
    let store = EngineStore::open(data_dir).map_err(|e| install_job::refuse(&root, kind, &e))?;
    let settings =
        EngineStoreSettings::load(&root).map_err(|e| install_job::refuse(&root, kind, &e))?;
    Ok((store, settings))
}

#[utoipa::path(
    get,
    path = "/api/v1/engines/installation",
    tag = "engines",
    responses(
        (status = 200, description = "Where the installation stands", body = InstallationState),
    )
)]
pub async fn engine_installation_state<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<InstallationState> {
    Json(install_job::state(&engine_store::store_root(
        &state.data_dir,
    )))
}

#[utoipa::path(
    post,
    path = "/api/v1/engines/installation",
    tag = "engines",
    request_body = EngineInstallRequest,
    responses(
        (status = 200, description = "The installation started, the one already running, or the refusal that kept it from starting", body = InstallationState),
    )
)]
pub async fn start_engine_installation<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<EngineInstallRequest>,
) -> Json<InstallationState> {
    let data_dir = state.data_dir.clone();
    let started = tokio::task::spawn_blocking(move || open(&data_dir, InstallKind::Install))
        .await
        .unwrap_or(Err(InstallationState::Idle));
    Json(match started {
        Ok((store, settings)) => {
            install_job::start_download(&store, &settings, body.engine, body.variant)
        }
        Err(refused) => refused,
    })
}

#[utoipa::path(
    post,
    path = "/api/v1/engines/installation/import",
    tag = "engines",
    request_body = EngineImportRequest,
    responses(
        (status = 200, description = "The import started, the installation already running, or the refusal that kept it from starting", body = InstallationState),
    )
)]
pub async fn start_engine_import<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<EngineImportRequest>,
) -> Json<InstallationState> {
    let data_dir = state.data_dir.clone();
    let started = tokio::task::spawn_blocking(move || open(&data_dir, InstallKind::Import))
        .await
        .unwrap_or(Err(InstallationState::Idle));
    Json(match started {
        Ok((store, _)) => install_job::start_import(&store, &PathBuf::from(body.path)),
        Err(refused) => refused,
    })
}

#[utoipa::path(
    delete,
    path = "/api/v1/engines/installation",
    tag = "engines",
    responses(
        (status = 200, description = "The download cancelled or the outcome forgotten; a verification or an extraction is left to end", body = InstallationState),
    )
)]
pub async fn dismiss_engine_installation<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<InstallationState> {
    Json(install_job::dismiss(&engine_store::store_root(&state.data_dir)).await)
}
