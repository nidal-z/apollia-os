//! Observer — enriches incoming `AIPTask` into a `ContextBundle`.
//!
//! The Observer is the first component of the ORIA pipeline, triggered
//! when a task is received. Its role is twofold:
//!
//! 1. **Enrichment**: build a [`ContextBundle`] grouping the original task,
//!    a memory snapshot (recent episodes + semantic facts), and the execution mode.
//!
//! 2. **Classification**: determine whether the task should run in [`ExecutionMode::Direct`]
//!    (single step) or [`ExecutionMode::Orchestrated`] (multi-step with planning).
//!
//! The Observer is a **pure function** (not a Tokio actor) — it takes inputs and
//! returns a result with no internal state.

use apollia_core::{AIPTask, AgentManifest};
use apollia_memory::episodic::EpisodicMemory;
use apollia_memory::manager::MemoryManager;
use apollia_memory::semantic::SemanticMemory;

/// Maximum number of recent episodes loaded into the snapshot.
const MAX_RECENT_EPISODES: usize = 10;

/// Step budget threshold above which a task is considered complex.
const COMPLEXITY_STEP_THRESHOLD: u32 = 15;

/// Input parts threshold above which a task is considered complex.
const COMPLEXITY_PARTS_THRESHOLD: usize = 3;

/// Tools required threshold above which a task is considered complex.
const COMPLEXITY_TOOLS_THRESHOLD: usize = 4;

/// Execution mode determined by the Observer for a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Simple single-step execution.
    Direct,
    /// Multi-step execution with planning.
    Orchestrated,
}

/// Snapshot of relevant memory for a task.
///
/// Built by the Observer during enrichment.
#[derive(Debug, Clone)]
pub struct MemorySnapshot {
    /// Recent episodes from episodic memory (textual content).
    pub episodic_recent: Vec<String>,
    /// Relevant semantic facts (key/value pairs).
    pub semantic_relevant: Vec<(String, String)>,
}

/// Enriched context bundle for a task.
///
/// Produced by [`observe`], consumed by the Reasoner.
#[derive(Debug, Clone)]
pub struct ContextBundle {
    /// Original task.
    pub task: AIPTask,
    /// Memory snapshot (None if no namespace configured).
    pub memory_snapshot: Option<MemorySnapshot>,
    /// Execution mode determined by [`classify`].
    pub execution_mode: ExecutionMode,
}

/// Errors that can occur during observation.
#[derive(Debug, thiserror::Error)]
pub enum ObserverError {
    /// Failed to build context from available data.
    #[error("failed to build context: {0}")]
    ContextBuildFailed(String),
    /// Memory access failed during snapshot construction.
    #[error("memory access failed: {0}")]
    MemoryError(String),
}

/// Classifies a task as Direct or Orchestrated.
///
/// Pure function based on simple heuristics.
///
/// Complexity criteria (any single one triggers Orchestrated):
/// - `manifest.step_budget.max_steps > 15`
/// - `task.input.parts.len() > 3`
/// - `manifest.tags` contains "multi-step"
/// - `manifest.tools_required.len() > 4`
pub fn classify(task: &AIPTask, manifest: &AgentManifest) -> ExecutionMode {
    let budget = manifest.step_budget.clone().unwrap_or_default();

    let is_complex = budget.max_steps > COMPLEXITY_STEP_THRESHOLD
        || task.input.parts.len() > COMPLEXITY_PARTS_THRESHOLD
        || manifest.tags.iter().any(|t| t == "multi-step")
        || manifest.tools_required.len() > COMPLEXITY_TOOLS_THRESHOLD;

    if is_complex {
        ExecutionMode::Orchestrated
    } else {
        ExecutionMode::Direct
    }
}

/// Observes a task and builds an enriched [`ContextBundle`].
///
/// If a [`MemoryManager`] is provided, loads a memory snapshot
/// (recent episodes + relevant semantic facts).
/// If no `MemoryManager` is provided, `memory_snapshot` is `None`.
///
/// The execution mode is determined by [`classify`].
pub fn observe(
    task: AIPTask,
    manifest: &AgentManifest,
    memory: Option<&mut MemoryManager>,
) -> Result<ContextBundle, ObserverError> {
    let execution_mode = classify(&task, manifest);

    let memory_snapshot = match memory {
        Some(mgr) => {
            let namespace = match &manifest.memory_namespace {
                Some(ns) => ns.clone(),
                None => {
                    return Ok(ContextBundle {
                        task,
                        memory_snapshot: None,
                        execution_mode,
                    });
                }
            };

            let store = mgr
                .store(&namespace)
                .map_err(|e| ObserverError::MemoryError(e.to_string()))?;

            let episodic = EpisodicMemory::new(store);
            let episodes = episodic
                .history(&namespace, MAX_RECENT_EPISODES as u32, None)
                .map_err(|e| ObserverError::MemoryError(e.to_string()))?;

            let episodic_recent: Vec<String> = episodes.into_iter().map(|ep| ep.content).collect();

            let semantic = SemanticMemory::new(store);
            let facts = semantic
                .recall_all(&namespace)
                .map_err(|e| ObserverError::MemoryError(e.to_string()))?;

            let semantic_relevant: Vec<(String, String)> = facts
                .into_iter()
                .map(|entry| (entry.key, entry.value.to_string()))
                .collect();

            Some(MemorySnapshot {
                episodic_recent,
                semantic_relevant,
            })
        }
        None => None,
    };

    Ok(ContextBundle {
        task,
        memory_snapshot,
        execution_mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AIPInput, AIPPart, StepBudgetConfig, TextPart};

    fn simple_manifest() -> AgentManifest {
        AgentManifest {
            name: "simple-agent".into(),
            version: "1.0.0".into(),
            description: "A simple agent".into(),
            tools_required: vec!["file_io".into(), "bash".into()],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: Some(StepBudgetConfig {
                max_steps: 10,
                max_tool_calls: 20,
                wall_clock_secs: 300,
            }),
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
        }
    }

    fn complex_manifest() -> AgentManifest {
        AgentManifest {
            name: "complex-agent".into(),
            version: "1.0.0".into(),
            description: "A complex agent".into(),
            tools_required: vec![
                "file_io".into(),
                "bash".into(),
                "python".into(),
                "http".into(),
                "db".into(),
            ],
            tools_optional: vec![],
            supports_streaming: false,
            supports_a2a: false,
            memory_namespace: None,
            shared_memory_namespaces: vec![],
            max_concurrent_tasks: 1,
            step_budget: Some(StepBudgetConfig {
                max_steps: 20,
                max_tool_calls: 50,
                wall_clock_secs: 600,
            }),
            network_allowlist: None,
            dangerous_tools_allowed: false,
            tags: vec![],
            skills: vec![],
        }
    }

    fn simple_task() -> AIPTask {
        AIPTask {
            task_id: "task-001".into(),
            context_id: "ctx-001".into(),
            input: AIPInput {
                parts: vec![AIPPart::Text(TextPart {
                    text: "Generate a report".into(),
                })],
            },
            history: vec![],
            timeout_seconds: None,
        }
    }

    fn multi_part_task() -> AIPTask {
        AIPTask {
            task_id: "task-002".into(),
            context_id: "ctx-002".into(),
            input: AIPInput {
                parts: vec![
                    AIPPart::Text(TextPart {
                        text: "Part 1".into(),
                    }),
                    AIPPart::Text(TextPart {
                        text: "Part 2".into(),
                    }),
                    AIPPart::Text(TextPart {
                        text: "Part 3".into(),
                    }),
                    AIPPart::Text(TextPart {
                        text: "Part 4".into(),
                    }),
                ],
            },
            history: vec![],
            timeout_seconds: None,
        }
    }

    // AC-1 — observe with memory populates snapshot
    #[test]
    fn test_observe_with_memory_populates_snapshot() {
        // GIVEN a MemoryManager with episodes and semantic facts
        let dir = std::env::temp_dir().join(format!("apollia_obs_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        let mut mgr = MemoryManager::new(&dir, Some("agent-test".into()), vec![]);

        let store = mgr.store("agent-test").expect("open store");

        let episodic = EpisodicMemory::new(store);
        episodic
            .record(
                "agent-test",
                "agent-1",
                "Episode one",
                0.8,
                None,
                None,
                None,
            )
            .expect("record episode 1");
        episodic
            .record(
                "agent-test",
                "agent-1",
                "Episode two",
                0.5,
                None,
                None,
                None,
            )
            .expect("record episode 2");

        let semantic = SemanticMemory::new(store);
        semantic
            .remember(
                "agent-test",
                "client.budget",
                &serde_json::json!(15000),
                1.0,
                None,
                None,
            )
            .expect("remember fact");

        let mut manifest = simple_manifest();
        manifest.memory_namespace = Some("agent-test".into());

        let task = simple_task();

        // WHEN
        let bundle = observe(task, &manifest, Some(&mut mgr)).expect("observe");

        // THEN
        assert!(bundle.memory_snapshot.is_some());
        let snapshot = bundle.memory_snapshot.as_ref().expect("snapshot");
        assert_eq!(snapshot.episodic_recent.len(), 2);
        assert_eq!(snapshot.semantic_relevant.len(), 1);
        assert_eq!(snapshot.semantic_relevant[0].0, "client.budget");
    }

    // AC-2 — observe without memory has snapshot None
    #[test]
    fn test_observe_without_memory_snapshot_is_none() {
        // GIVEN
        let task = simple_task();
        let manifest = simple_manifest();

        // WHEN
        let bundle = observe(task, &manifest, None).expect("observe");

        // THEN
        assert!(bundle.memory_snapshot.is_none());
        assert_eq!(bundle.execution_mode, ExecutionMode::Direct);
    }

    // AC-3 — classify simple agent returns Direct
    #[test]
    fn test_classify_simple_agent_returns_direct() {
        // GIVEN
        let task = simple_task();
        let manifest = simple_manifest();

        // WHEN
        let mode = classify(&task, &manifest);

        // THEN
        assert_eq!(mode, ExecutionMode::Direct);
    }

    // AC-4 — classify complex agent returns Orchestrated
    #[test]
    fn test_classify_complex_agent_returns_orchestrated() {
        // GIVEN
        let task = simple_task();
        let manifest = complex_manifest();

        // WHEN
        let mode = classify(&task, &manifest);

        // THEN
        assert_eq!(mode, ExecutionMode::Orchestrated);
    }

    // AC-5 — tag "multi-step" forces Orchestrated
    #[test]
    fn test_classify_multi_step_tag_returns_orchestrated() {
        // GIVEN a simple manifest but with "multi-step" tag
        let mut manifest = simple_manifest();
        manifest.tags = vec!["multi-step".into()];
        let task = simple_task();

        // WHEN
        let mode = classify(&task, &manifest);

        // THEN
        assert_eq!(mode, ExecutionMode::Orchestrated);
    }

    // AC-6 — many input parts forces Orchestrated
    #[test]
    fn test_classify_many_input_parts_returns_orchestrated() {
        // GIVEN a simple manifest and a task with 4 input parts
        let manifest = simple_manifest();
        let task = multi_part_task();

        // WHEN
        let mode = classify(&task, &manifest);

        // THEN
        assert_eq!(mode, ExecutionMode::Orchestrated);
    }
}
