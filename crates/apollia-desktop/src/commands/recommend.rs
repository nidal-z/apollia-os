//! The Tauri surface for model recommendations.
//!
//! One command, because the decision is one decision: given this machine, which
//! model should it run. Everything behind it lives in `apollia_llm::recommend`,
//! and this file is the boundary that turns a hardware probe plus the Hub into
//! a serialisable answer.
//!
//! The error shape matters more than usual here. An empty list and an
//! unreachable Hub look the same to an interface that only receives a list, and
//! they call for opposite responses: the first means "your machine is too small
//! for anything we curate", the second means "we could not look". So the
//! outcome is an enum, and the onboarding step renders a different branch for
//! each.

use apollia_llm::hardware::detect as detect_hardware;
use apollia_llm::recommend::{
    resolve, FamilyManifest, Recommendation, ResolveError, ResolveOptions, RuntimeShape,
};
use serde::{Deserialize, Serialize};
use tracing::{event, Level};

use super::model_hub::HardwareProfileView;

/// What the caller may vary. Everything is optional: onboarding sends nothing.
#[derive(Debug, Default, Deserialize)]
pub struct RecommendParams {
    /// Token for gated repositories, when the operator has configured one.
    pub hf_token: Option<String>,
    /// How many recommendations to return. Clamped to a sane ceiling.
    pub limit: Option<usize>,
    /// Context window to size the key/value cache against.
    ///
    /// Defaults to what the runtime actually launches with. An operator who has
    /// lowered it through `APOLLIA_LLAMA_N_CTX` can pass the real value and see
    /// the larger models that then fit.
    pub n_ctx: Option<u32>,
}

/// The answer, with the two failure modes kept apart from the success.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RecommendOutcome {
    /// Models were found, best first.
    Ok {
        /// The ranked list.
        models: Vec<Recommendation>,
        /// The machine the ranking was computed for, so the interface can
        /// explain the choice without probing the hardware a second time.
        hardware: HardwareProfileView,
    },

    /// The Hub could not be reached, so there is no catalogue to rank.
    ///
    /// Distinct from an empty list on purpose: the operator should be offered
    /// the paths that need no network rather than told their machine runs
    /// nothing.
    Unreachable {
        /// What failed, for the details disclosure.
        detail: String,
        /// The hardware probe, which succeeds offline and is still worth showing.
        hardware: HardwareProfileView,
    },

    /// The Hub answered and nothing it carries fits this machine.
    Empty {
        /// The hardware probe, so the interface can say what it measured.
        hardware: HardwareProfileView,
    },
}

/// Most recommendations anyone needs to see at once.
const MAX_LIMIT: usize = 10;

/// How long a ranked list is reused before the Hub is asked again.
///
/// Onboarding remounts the step whenever the operator goes back and forth, and
/// each mount used to pay for a whole resolution. The catalogue does not move
/// on that scale, and the hardware does not move at all.
const CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(600);

/// The last successful answer, keyed by the context window it was sized for.
///
/// Only a ranked list is kept. An unreachable Hub is exactly the answer worth
/// asking again, so it is never cached.
static LAST_ANSWER: std::sync::Mutex<Option<(std::time::Instant, u32, RecommendOutcome)>> =
    std::sync::Mutex::new(None);

fn cached(n_ctx: u32) -> Option<RecommendOutcome> {
    let guard = LAST_ANSWER.lock().ok()?;
    let (at, key, outcome) = guard.as_ref()?;
    (*key == n_ctx && at.elapsed() < CACHE_TTL).then(|| outcome.clone())
}

fn remember(n_ctx: u32, outcome: &RecommendOutcome) {
    if !matches!(outcome, RecommendOutcome::Ok { .. }) {
        return;
    }
    if let Ok(mut guard) = LAST_ANSWER.lock() {
        *guard = Some((std::time::Instant::now(), n_ctx, outcome.clone()));
    }
}

/// Rank the models this machine should be offered.
///
/// Probes the hardware, plans against the embedded generation table, resolves
/// the survivors on the Hub, reads each finalist's GGUF header over a range
/// request, and ranks what is left.
///
/// # Errors
/// Returns `Err` only when the hardware probe itself fails, which is a broken
/// machine rather than a condition the interface handles. Everything else,
/// including an unreachable Hub, comes back as a [`RecommendOutcome`] variant.
#[tauri::command]
pub async fn recommend_models(params: RecommendParams) -> Result<RecommendOutcome, String> {
    let cache_key = params.n_ctx.unwrap_or(0);
    if let Some(outcome) = cached(cache_key) {
        return Ok(truncated(outcome, params.limit));
    }

    let profile = tokio::task::spawn_blocking(detect_hardware)
        .await
        .map_err(|e| format!("hardware detection failed: {e}"))?;
    let hardware = HardwareProfileView::from(profile.clone());

    let manifest = match FamilyManifest::embedded() {
        Ok(manifest) => manifest,
        Err(err) => {
            // The table ships inside this binary, so a failure here is a defect
            // in the build rather than anything the operator did.
            event!(Level::ERROR, error = %err, "recommend.manifest.invalid");
            return Err(format!("the shipped model table is invalid: {err}"));
        }
    };

    let mut options = ResolveOptions {
        hf_token: params.hf_token,
        ..ResolveOptions::default()
    };
    if let Some(n_ctx) = params.n_ctx {
        options.shape = RuntimeShape {
            n_ctx,
            ..RuntimeShape::default()
        };
    }

    match resolve(&manifest, &profile, &options).await {
        Ok(models) if models.is_empty() => Ok(RecommendOutcome::Empty { hardware }),
        Ok(models) => {
            let outcome = RecommendOutcome::Ok { models, hardware };
            remember(cache_key, &outcome);
            Ok(truncated(outcome, params.limit))
        }
        Err(ResolveError::HubUnreachable(detail)) => {
            event!(Level::WARN, detail = %detail, "recommend.hub.unreachable");
            Ok(RecommendOutcome::Unreachable { detail, hardware })
        }
        Err(err) => Err(err.to_string()),
    }
}

/// Cut a ranked list to what the caller asked for.
fn truncated(outcome: RecommendOutcome, limit: Option<usize>) -> RecommendOutcome {
    match outcome {
        RecommendOutcome::Ok {
            mut models,
            hardware,
        } => {
            models.truncate(limit.unwrap_or(MAX_LIMIT).clamp(1, MAX_LIMIT));
            RecommendOutcome::Ok { models, hardware }
        }
        other => other,
    }
}
