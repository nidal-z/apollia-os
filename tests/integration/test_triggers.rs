//! Integration tests — TriggerEngine : interval, FileWatch, on_busy (STORY-078).
//!
//! Couvre les critères d'acceptation AC-1, AC-2, AC-5, AC-6.
//! Aucune dépendance Python. Exécutés dans tous les environnements CI.
//!
//! **AC-1** : IntervalTrigger 200 ms → ≥ 2 submit() en 700 ms
//! **AC-2** : FileWatch `create` → submit() dans les 5 s
//! **AC-5** : `on_busy=Drop` + agent occupé → TriggerSkipped émis, 0 submit
//! **AC-6** : `on_busy=Queue` + agent occupé → 1 submit quand même

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use apollia_core::{AIPInput, EventBusSender, RuntimeEvent, TaskId};
use apollia_triggers::{
    FileEventKind, InputTemplate, OnBusyPolicy, TaskSubmitter, TriggerDefinition,
    TriggerEngineHandle, TriggerSourceConfig,
};
use tempfile::TempDir;

// ─── MockTaskSubmitter ────────────────────────────────────────────────────

/// Mock implémentant [`TaskSubmitter`] pour compter les soumissions.
///
/// `pending_count_value` contrôle si l'agent est "occupé" aux yeux
/// du moteur (`> 0` = occupé).
struct MockTaskSubmitter {
    submit_count: Arc<AtomicU32>,
    pending_count_value: usize,
}

impl MockTaskSubmitter {
    /// Crée un mock avec `pending_count = 0` (agent libre).
    fn new() -> (Self, Arc<AtomicU32>) {
        let count = Arc::new(AtomicU32::new(0));
        (
            Self {
                submit_count: Arc::clone(&count),
                pending_count_value: 0,
            },
            count,
        )
    }

    /// Crée un mock avec `pending_count = 1` (agent occupé).
    fn new_busy() -> (Self, Arc<AtomicU32>) {
        let count = Arc::new(AtomicU32::new(0));
        (
            Self {
                submit_count: Arc::clone(&count),
                pending_count_value: 1,
            },
            count,
        )
    }
}

impl TaskSubmitter for MockTaskSubmitter {
    fn submit<'a>(
        &'a self,
        _agent: &'a str,
        _input: AIPInput,
    ) -> Pin<Box<dyn Future<Output = Result<TaskId, String>> + Send + 'a>> {
        self.submit_count.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(TaskId::new_v4()) })
    }

    fn pending_count<'a>(
        &'a self,
        _agent: &'a str,
    ) -> Pin<Box<dyn Future<Output = usize> + Send + 'a>> {
        let p = self.pending_count_value;
        Box::pin(async move { p })
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────

/// Crée un `EventBusSender` jetable (aucun abonné actif).
fn make_bus() -> EventBusSender {
    tokio::sync::broadcast::channel::<RuntimeEvent>(64).0
}

/// Construit une `TriggerDefinition` avec source `Interval`.
fn interval_def(id: &str, every: &str, on_busy: OnBusyPolicy) -> TriggerDefinition {
    TriggerDefinition {
        id: id.into(),
        agent: "test-agent".into(),
        enabled: true,
        on_busy,
        source: TriggerSourceConfig::Interval {
            every: every.into(),
        },
        input_template: InputTemplate("tick at {{fired_at}}".into()),
    }
}

// ─── AC-1 ─────────────────────────────────────────────────────────────────

/// AC-1 : IntervalTrigger 200 ms → au moins 2 submit() en 700 ms.
#[tokio::test]
async fn test_ac1_interval_trigger_submits_tasks() {
    // GIVEN — moteur avec un trigger à 200 ms d'intervalle
    let (mock_router, submit_count) = MockTaskSubmitter::new();
    let bus_tx = make_bus();
    let def = interval_def("fast-trigger", "200ms", OnBusyPolicy::Queue);
    let _handle = TriggerEngineHandle::spawn(vec![def], mock_router, bus_tx, None).await;

    // WHEN — attendre 700 ms (3,5× l'intervalle)
    tokio::time::sleep(Duration::from_millis(700)).await;

    // THEN — au moins 2 submit() effectués (marge pour CI lente)
    let count = submit_count.load(Ordering::SeqCst);
    assert!(
        count >= 2,
        "expected at least 2 submits in 700ms (interval=200ms), got {count}"
    );
}

// ─── AC-2 ─────────────────────────────────────────────────────────────────

/// AC-2 : FileWatch `create` → 1 submit() dans les 5 secondes.
#[tokio::test]
async fn test_ac2_file_watch_create_submits_task() {
    // GIVEN — moteur avec FileWatch sur un répertoire temporaire
    let dir = TempDir::new().expect("TempDir creation failed");
    let (mock_router, submit_count) = MockTaskSubmitter::new();
    let bus_tx = make_bus();
    let def = TriggerDefinition {
        id: "watch-test".into(),
        agent: "test-agent".into(),
        enabled: true,
        on_busy: OnBusyPolicy::Queue,
        source: TriggerSourceConfig::FileWatch {
            path: dir.path().to_path_buf(),
            events: vec![FileEventKind::Create],
        },
        input_template: InputTemplate("file: {{filename}}".into()),
    };
    let _handle = TriggerEngineHandle::spawn(vec![def], mock_router, bus_tx, None).await;

    // Laisser le watcher s'initialiser
    tokio::time::sleep(Duration::from_millis(300)).await;

    // WHEN — créer un fichier dans le répertoire surveillé
    let test_file = dir.path().join("facture.pdf");
    std::fs::write(&test_file, b"pdf-content").expect("file write failed");

    // THEN — au moins 1 submit dans les 5 secondes (marge large pour CI)
    let submitted = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if submit_count.load(Ordering::SeqCst) >= 1 {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;

    assert!(
        submitted.unwrap_or(false),
        "FileWatch did not submit a task within 5s after file creation"
    );
}

// ─── AC-5 ─────────────────────────────────────────────────────────────────

/// AC-5 : `on_busy=Drop` + agent occupé → `TriggerSkipped` émis, zéro submit.
#[tokio::test]
async fn test_ac5_on_busy_drop_skips_and_emits_event() {
    // GIVEN — mock occupé (pending_count = 1) + policy Drop + interval "1h" (pas d'auto-fire)
    let (mock_router, submit_count) = MockTaskSubmitter::new_busy();
    let bus_tx = tokio::sync::broadcast::channel::<RuntimeEvent>(64).0;
    let mut bus_rx = bus_tx.subscribe();
    let def = interval_def("drop-trigger", "1h", OnBusyPolicy::Drop);
    let handle = TriggerEngineHandle::spawn(vec![def], mock_router, bus_tx, None).await;

    // WHEN — fire_now force le déclenchement (ignoré car agent occupé + Drop)
    let _ = handle.fire_now("drop-trigger").await;

    // THEN — TriggerSkipped doit être reçu dans les 500 ms
    let skipped = tokio::time::timeout(Duration::from_millis(500), async {
        loop {
            match bus_rx.recv().await {
                Ok(RuntimeEvent::TriggerSkipped { trigger_id, .. })
                    if trigger_id == "drop-trigger" =>
                {
                    return true;
                }
                Ok(_) => continue,
                Err(_) => return false,
            }
        }
    })
    .await;

    assert!(
        skipped.unwrap_or(false),
        "TriggerSkipped event not received within 500ms"
    );
    assert_eq!(
        submit_count.load(Ordering::SeqCst),
        0,
        "submit() must NOT be called when on_busy=Drop and agent is busy"
    );
}

// ─── AC-6 ─────────────────────────────────────────────────────────────────

/// AC-6 : `on_busy=Queue` + agent occupé → 1 submit() quand même.
#[tokio::test]
async fn test_ac6_on_busy_queue_submits_task_when_agent_busy() {
    // GIVEN — mock occupé (pending_count = 1) + policy Queue + interval "1h" (pas d'auto-fire)
    let (mock_router, submit_count) = MockTaskSubmitter::new_busy();
    let bus_tx = make_bus();
    let def = interval_def("queue-trigger", "1h", OnBusyPolicy::Queue);
    let handle = TriggerEngineHandle::spawn(vec![def], mock_router, bus_tx, None).await;

    // WHEN — fire_now force le déclenchement
    let result = handle.fire_now("queue-trigger").await;

    // Laisser le temps à l'acteur de traiter
    tokio::time::sleep(Duration::from_millis(100)).await;

    // THEN — tâche soumise malgré l'agent occupé (Queue délègue au TaskRouter)
    assert!(
        result.is_ok(),
        "fire_now should succeed with Queue policy, got: {result:?}"
    );
    assert_eq!(
        submit_count.load(Ordering::SeqCst),
        1,
        "expected exactly 1 submit with on_busy=Queue even when agent is busy"
    );
}
