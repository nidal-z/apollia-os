//! Tauri IPC command for the binary plan choice (binary feedback).
//!
//! `choose_plan` receives the operator's choice from `PlanAlternativesView.svelte`,
//! emits `RuntimeEvent::PlanChosen` on the EventBus and returns a confirmation.

use apollia_core::plan_alternatives::{ChosenPlan, PlanChoice};
use apollia_core::RuntimeEvent;
use apollia_runtime::embedded::RuntimeHandle;
use tauri::State;

/// Records the operator's plan choice and emits the event on the EventBus.
///
/// Receives the `session_id` and chosen plan from the frontend, builds a
/// [`PlanChoice`] with the current timestamp, then broadcasts
/// `RuntimeEvent::PlanChosen` over the EventBus.
///
/// Returns `Ok(())` if the event was broadcast, `Err(String)` on an invalid
/// `chosen` value.
#[tauri::command]
pub async fn choose_plan(
    state: State<'_, RuntimeHandle>,
    session_id: String,
    chosen: String,
) -> Result<(), String> {
    let chosen_plan = match chosen.as_str() {
        "plan_a" => ChosenPlan::PlanA,
        "plan_b" => ChosenPlan::PlanB,
        other => return Err(format!("valeur 'chosen' invalide : '{other}'")),
    };

    let chosen_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let choice = PlanChoice {
        session_id: session_id.clone(),
        chosen: chosen_plan,
        chosen_at,
    };

    let _ = state.event_sender.send(RuntimeEvent::PlanChosen { choice });

    tracing::info!(
        session_id = %session_id,
        chosen = %chosen,
        "plan choice received from desktop"
    );

    Ok(())
}
