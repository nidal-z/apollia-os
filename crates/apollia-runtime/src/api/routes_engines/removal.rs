//! The removal of one installed engine build, to win back its place.
//!
//! The build is deleted the way the automatic purge deletes one
//! ([`purge::remove_build`]). Two builds are never removed: the one in
//! service, which a process runs now or the automatic choice takes at the
//! next launch, and the engine embedded in the bundle, which lives outside
//! the store.
//!
//! The build `engines.gguf` names, which no process runs, goes on
//! confirmation only. When no other build of the chosen variant is left, the
//! setting is first written back to `auto`, through the one writer of the
//! file, so the next launch takes the automatic choice instead of falling
//! back to it.

use std::path::{Path, PathBuf};

use apollia_core::config::writer::{update_config, ConfigEdit, ConfigWriteError, KeyChange};
use apollia_core::config::EngineChoice;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::routes_engines::{engines_in_service_at, entry, mark_in_service};
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::engine_store::selection::{named_variants, resolve, GGUF_ENGINE_SETTING};
use crate::engine_store::{
    self, compare_tags, installed_packages, purge, Engine, EngineChoices, InstalledPackage,
    Resolved, Source, Variant,
};

/// The tag that names the engine embedded in the bundle, which has none of
/// its own: `GET /api/v1/engines` lists it with no tag, and a removal
/// addresses it by this reserved word, to be refused.
pub const EMBEDDED_TAG: &str = "embedded";

/// A refused removal: the message and its stable code.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EngineRemovalErrorBody {
    /// What went wrong, for a person.
    pub error: String,
    /// Stable code: `not_found`, `in_service`, `embedded`, `chosen`,
    /// `locked`, `config`, `store`.
    pub code: String,
    /// With `chosen`, what `engines.gguf` comes down to once the build is
    /// gone: `auto`, or the tag of the build of the chosen variant that
    /// takes its place.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
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
            after: None,
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

/// The query of a removal.
#[derive(Debug, Default, Deserialize)]
pub struct RemovalQuery {
    /// Removes the build `engines.gguf` names, which is refused without it.
    #[serde(default)]
    pub confirm: bool,
}

/// What `engines.gguf` says of the build to remove.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Chosen {
    /// The setting does not name this build.
    No,
    /// The setting names this build, and the build `tag` of the chosen
    /// variant `variant` takes its place once it is gone.
    Replaced { tag: String, variant: Variant },
    /// The setting names this build and nothing would be left to take its
    /// place: the setting goes back to `auto`.
    Last,
}

impl Chosen {
    /// What the setting comes down to once the build is gone: `auto`, the
    /// tag that takes its place, or nothing when the setting does not name
    /// the build.
    pub(super) fn after(&self) -> Option<String> {
        match self {
            Self::No => None,
            Self::Replaced { tag, .. } => Some(tag.clone()),
            Self::Last => Some(EngineChoice::Auto.as_str().to_owned()),
        }
    }
}

/// The fate of a removal request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Verdict {
    /// Delete the build; [`Chosen::Last`] writes `engines.gguf` back to
    /// `auto` first.
    Remove(Chosen),
    /// A process runs the build, or the automatic choice takes it next.
    InService,
    /// The build `engines.gguf` names, asked for without confirmation.
    Chosen(Chosen),
}

/// Weighs a removal of `package` against the `running` binaries and the
/// `next` launch (its `llama-server` binary and its MLX packages). A process
/// running it refuses it whatever the confirmation; the build the setting
/// names goes with `confirm` only; the build the automatic choice takes next
/// is refused.
pub(super) fn weigh(
    package: &InstalledPackage,
    running: &[PathBuf],
    next: (Option<&Path>, Option<&Path>),
    chosen: Chosen,
    confirm: bool,
) -> Verdict {
    let (next_llama, next_mlx) = next;
    if is_in_service(package, running, None, None) {
        return Verdict::InService;
    }
    match chosen {
        Chosen::No if is_in_service(package, &[], next_llama, next_mlx) => Verdict::InService,
        Chosen::No => Verdict::Remove(Chosen::No),
        _ if !confirm => Verdict::Chosen(chosen),
        named => Verdict::Remove(named),
    }
}

/// What `choice`, the setting of the GGUF engine, says of `package` in the
/// store under `root`.
pub(super) fn chosen_in(root: &Path, choice: EngineChoice, package: &InstalledPackage) -> Chosen {
    if package.engine != Engine::LlamaServer {
        return Chosen::No;
    }
    let same = |p: &InstalledPackage| p.tag == package.tag && p.variant == package.variant;
    match resolve(Some(root), Engine::LlamaServer, choice).resolved {
        Resolved::Stored(named) if same(&named) => {}
        _ => return Chosen::No,
    }
    let others: Vec<InstalledPackage> = installed_packages(root)
        .into_iter()
        .filter(|p| p.engine == Engine::LlamaServer && !same(p))
        .collect();
    let replacement = named_variants(choice).iter().find_map(|variant| {
        others
            .iter()
            .filter(|p| p.variant == *variant)
            .max_by(|a, b| compare_tags(&a.tag, &b.tag))
    });
    match replacement {
        Some(next) => Chosen::Replaced {
            tag: next.tag.clone(),
            variant: next.variant,
        },
        None => Chosen::Last,
    }
}

/// The refusal of a build `engines.gguf` names, asked for without
/// confirmation: the message says what the setting comes down to, and
/// `after` carries it for a client.
pub(super) fn chosen_refusal(package: &InstalledPackage, chosen: &Chosen) -> RemovalError {
    let after = match chosen {
        Chosen::Replaced { tag, variant } => format!(
            "the build {tag} {} of the chosen variant takes its place",
            variant.as_str()
        ),
        Chosen::No | Chosen::Last => "the choice goes back to auto".to_owned(),
    };
    let (status, Json(mut body)) = refused(
        StatusCode::CONFLICT,
        "chosen",
        format!(
            "{} is the build engines.gguf chooses; remove it with confirm=true and {after}",
            name(package)
        ),
    );
    body.after = chosen.after();
    (status, Json(body))
}

/// The refusal of a deletion that failed; when `engines.gguf` was written
/// back to `auto` before it, the message says so.
pub(super) fn store_failure(reset: bool, error: impl std::fmt::Display) -> RemovalError {
    let error = if reset {
        format!("{error}; engines.gguf was set back to auto and the build is still installed")
    } else {
        error.to_string()
    };
    refused(StatusCode::INTERNAL_SERVER_ERROR, "store", error)
}

fn name(package: &InstalledPackage) -> String {
    format!(
        "{} {} {}",
        package.engine.as_str(),
        package.tag,
        package.variant.as_str()
    )
}

/// Writes `engines.gguf` back to `auto` through the one writer of the file,
/// which holds the key to its class, its value and the organisation policy,
/// then shows the reloader the file it wrote.
async fn reset_choice<B: ExecutionBackend + Clone>(
    state: &AppState<B>,
    package: &InstalledPackage,
) -> Result<(), RemovalError> {
    let Some(path) = state.config_path.clone() else {
        return Err(refused(
            StatusCode::INTERNAL_SERVER_ERROR,
            "config",
            "this runtime was started without a configuration file",
        ));
    };
    let policy = state.inference_governance.imposed_settings();
    let change = KeyChange::set(
        GGUF_ENGINE_SETTING,
        serde_json::json!(EngineChoice::Auto.as_str()),
    );
    let written = tokio::task::spawn_blocking(move || {
        update_config(&path, ConfigEdit::Keys(vec![change]), &policy)
    })
    .await
    .map_err(|e| refused(StatusCode::INTERNAL_SERVER_ERROR, "config", e))?
    .map_err(|e| match e {
        ConfigWriteError::LockedByPolicy { .. } => refused(
            StatusCode::FORBIDDEN,
            "locked",
            format!(
                "{} is the build the organisation policy chooses: {e}",
                name(package)
            ),
        ),
        other => refused(StatusCode::INTERNAL_SERVER_ERROR, "config", other),
    })?;
    if let Some(reload) = &state.config_reload {
        reload
            .saved(written.text, crate::config_reload::ConfigOrigin::Api)
            .await;
    }
    Ok(())
}

/// `DELETE /api/v1/engines/{engine}/{tag}/{variant}`: removes one installed
/// build of the engine store.
///
/// The embedded engine has no tag of its own: `GET /api/v1/engines` lists it
/// with `tag` null, and this route addresses it by the reserved tag
/// `embedded`, always to refuse it (`embedded`), confirmation or not. An
/// engine or a variant this build does not know answers `not_found` first.
///
/// The build `engines.gguf` names, which no process runs, is refused without
/// `confirm=true` (`chosen`), and `after` says what the setting comes down to
/// once it is gone: `auto`, or the tag of the build of the chosen variant
/// that takes its place. With it the build goes; when no other build of the
/// chosen variant is left, `engines.gguf` is first written back to `auto`,
/// and the `engine.removed` event says so (`choice_reset`, `after`). A build
/// a process runs stays refused (`in_service`) whatever the confirmation.
#[utoipa::path(
    delete,
    path = "/api/v1/engines/{engine}/{tag}/{variant}",
    tag = "engines",
    params(
        ("engine" = String, Path, description = "Engine, as the list names it: `llama-server`, `vllm-mlx`, `nodejs`"),
        ("tag" = String, Path, description = "Upstream tag of the build; the reserved tag `embedded` names the engine shipped with the application, which is never removed"),
        ("variant" = String, Path, description = "Variant, as the list names it: `cuda-12`, `vulkan`, `cpu`..."),
        ("confirm" = Option<bool>, Query, description = "Remove the build `engines.gguf` names; `false` when absent"),
    ),
    responses(
        (status = 204, description = "The build and its directory are gone"),
        (status = 403, description = "The organisation policy fixes `engines.gguf` to this build (`locked`)", body = EngineRemovalErrorBody),
        (status = 404, description = "No such build installed (`not_found`)", body = EngineRemovalErrorBody),
        (status = 409, description = "The build in service (`in_service`), the embedded engine (`embedded`), or the build `engines.gguf` names without `confirm=true` (`chosen`)", body = EngineRemovalErrorBody),
        (status = 500, description = "The build could not be deleted (`store`) or the setting written back (`config`)", body = EngineRemovalErrorBody),
    )
)]
pub async fn remove_engine<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    UrlPath((engine, tag, variant)): UrlPath<(String, String, String)>,
    Query(query): Query<RemovalQuery>,
) -> Result<StatusCode, RemovalError> {
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
    let embedded = || {
        refused(
            StatusCode::CONFLICT,
            "embedded",
            "the engine embedded in the application cannot be removed",
        )
    };
    if tag == EMBEDDED_TAG {
        return Err(embedded());
    }
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
        return Err(embedded());
    }

    let running = match &state.llama_server_supervisor {
        Some(supervisor) => supervisor.running_binaries().await,
        None => Vec::new(),
    };
    let config_file = state.config_path.clone();
    let imposed = state.inference_governance.imposed_settings();
    let (service_root, weighed) = (root.clone(), package.clone());
    let (next_llama, next_mlx, chosen) = tokio::task::spawn_blocking(move || {
        let choices = EngineChoices::read(config_file.as_deref(), &imposed);
        let chosen = chosen_in(&service_root, choices.gguf, &weighed);
        let (llama, mlx) = engines_in_service_at(&service_root, choices);
        (llama, mlx, chosen)
    })
    .await
    .unwrap_or((None, None, Chosen::No));
    let verdict = weigh(
        &package,
        &running,
        (next_llama.as_deref(), next_mlx.as_deref()),
        chosen,
        query.confirm,
    );
    let chosen = match verdict {
        Verdict::Remove(chosen) => chosen,
        Verdict::InService => {
            return Err(refused(
                StatusCode::CONFLICT,
                "in_service",
                format!(
                    "{} is the build in service; choose another engine first",
                    name(&package)
                ),
            ))
        }
        Verdict::Chosen(chosen) => return Err(chosen_refusal(&package, &chosen)),
    };
    let reset = chosen == Chosen::Last;
    if reset {
        reset_choice(&state, &package).await?;
    }

    let removed = package.clone();
    tokio::task::spawn_blocking(move || purge::remove_build(&removed))
        .await
        .map_err(|e| store_failure(reset, e))?
        .map_err(|e| store_failure(reset, e))?;
    let after = chosen.after();
    tracing::event!(
        tracing::Level::INFO,
        engine = package.engine.as_str(),
        tag = %package.tag,
        variant = package.variant.as_str(),
        choice_reset = reset,
        after = after.as_deref(),
        "engine.removed"
    );
    Ok(StatusCode::NO_CONTENT)
}
