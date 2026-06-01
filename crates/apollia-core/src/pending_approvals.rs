//! Registre thread-safe des approbations HITL en attente.
//!
//! Défini dans `apollia-core` pour être partagé sans dépendance circulaire
//! entre `apollia-oria` (qui enregistre) et `apollia-runtime` (qui résout).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use crate::result::InputResponseData;

// ─────────────────────────────────────────────
// PendingApprovalError
// ─────────────────────────────────────────────

/// Erreurs du registre d'approbations en attente.
#[derive(Debug, thiserror::Error)]
pub enum PendingApprovalError {
    /// Aucune approbation en attente pour la tâche donnée.
    #[error("aucune approbation en attente pour la tâche : {0}")]
    NotFound(String),
}

// ─────────────────────────────────────────────
// PendingApprovals
// ─────────────────────────────────────────────

/// Registre thread-safe des tâches en attente d'approbation humaine (HITL).
///
/// Chaque tâche suspendue possède un [`oneshot::Sender`] enregistré par
/// [`ORIAEngine::execute_direct`]. Le `ResumeHandler`
/// appelle [`resolve`] pour débloquer l'attente et transmettre la décision humaine.
///
/// Cloneable via `Arc` - partagé entre `ORIAEngine` et les routes REST via `AppState`.
///
/// [`ORIAEngine::execute_direct`]: apollia_oria::engine::ORIAEngine::execute_direct
/// [`resolve`]: PendingApprovals::resolve
#[derive(Debug, Clone)]
pub struct PendingApprovals {
    // Exception au principe #5 (un acteur, une responsabilité) :
    // PendingApprovals est partagé entre ORIAEngine (qui enregistre le oneshot lors
    // d'une suspension HITL) et les routes REST (qui résolvent l'approbation depuis
    // un thread HTTP distinct). Un refactoring vers un acteur dédié introduirait une
    // latence inacceptable sur le chemin critique d'approbation humaine.
    inner: Arc<Mutex<HashMap<String, oneshot::Sender<InputResponseData>>>>,
}

impl PendingApprovals {
    /// Crée un nouveau registre vide.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Crée un oneshot channel pour `task_id`, stocke l'émetteur, retourne le récepteur.
    ///
    /// L'appelant (typiquement `ORIAEngine::execute_direct`) fait `rx.await` pour se
    /// bloquer jusqu'à ce que [`resolve`] soit appelé par le `ResumeHandler`.
    ///
    /// Si une entrée existait déjà pour ce `task_id`, elle est remplacée - l'ancien
    /// sender est dropped, le receiver correspondant recevra une erreur de canal fermé.
    ///
    /// [`resolve`]: PendingApprovals::resolve
    pub fn register(&self, task_id: &str) -> oneshot::Receiver<InputResponseData> {
        let (tx, rx) = oneshot::channel();
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.insert(task_id.to_string(), tx);
        rx
    }

    /// Retourne les identifiants des tâches actuellement en attente d'approbation.
    ///
    /// Utilisé par les commandes IPC desktop pour lister les approbations en attente.
    pub fn pending_task_ids(&self) -> Vec<String> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.keys().cloned().collect()
    }

    /// Résout l'approbation en attente pour `task_id` - appelé par le `ResumeHandler`.
    ///
    /// Retire l'entrée du registre et envoie `response` sur le oneshot channel.
    /// Si le receiver côté ORIA a déjà été droppé (ex: shutdown du runtime),
    /// l'erreur d'envoi est ignorée silencieusement.
    ///
    /// Retourne [`PendingApprovalError::NotFound`] si aucune approbation n'est enregistrée
    /// pour `task_id` - typiquement si la tâche n'est pas en status `input_required`.
    pub fn resolve(
        &self,
        task_id: &str,
        response: InputResponseData,
    ) -> Result<(), PendingApprovalError> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        match guard.remove(task_id) {
            Some(tx) => {
                // Ignore send error - receiver may have been dropped on runtime shutdown.
                let _ = tx.send(response);
                Ok(())
            }
            None => Err(PendingApprovalError::NotFound(task_id.to_string())),
        }
    }
}

impl Default for PendingApprovals {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn make_response(approved: bool) -> InputResponseData {
        InputResponseData {
            approved,
            reason: None,
            context: Value::Null,
            responded_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    // deux tâches simultanées isolées
    #[tokio::test]
    async fn test_ac5_two_tasks_isolated() {
        // GIVEN deux registrations pour t-0001 et t-0002
        let pending = PendingApprovals::new();
        let rx1 = pending.register("t-0001");
        let mut rx2 = pending.register("t-0002");

        // WHEN resolve(t-0001, approved)
        pending
            .resolve("t-0001", make_response(true))
            .expect("resolve t-0001 failed");

        // THEN t-0001 reçoit la réponse
        let resp1 = rx1.await.expect("rx1 should receive");
        assert!(resp1.approved);

        // THEN t-0002 toujours en attente (receiver non consommé)
        assert!(rx2.try_recv().is_err(), "t-0002 should still be pending");
    }

    #[tokio::test]
    async fn test_resolve_not_found_returns_error() {
        // GIVEN un registre vide
        let pending = PendingApprovals::new();

        // WHEN resolve sur une tâche inexistante
        let result = pending.resolve("t-9999", make_response(true));

        // THEN NotFound
        assert!(
            matches!(result, Err(PendingApprovalError::NotFound(_))),
            "expected NotFound error"
        );
    }

    #[tokio::test]
    async fn test_register_and_resolve_approved() {
        // GIVEN un registre avec t-0001
        let pending = PendingApprovals::new();
        let rx = pending.register("t-0001");

        // WHEN resolve avec approved=true
        pending
            .resolve("t-0001", make_response(true))
            .expect("resolve failed");

        // THEN rx reçoit la réponse avec approved=true
        let resp = rx.await.expect("rx should receive");
        assert!(resp.approved);
    }

    #[tokio::test]
    async fn test_register_and_resolve_rejected_with_reason() {
        // GIVEN un registre avec t-0002
        let pending = PendingApprovals::new();
        let rx = pending.register("t-0002");

        // WHEN resolve avec approved=false et une raison
        let mut resp_data = make_response(false);
        resp_data.reason = Some("Budget insuffisant".into());
        pending
            .resolve("t-0002", resp_data)
            .expect("resolve failed");

        // THEN rx reçoit la réponse avec approved=false et la raison transmise
        let resp = rx.await.expect("rx should receive");
        assert!(!resp.approved);
        assert_eq!(resp.reason.as_deref(), Some("Budget insuffisant"));
    }
}
