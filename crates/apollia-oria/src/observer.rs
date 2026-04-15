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

use apollia_core::{AIPInput, AIPPart, AIPTask, AgentManifest};
use apollia_memory::episodic::EpisodicMemory;
use apollia_memory::manager::MemoryManager;
use apollia_memory::semantic::SemanticMemory;

/// Maximum number of recent episodes loaded into the snapshot.
const MAX_RECENT_EPISODES: usize = 10;

// ─── Weighted scoring constants ─────────────────────────────────

/// Weight for high step budget (`max_steps > 15`).
const WEIGHT_STEPS: f32 = 0.30;

/// Weight for many input parts (`parts.len() > 3`).
const WEIGHT_PARTS: f32 = 0.20;

/// Weight for the `"multi-step"` tag presence.
const WEIGHT_MULTI_STEP_TAG: f32 = 0.40;

/// Weight for many required tools (`tools_required.len() > 4`).
const WEIGHT_TOOLS: f32 = 0.20;

/// Weight for long input text (`total chars > INPUT_LENGTH_THRESHOLD`).
const WEIGHT_INPUT_LENGTH: f32 = 0.10;

/// Weight for deep episodic memory (`episodes.len() > MEMORY_DEPTH_THRESHOLD`).
const WEIGHT_MEMORY_DEPTH: f32 = 0.10;

/// Weight for planning keywords in the system prompt.
const WEIGHT_PLANNING_PROMPT: f32 = 0.10;

/// Default minimum weighted score to classify a task as Orchestrated.
///
/// Used as the default threshold value in tests. In production, the threshold
/// is read from [`apollia_core::ORIAConfig::orchestrated_threshold`] and passed
/// directly to [`classify`].
#[cfg(test)]
const ORCHESTRATED_THRESHOLD: f32 = 0.40;

/// Input text length (in chars) above which `WEIGHT_INPUT_LENGTH` is added.
const INPUT_LENGTH_THRESHOLD: usize = 500;

/// Episode count above which `WEIGHT_MEMORY_DEPTH` is added.
const MEMORY_DEPTH_THRESHOLD: usize = 5;

/// Step budget threshold above which `WEIGHT_STEPS` is added.
const COMPLEXITY_STEP_THRESHOLD: u32 = 15;

/// Input parts threshold above which `WEIGHT_PARTS` is added.
const COMPLEXITY_PARTS_THRESHOLD: usize = 3;

/// Tools required threshold above which `WEIGHT_TOOLS` is added.
const COMPLEXITY_TOOLS_THRESHOLD: usize = 4;

/// Keywords in the system prompt that suggest planning intent.
const PLANNING_KEYWORDS: &[&str] = &["plan", "etape", "step", "sequence", "workflow", "pipeline"];

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
    /// Names of tools available to the agent (from manifest `tools_required` + `tools_optional`).
    pub available_tools: Vec<String>,
    /// System prompt from the agent manifest, forwarded to the Reasoner for plan generation.
    pub manifest_system_prompt: Option<String>,
    /// Names of LLM backends available for multi-model routing per step.
    ///
    /// Injected into the Reasoner prompt via `{llm_backend_names}` so the LLM
    /// can populate `PlanStep.model_hint` with valid backend names.
    pub llm_backend_names: Vec<String>,
}

impl Default for ContextBundle {
    fn default() -> Self {
        Self {
            task: AIPTask::default(),
            memory_snapshot: None,
            execution_mode: ExecutionMode::Direct,
            available_tools: vec![],
            manifest_system_prompt: None,
            llm_backend_names: vec![],
        }
    }
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

/// Extracts the total character length of all text parts in an [`AIPInput`].
///
/// Only [`AIPPart::Text`] variants contribute; `File` and `Data` parts are ignored.
pub fn extract_total_text_length(input: &AIPInput) -> usize {
    input
        .parts
        .iter()
        .map(|part| match part {
            AIPPart::Text(t) => t.text.len(),
            AIPPart::File(_) | AIPPart::Data(_) => 0,
        })
        .sum()
}

/// Computes a weighted complexity score for a task, between 0.0 and ~1.4.
///
/// Each factor that exceeds its threshold adds its weight to the total.
/// The caller compares the result against [`ORCHESTRATED_THRESHOLD`].
///
/// This is a **pure function** — deterministic, no side effects.
pub fn compute_complexity_score(
    manifest: &AgentManifest,
    input: &AIPInput,
    memory_snapshot: Option<&MemorySnapshot>,
) -> f32 {
    let mut score: f32 = 0.0;

    let budget = manifest.step_budget.clone().unwrap_or_default();
    if budget.max_steps > COMPLEXITY_STEP_THRESHOLD {
        score += WEIGHT_STEPS;
    }

    if input.parts.len() > COMPLEXITY_PARTS_THRESHOLD {
        score += WEIGHT_PARTS;
    }

    if manifest.tags.iter().any(|t| t == "multi-step") {
        score += WEIGHT_MULTI_STEP_TAG;
    }

    if manifest.tools_required.len() > COMPLEXITY_TOOLS_THRESHOLD {
        score += WEIGHT_TOOLS;
    }

    if extract_total_text_length(input) > INPUT_LENGTH_THRESHOLD {
        score += WEIGHT_INPUT_LENGTH;
    }

    if let Some(snapshot) = memory_snapshot {
        if snapshot.episodic_recent.len() > MEMORY_DEPTH_THRESHOLD {
            score += WEIGHT_MEMORY_DEPTH;
        }
    }

    if let Some(ref prompt) = manifest.system_prompt {
        let lower = prompt.to_lowercase();
        if PLANNING_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
            score += WEIGHT_PLANNING_PROMPT;
        }
    }

    score
}

/// Classifies a task as Direct or Orchestrated using weighted scoring.
///
/// Honours the `execution_mode` field of the manifest first:
/// - `"orchestrated"` → always [`ExecutionMode::Orchestrated`], skipping scoring.
/// - `"direct"` → always [`ExecutionMode::Direct`], skipping scoring.
/// - `"auto"` (or any unknown value) → falls through to [`compute_complexity_score`].
///
/// If the weighted score ≥ `threshold`, the task is Orchestrated.
/// Pass [`ORCHESTRATED_THRESHOLD`] as the default when no config is available.
///
/// This is a **pure function** — no side effects, deterministic output.
pub fn classify(
    task: &AIPTask,
    manifest: &AgentManifest,
    memory_snapshot: Option<&MemorySnapshot>,
    threshold: f32,
) -> ExecutionMode {
    // Override explicite — priorité absolue sur le scoring.
    match manifest.execution_mode.as_str() {
        "orchestrated" => return ExecutionMode::Orchestrated,
        "direct" => return ExecutionMode::Direct,
        _ => {} // "auto" ou valeur inconnue → scoring pondéré
    }

    let score = compute_complexity_score(manifest, &task.input, memory_snapshot);

    if score >= threshold {
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
/// The memory snapshot is built first so it can inform [`classify`]
/// (the `WEIGHT_MEMORY_DEPTH` factor uses episode count).
///
/// `threshold` is forwarded to [`classify`] — pass [`ORCHESTRATED_THRESHOLD`]
/// as the default when no config is available.
pub fn observe(
    task: AIPTask,
    manifest: &AgentManifest,
    memory: Option<&mut MemoryManager>,
    threshold: f32,
) -> Result<ContextBundle, ObserverError> {
    let manifest_system_prompt = manifest.system_prompt.clone();

    let available_tools: Vec<String> = manifest
        .tools_required
        .iter()
        .chain(manifest.tools_optional.iter())
        .cloned()
        .collect();

    // Build memory snapshot first so classify() can use it for scoring.
    let memory_snapshot = match memory {
        Some(mgr) => {
            let namespace = match &manifest.memory_namespace {
                Some(ns) => ns.clone(),
                None => {
                    let execution_mode = classify(&task, manifest, None, threshold);
                    return Ok(ContextBundle {
                        task,
                        memory_snapshot: None,
                        execution_mode,
                        available_tools,
                        manifest_system_prompt,
                        llm_backend_names: vec![],
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

    let execution_mode = classify(&task, manifest, memory_snapshot.as_ref(), threshold);

    Ok(ContextBundle {
        task,
        memory_snapshot,
        execution_mode,
        available_tools,
        manifest_system_prompt,
        llm_backend_names: vec![],
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
            execution_mode: "auto".to_string(),
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
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
            execution_mode: "auto".to_string(),
            system_prompt: None,
            tools_requiring_approval: vec![],
            llm_backend: None,
            packages: vec![],
            memory_config: None,
            agent_type: None,
            examples: vec![],
            limitations: vec![],
            setup_notes: None,
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
            ..AIPTask::default()
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
            ..AIPTask::default()
        }
    }

    // observe with memory populates snapshot
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
        let bundle =
            observe(task, &manifest, Some(&mut mgr), ORCHESTRATED_THRESHOLD).expect("observe");

        // THEN
        assert!(bundle.memory_snapshot.is_some());
        let snapshot = bundle.memory_snapshot.as_ref().expect("snapshot");
        assert_eq!(snapshot.episodic_recent.len(), 2);
        assert_eq!(snapshot.semantic_relevant.len(), 1);
        assert_eq!(snapshot.semantic_relevant[0].0, "client.budget");
    }

    // observe without memory has snapshot None
    #[test]
    fn test_observe_without_memory_snapshot_is_none() {
        // GIVEN
        let task = simple_task();
        let manifest = simple_manifest();

        // WHEN
        let bundle = observe(task, &manifest, None, ORCHESTRATED_THRESHOLD).expect("observe");

        // THEN
        assert!(bundle.memory_snapshot.is_none());
        assert_eq!(bundle.execution_mode, ExecutionMode::Direct);
    }

    // classify simple agent returns Direct
    #[test]
    fn test_classify_simple_agent_returns_direct() {
        // GIVEN
        let task = simple_task();
        let manifest = simple_manifest();

        // WHEN
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN
        assert_eq!(mode, ExecutionMode::Direct);
    }

    // classify complex agent returns Orchestrated
    #[test]
    fn test_classify_complex_agent_returns_orchestrated() {
        // GIVEN — 5 tools (>4) + 20 steps (>15) → score = 0.20 + 0.30 = 0.50 ≥ 0.40
        let task = simple_task();
        let manifest = complex_manifest();

        // WHEN
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN
        assert_eq!(mode, ExecutionMode::Orchestrated);
    }

    // tag "multi-step" forces Orchestrated
    #[test]
    fn test_classify_multi_step_tag_returns_orchestrated() {
        // GIVEN a simple manifest but with "multi-step" tag → score = 0.40 ≥ 0.40
        let mut manifest = simple_manifest();
        manifest.tags = vec!["multi-step".into()];
        let task = simple_task();

        // WHEN
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN
        assert_eq!(mode, ExecutionMode::Orchestrated);
    }

    // many input parts alone are below threshold with weighted scoring
    // (4 parts = WEIGHT_PARTS 0.20 < 0.40 — correctly classified as Direct)
    #[test]
    fn test_classify_many_input_parts_alone_returns_direct() {
        // GIVEN a simple manifest and a task with 4 input parts
        let manifest = simple_manifest();
        let task = multi_part_task();

        // WHEN — score = WEIGHT_PARTS (0.20) < ORCHESTRATED_THRESHOLD (0.40)
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN — weighted scoring reduces false positives vs old boolean OR
        assert_eq!(mode, ExecutionMode::Direct);
    }

    // override explicite "orchestrated"
    #[test]
    fn test_ac1_orchestrated_override() {
        // GIVEN un manifest avec execution_mode = "orchestrated"
        let mut manifest = simple_manifest();
        manifest.execution_mode = "orchestrated".to_string();
        let task = simple_task();

        // WHEN
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN : override prime, même pour un agent simple
        assert_eq!(mode, ExecutionMode::Orchestrated);
    }

    // override explicite "direct" même avec 6 outils
    #[test]
    fn test_ac2_direct_override_despite_many_tools() {
        // GIVEN un manifest avec execution_mode = "direct" et 6 outils (> 4)
        let mut manifest = simple_manifest();
        manifest.execution_mode = "direct".to_string();
        manifest.tools_required = vec!["a", "b", "c", "d", "e", "f"]
            .into_iter()
            .map(String::from)
            .collect();
        let task = simple_task();

        // WHEN
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN : l'override prime sur l'heuristique
        assert_eq!(mode, ExecutionMode::Direct);
    }

    // "auto" + 5 outils + 20 steps → Orchestrated
    // (5 tools alone = 0.20, need steps > 15 too for 0.50 ≥ 0.40)
    #[test]
    fn test_ac3_auto_heuristic_orchestrated_on_many_tools_and_steps() {
        // GIVEN un manifest avec 5 outils ET 20 steps
        let mut manifest = simple_manifest();
        manifest.execution_mode = "auto".to_string();
        manifest.tools_required = vec!["a", "b", "c", "d", "e"]
            .into_iter()
            .map(String::from)
            .collect();
        manifest.step_budget = Some(StepBudgetConfig {
            max_steps: 20,
            max_tool_calls: 50,
            wall_clock_secs: 600,
        });
        let task = simple_task();

        // WHEN — score = WEIGHT_TOOLS (0.20) + WEIGHT_STEPS (0.30) = 0.50 ≥ 0.40
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN : scoring → Orchestrated
        assert_eq!(mode, ExecutionMode::Orchestrated);
    }

    // "auto" + agent simple → Direct
    #[test]
    fn test_ac4_auto_heuristic_direct_on_simple_agent() {
        // GIVEN un manifest avec execution_mode = "auto" et 1 outil
        let mut manifest = simple_manifest();
        manifest.execution_mode = "auto".to_string();
        manifest.tools_required = vec!["file_io".to_string()];
        let task = simple_task();

        // WHEN
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN : heuristique → Direct
        assert_eq!(mode, ExecutionMode::Direct);
    }

    // agent simple classé Direct (score ~0.0)
    #[test]
    fn test_simple_agent_classified_direct() {
        // GIVEN un agent avec 2 outils, 10 steps max, pas de tag, input court
        let manifest = simple_manifest();
        let task = simple_task();

        // WHEN
        let score = compute_complexity_score(&manifest, &task.input, None);
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN
        assert!(score < f32::EPSILON, "score should be ~0.0, got {score}");
        assert_eq!(mode, ExecutionMode::Direct);
    }

    // agent complexe classé Orchestrated (score >= 0.9)
    #[test]
    fn test_complex_agent_classified_orchestrated() {
        // GIVEN un agent avec 5+ outils, 20 steps, tag "multi-step", input long
        let mut manifest = complex_manifest();
        manifest.tags = vec!["multi-step".into()];
        let task = AIPTask {
            task_id: "task-complex".into(),
            context_id: "ctx-complex".into(),
            input: AIPInput {
                parts: vec![
                    AIPPart::Text(TextPart {
                        text: "a]".repeat(300),
                    }),
                    AIPPart::Text(TextPart {
                        text: "b".repeat(300),
                    }),
                ],
            },
            history: vec![],
            timeout_seconds: None,
            ..AIPTask::default()
        };

        // WHEN — steps(0.30) + parts(0) + tag(0.40) + tools(0.20) + input_len(0.10) = 1.0
        let score = compute_complexity_score(&manifest, &task.input, None);
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN
        assert!(score >= 0.90, "score should be >= 0.90, got {score}");
        assert_eq!(mode, ExecutionMode::Orchestrated);
    }

    // tag "multi-step" seul déclenche Orchestrated
    #[test]
    fn test_multi_step_tag_alone_triggers_orchestrated() {
        // GIVEN un agent avec 1 outil, 5 steps, tag "multi-step", input court
        let mut manifest = simple_manifest();
        manifest.tools_required = vec!["file_io".into()];
        manifest.step_budget = Some(StepBudgetConfig {
            max_steps: 5,
            max_tool_calls: 10,
            wall_clock_secs: 60,
        });
        manifest.tags = vec!["multi-step".into()];
        let task = simple_task();

        // WHEN
        let score = compute_complexity_score(&manifest, &task.input, None);
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN — score = WEIGHT_MULTI_STEP_TAG (0.40) ≥ 0.40
        assert!(score >= 0.40, "score should be >= 0.40, got {score}");
        assert_eq!(mode, ExecutionMode::Orchestrated);
    }

    // input long + nombreux outils contribuent au score
    #[test]
    fn test_input_length_and_tools_contribute_to_score() {
        // GIVEN un agent avec 6 outils, 10 steps, pas de tag, input de 800 chars
        let mut manifest = simple_manifest();
        manifest.tools_required = vec!["a", "b", "c", "d", "e", "f"]
            .into_iter()
            .map(String::from)
            .collect();
        let task = AIPTask {
            task_id: "task-long".into(),
            context_id: "ctx-long".into(),
            input: AIPInput {
                parts: vec![AIPPart::Text(TextPart {
                    text: "x".repeat(800),
                })],
            },
            history: vec![],
            timeout_seconds: None,
            ..AIPTask::default()
        };

        // WHEN — tools(0.20) + input_len(0.10) = 0.30
        let score = compute_complexity_score(&manifest, &task.input, None);

        // THEN
        assert!(score >= 0.30, "score should be >= 0.30, got {score}");
    }

    // serde round-trip JSON → execution_mode + system_prompt
    #[test]
    fn test_ac5_serde_round_trip() {
        use apollia_core::AgentManifest;

        // GIVEN un JSON avec execution_mode et system_prompt
        let json = r#"{"name":"a","version":"1.0.0","description":"d","tools_required":[],"execution_mode":"orchestrated","system_prompt":"Planifie."}"#;

        // WHEN
        let manifest: AgentManifest = serde_json::from_str(json).expect("deserialize");

        // THEN
        assert_eq!(manifest.execution_mode, "orchestrated");
        assert_eq!(manifest.system_prompt, Some("Planifie.".to_string()));
    }

    // valeur par défaut "auto" quand le champ est absent
    #[test]
    fn test_default_execution_mode_is_auto() {
        use apollia_core::AgentManifest;

        // GIVEN un JSON minimal sans execution_mode
        let json = r#"{"name":"a","version":"1.0.0","description":"d","tools_required":[]}"#;

        // WHEN
        let manifest: AgentManifest = serde_json::from_str(json).expect("deserialize");

        // THEN
        assert_eq!(manifest.execution_mode, "auto");
        assert_eq!(manifest.system_prompt, None);
    }
}
