//! The example packages the binary proposes.
//!
//! - `GET  /api/v1/packages/examples`                , the examples, each installed or not
//! - `POST /api/v1/packages/examples/{name}/preview` , what an example contains and asks for; nothing is written
//! - `POST /api/v1/packages/examples/{name}/install` , install the example the user chose
//!
//! An example is named by its package, never by a path: the runtime carries
//! its files, and only what it carries is treated as its own. The preview and
//! the installation are those of any package, read from the same governance,
//! with one difference: an example is not signed, and is neither flagged nor
//! refused for it. Both answer `503` without an audit journal.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use apollia_tools::apollia_manifest::ApolliaManifest;
use apollia_tools::PackageRepository;

use super::{
    agents_db, api_err, connections_of, gather_for, install_archive, internal, package_error,
    ApiError, PackageErrorBody, API_ACTOR,
};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::packages::{
    find_example, list_examples, preview_example, proposed_examples, ExamplePackage, ExampleSource,
    InstallOptions, InstallReport, PackageOrigin, PackagePreview,
};

/// Body of `POST /api/v1/packages/examples/{name}/install`.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExampleInstallRequest {
    /// The user accepted the Python dependencies the package installs.
    #[serde(default)]
    pub deps_confirmed: bool,
    /// The user accepted what an update adds.
    #[serde(default)]
    pub changes_confirmed: bool,
}

/// The example named `name`, or the `404` that says the binary proposes none
/// of that name.
fn example_named(name: &str) -> Result<(&'static ExampleSource, ApolliaManifest), ApiError> {
    find_example(proposed_examples(), name).ok_or_else(|| {
        api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("no example package named {name}"),
        )
    })
}

/// `GET /api/v1/packages/examples`: the example packages the binary carries,
/// each with its agents and their mission, the connections it requires
/// (present or missing on this machine) and the installed version when the
/// user installed it. None of them is installed until the user chooses it.
#[utoipa::path(
    get,
    path = "/api/v1/packages/examples",
    tag = "packages",
    responses(
        (status = 200, description = "The examples", body = [ExamplePackage]),
        (status = 500, description = "A store cannot be read", body = PackageErrorBody),
    )
)]
pub(crate) async fn list_example_packages<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<ExamplePackage>>, ApiError> {
    let examples = proposed_examples();
    if examples.is_empty() {
        return Ok(Json(Vec::new()));
    }
    let requires_connections = examples
        .iter()
        .filter_map(|example| example.manifest().ok())
        .any(|manifest| !manifest.requires.is_empty());
    let connections = connections_of(&state, requires_connections).await?;
    let db = agents_db(&state);
    let listed = tokio::task::spawn_blocking(move || {
        let packages = PackageRepository::open(&db)?;
        list_examples(examples, &packages, &connections)
    })
    .await
    .map_err(internal)?;
    listed.map(Json).map_err(package_error)
}

/// `POST /api/v1/packages/examples/{name}/preview`: what the example
/// contains, what it asks for and which connections it requires, as the
/// preview of a package file says it. Nothing is written.
#[utoipa::path(
    post,
    path = "/api/v1/packages/examples/{name}/preview",
    tag = "packages",
    params(("name" = String, Path, description = "Package name of the example")),
    responses(
        (status = 200, description = "The preview", body = PackagePreview),
        (status = 404, description = "No example of that name", body = PackageErrorBody),
        (status = 503, description = "No audit journal", body = PackageErrorBody),
    )
)]
pub(crate) async fn preview_example_package<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<PackagePreview>, ApiError> {
    let (_, manifest) = example_named(&name)?;
    let stores = gather_for(&state, !manifest.requires.is_empty()).await?;
    let result = tokio::task::spawn_blocking(move || preview_example(&manifest, &stores.sources()))
        .await
        .map_err(internal)?;
    result.map(Json).map_err(package_error)
}

/// `POST /api/v1/packages/examples/{name}/install`: install the example.
///
/// The example is assembled into a package archive and installed like any
/// package file: its preview is computed again and its refusal applies, an
/// update that adds accesses or connections needs `changesConfirmed`, and the
/// installation is recorded in the audit journal. Once installed it is
/// uninstalled like any package.
#[utoipa::path(
    post,
    path = "/api/v1/packages/examples/{name}/install",
    tag = "packages",
    params(("name" = String, Path, description = "Package name of the example")),
    request_body = ExampleInstallRequest,
    responses(
        (status = 201, description = "Installed", body = InstallReport),
        (status = 404, description = "No example of that name", body = PackageErrorBody),
        (status = 409, description = "Older than the installed version, or agent owned elsewhere", body = PackageErrorBody),
        (status = 428, description = "A confirmation is missing", body = PackageErrorBody),
        (status = 500, description = "Store, environment or journal failure", body = PackageErrorBody),
        (status = 503, description = "No audit journal, or no MCP manager for the servers it brings", body = PackageErrorBody),
    )
)]
pub(crate) async fn install_example_package<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
    Json(request): Json<ExampleInstallRequest>,
) -> Result<(StatusCode, Json<InstallReport>), ApiError> {
    let (example, manifest) = example_named(&name)?;
    let stores = gather_for(&state, !manifest.requires.is_empty()).await?;
    let data_dir = state.data_dir.clone();
    let archive = tokio::task::spawn_blocking(move || example.assemble(&data_dir))
        .await
        .map_err(internal)?
        .map_err(package_error)?;
    let options = InstallOptions {
        deps_confirmed: request.deps_confirmed,
        changes_confirmed: request.changes_confirmed,
        allow_downgrade: false,
        actor: API_ACTOR.to_owned(),
        origin: PackageOrigin::Example,
    };
    let installed = install_archive(&state, stores, archive.clone(), options).await;
    // The archive was only the vehicle of this installation.
    if let Err(error) = std::fs::remove_file(&archive) {
        tracing::event!(
            tracing::Level::WARN,
            path = %archive.display(),
            error = %error,
            "packages.example.archive_cleanup_failed"
        );
    }
    let report = installed?;
    tracing::event!(
        tracing::Level::INFO,
        package = %report.name,
        version = %report.version,
        "packages.example.installed"
    );
    Ok((StatusCode::CREATED, Json(report)))
}

#[cfg(test)]
mod tests;
