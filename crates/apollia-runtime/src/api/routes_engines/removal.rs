//! The removal of one installed engine build, to win back its place.
//!
//! The build is deleted the way the automatic purge deletes one
//! ([`purge::remove_build`]). Two builds are never removed: the one in
//! service, which a process runs now or the next launch takes under the
//! settings, and the engine embedded in the bundle, which lives outside the
//! store. A variant chosen in the settings and since removed by other means
//! falls back to the automatic choice at the next launch, flagged.

use std::path::{Path, PathBuf};

use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::api::routes_engines::{engines_in_service_at, entry, mark_in_service};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::engine_store::{
    self, installed_packages, purge, Engine, EngineChoices, InstalledPackage, Source, Variant,
};

/// The tag that names the engine embedded in the bundle, which has none of
/// its own. A removal addressed to it is refused.
pub const EMBEDDED_TAG: &str = "embedded";

/// A refused removal: the message and its stable code.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EngineRemovalErrorBody {
    /// What went wrong, for a person.
    pub error: String,
    /// Stable code: `not_found`, `in_service`, `embedded`, `store`.
    pub code: String,
}

type RemovalError = (StatusCode, Json<EngineRemovalErrorBody>);

fn refused(status: StatusCode, code: &str, error: impl std::fmt::Display) -> RemovalError {
    let error = error.to_string();
    tracing::event!(
        tracing::Level::WARN,
        code,
        error = %error,
        "engine.remove.refused"
    );
    (
        status,
        Json(EngineRemovalErrorBody {
            error,
            code: code.to_owned(),
        }),
    )
}

/// Whether `package` holds one of the `running` binaries, the binary the
/// next launch takes, or the MLX environment it imports.
pub(super) fn is_in_service(
    package: &InstalledPackage,
    running: &[PathBuf],
    next_llama: Option<&Path>,
    next_mlx: Option<&Path>,
) -> bool {
    let candidates = running.iter().map(PathBuf::as_path).chain(next_llama);
    let mut entries = [entry(package.clone())];
    for binary in candidates {
        mark_in_service(&mut entries, Some(binary), None);
        if entries[0].in_service {
            return true;
        }
    }
    mark_in_service(&mut entries, None, next_mlx);
    entries[0].in_service
}

#[utoipa::path(
    delete,
    path = "/api/v1/engines/{engine}/{tag}/{variant}",
    tag = "engines",
    params(
        ("engine" = String, Path, description = "Engine, as the list names it: `llama-server`, `vllm-mlx`, `nodejs`"),
        ("tag" = String, Path, description = "Upstream tag of the build; `embedded` names the engine shipped with the application"),
        ("variant" = String, Path, description = "Variant, as the list names it: `cuda-12`, `vulkan`, `cpu`..."),
    ),
    responses(
        (status = 204, description = "The build and its directory are gone"),
        (status = 404, description = "No such build installed (`not_found`)", body = EngineRemovalErrorBody),
        (status = 409, description = "The build in service (`in_service`) or the embedded engine (`embedded`)", body = EngineRemovalErrorBody),
        (status = 500, description = "The build could not be deleted (`store`)", body = EngineRemovalErrorBody),
    )
)]
pub async fn remove_engine<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    UrlPath((engine, tag, variant)): UrlPath<(String, String, String)>,
) -> Result<StatusCode, RemovalError> {
    if tag == EMBEDDED_TAG {
        return Err(refused(
            StatusCode::CONFLICT,
            "embedded",
            "the engine embedded in the application cannot be removed",
        ));
    }
    let not_found = || {
        refused(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("no build {engine} {tag} {variant} is installed"),
        )
    };
    let (Some(wanted_engine), Some(wanted_variant)) =
        (Engine::from_wire(&engine), Variant::from_wire(&variant))
    else {
        return Err(not_found());
    };
    let root = engine_store::store_root(&state.data_dir);
    let (lookup_root, wanted_tag) = (root.clone(), tag.clone());
    let package = tokio::task::spawn_blocking(move || {
        installed_packages(&lookup_root).into_iter().find(|p| {
            p.engine == wanted_engine && p.variant == wanted_variant && p.tag == wanted_tag
        })
    })
    .await
    .ok()
    .flatten()
    .ok_or_else(not_found)?;
    if package.source == Source::Embedded {
        return Err(refused(
            StatusCode::CONFLICT,
            "embedded",
            "the engine embedded in the application cannot be removed",
        ));
    }

    let running = match &state.llama_server_supervisor {
        Some(supervisor) => supervisor.running_binaries().await,
        None => Vec::new(),
    };
    let config_file = state.config_path.clone();
    let imposed = state.inference_governance.imposed_settings();
    let service_root = root.clone();
    let (next_llama, next_mlx) = tokio::task::spawn_blocking(move || {
        engines_in_service_at(
            &service_root,
            EngineChoices::read(config_file.as_deref(), &imposed),
        )
    })
    .await
    .unwrap_or((None, None));
    if is_in_service(
        &package,
        &running,
        next_llama.as_deref(),
        next_mlx.as_deref(),
    ) {
        return Err(refused(
            StatusCode::CONFLICT,
            "in_service",
            format!(
                "{} {} {} is the build in service; choose another engine first",
                package.engine.as_str(),
                package.tag,
                package.variant.as_str()
            ),
        ));
    }

    let removed = package.clone();
    tokio::task::spawn_blocking(move || purge::remove_build(&removed))
        .await
        .map_err(|e| refused(StatusCode::INTERNAL_SERVER_ERROR, "store", e))?
        .map_err(|e| refused(StatusCode::INTERNAL_SERVER_ERROR, "store", e))?;
    tracing::event!(
        tracing::Level::INFO,
        engine = package.engine.as_str(),
        tag = %package.tag,
        variant = package.variant.as_str(),
        "engine.removed"
    );
    Ok(StatusCode::NO_CONTENT)
}
