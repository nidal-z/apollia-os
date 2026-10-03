//! The Tauri surface for model recommendations.
//!
//! One command, because the decision is one decision: given this machine, which
//! model should it run. The decision lives in `apollia_runtime::recommendation`,
//! which the daemon serves as `GET /api/v1/llm/recommendations`: embedded, the
//! command calls it in this process; attached, it asks the daemon, so both
//! modes give the same outcome (ranked, unreachable Hub, or empty, each with
//! the hardware and the editorial selection).

use tauri::State;

use crate::attached::RuntimeLink;

pub use apollia_runtime::recommendation::RecommendParams;

/// Rank the models this machine should be offered.
///
/// # Errors
/// Returns `Err` only when the hardware probe or the shipped table fails, or
/// when the daemon refuses, in its own words. An unreachable Hub comes back as
/// the `unreachable` outcome.
#[tauri::command]
pub async fn recommend_models(
    link: State<'_, RuntimeLink>,
    params: RecommendParams,
) -> Result<serde_json::Value, String> {
    super::model_hub::recommend_on(&link, params).await
}
