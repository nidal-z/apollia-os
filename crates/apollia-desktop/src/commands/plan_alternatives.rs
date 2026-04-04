//! Commande IPC Tauri pour le choix de plan binaire (binary feedback).
//!
//! `choose_plan` reçoit le choix de l'opérateur depuis `PlanAlternativesView.svelte`,
//! émet `RuntimeEvent::PlanChosen` sur l'EventBus et retourne une confirmation.

use apollia_core::plan_alternatives::{ChosenPlan, PlanChoice};
use apollia_core::RuntimeEvent;
use apollia_runtime::embedded::RuntimeHandle;
use tauri::State;

/// Enregistre le choix de plan de l'opérateur et émet l'événement sur l'EventBus.
///
/// Reçoit le `session_id` et le plan choisi depuis le frontend, construit un
/// [`PlanChoice`] avec le timestamp courant, puis diffuse
/// `RuntimeEvent::PlanChosen` via l'EventBus.
///
/// Retourne `Ok(())` si l'événement a été diffusé, `Err(String)` en cas de
/// valeur `chosen` invalide.
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
