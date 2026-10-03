//! Tauri IPC commands for session artifacts.
//!
//! Artifacts are snapshots of agent tool outputs (file contents, code blocks,
//! specs, diagrams, bash logs) captured per chat session, kept in
//! `artifacts.db` by [`apollia_runtime::artifacts`]. With the runtime in this
//! process, each command calls that module on `~/.apollia`; attached to a
//! daemon, it reaches the same functions through `/api/v1/artifacts`, against
//! the daemon's data directory.

use apollia_runtime::api::routes_artifacts::DeletedArtifact;
use apollia_runtime::artifacts::{self, ArtifactStore, UpdateArtifactRequest};
pub use apollia_runtime::artifacts::{Artifact, SaveArtifactRequest};
use tauri::State;

use crate::attached::{segment, AttachedClient, RuntimeLink};

/// The store of the runtime running in this process: `artifacts.db` of
/// `~/.apollia`.
fn embedded_store() -> Result<ArtifactStore, String> {
    let home = apollia_core::paths::home_string_or_err()
        .map_err(|_| "cannot determine home directory: $HOME not set".to_string())?;
    Ok(ArtifactStore::under(&apollia_core::paths::data_dir_under(
        home,
    )))
}

/// Persist a new artifact. Returns the stored row.
#[tauri::command]
pub async fn save_artifact(
    link: State<'_, RuntimeLink>,
    request: SaveArtifactRequest,
) -> Result<Artifact, String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => artifacts::save(&embedded_store()?, request)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => remote_save(remote, &request).await,
    }
}

/// List artifacts for a session, most recent first.
#[tauri::command]
pub async fn list_artifacts(
    link: State<'_, RuntimeLink>,
    session_id: String,
) -> Result<Vec<Artifact>, String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => artifacts::list(&embedded_store()?, session_id)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => remote_list(remote, &session_id).await,
    }
}

/// Update the content and/or title of an artifact.
#[tauri::command]
pub async fn update_artifact(
    link: State<'_, RuntimeLink>,
    id: String,
    title: Option<String>,
    content: Option<String>,
) -> Result<Artifact, String> {
    let change = UpdateArtifactRequest { title, content };
    match link.inner() {
        RuntimeLink::Embedded(_) => artifacts::update(&embedded_store()?, id, change)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => remote_update(remote, &id, &change).await,
    }
}

/// Delete an artifact by id. Idempotent: returns `Ok(())` even when absent.
#[tauri::command]
pub async fn delete_artifact(link: State<'_, RuntimeLink>, id: String) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => artifacts::delete(&embedded_store()?, id)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => remote_delete(remote, &id).await,
    }
}

/// `POST /api/v1/artifacts`, for `save_artifact`.
pub(crate) async fn remote_save(
    remote: &AttachedClient,
    request: &SaveArtifactRequest,
) -> Result<Artifact, String> {
    Ok(remote.post("/api/v1/artifacts", request).await?)
}

/// `GET /api/v1/sessions/{id}/artifacts`, for `list_artifacts`.
pub(crate) async fn remote_list(
    remote: &AttachedClient,
    session_id: &str,
) -> Result<Vec<Artifact>, String> {
    let path = format!("/api/v1/sessions/{}/artifacts", segment(session_id));
    Ok(remote.get(&path).await?)
}

/// `PATCH /api/v1/artifacts/{id}`, for `update_artifact`.
pub(crate) async fn remote_update(
    remote: &AttachedClient,
    id: &str,
    change: &UpdateArtifactRequest,
) -> Result<Artifact, String> {
    let path = format!("/api/v1/artifacts/{}", segment(id));
    Ok(remote.patch(&path, change).await?)
}

/// `DELETE /api/v1/artifacts/{id}`, for `delete_artifact`.
pub(crate) async fn remote_delete(remote: &AttachedClient, id: &str) -> Result<(), String> {
    let path = format!("/api/v1/artifacts/{}", segment(id));
    let _: DeletedArtifact = remote.delete(&path).await?;
    Ok(())
}

#[cfg(test)]
mod tests;
