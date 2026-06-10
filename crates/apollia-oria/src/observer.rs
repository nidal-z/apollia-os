//! Observer: enriches incoming `AIPTask` into a `ContextBundle`.
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
//! The Observer is a **pure function** (not a Tokio actor): it takes inputs and
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
/// This is a **pure function**: deterministic, no side effects.
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

/// Returns the calibrated threshold above which a request is considered worth
/// orchestrating.
///
/// Single source of truth for both ORIA orchestration and the chat plan-flow
/// routing: the value is read from [`apollia_core::ORIAConfig`] (its calibrated
/// default), never duplicated as a literal in callers. The chat turn router
/// compares [`score_turn_text`] against this value to decide whether a turn is
/// substantive enough to enter the plan flow.
pub fn orchestrated_threshold() -> f32 {
    apollia_core::ORIAConfig::default().orchestrated_threshold as f32
}

/// Scores a free-text chat turn for substantiveness, reusing the Observer's
/// existing weights.
///
/// The Observer's [`compute_complexity_score`] scores a task from manifest
/// features (tools, step budget, tags) that a raw chat turn does not carry. This
/// helper applies the same weights to the two signals a turn does expose:
///
/// - planning intent: the turn mentions a [`PLANNING_KEYWORDS`] cue, adding
///   `WEIGHT_MULTI_STEP_TAG` (the strongest planning weight, mirroring the
///   `"multi-step"` tag);
/// - length: the turn is longer than [`INPUT_LENGTH_THRESHOLD`], adding
///   `WEIGHT_INPUT_LENGTH`.
///
/// The weights are the Observer's own constants, so the calibration stays in one
/// place. An empty turn scores `0.0`.
///
/// This is a **pure function**: deterministic, no side effects, no model call.
pub fn score_turn_text(input: &str) -> f32 {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return 0.0;
    }

    let mut score: f32 = 0.0;
    let lower = trimmed.to_lowercase();

    if PLANNING_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        score += WEIGHT_MULTI_STEP_TAG;
    }

    if trimmed.chars().count() > INPUT_LENGTH_THRESHOLD {
        score += WEIGHT_INPUT_LENGTH;
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
/// If the weighted score is at least `threshold`, the task is Orchestrated.
/// Pass [`ORCHESTRATED_THRESHOLD`] as the default when no config is available.
///
/// This is a **pure function**: no side effects, deterministic output.
pub fn classify(
    task: &AIPTask,
    manifest: &AgentManifest,
    memory_snapshot: Option<&MemorySnapshot>,
    threshold: f32,
) -> ExecutionMode {
    // Explicit override: absolute priority over scoring.
    match manifest.execution_mode.as_str() {
        "orchestrated" => return ExecutionMode::Orchestrated,
        "direct" => return ExecutionMode::Direct,
        _ => {} // "auto" or unknown value: weighted scoring
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
/// `threshold` is forwarded to [`classify`]; pass [`ORCHESTRATED_THRESHOLD`]
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
                .recall_all(&namespace, None)
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
            agent_class: None,
            user_memory_write: false,
            datasources: vec![],
            templates: vec![],
            secrets: vec![],
            check_commands: vec![],
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
            agent_class: None,
            user_memory_write: false,
            datasources: vec![],
            templates: vec![],
            secrets: vec![],
            check_commands: vec![],
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
            .remember(apollia_memory::semantic::RememberInput {
                namespace: "agent-test",
                key: "client.budget",
                value: &serde_json::json!(15000),
                confidence: 1.0,
                source: None,
                expires_at: None,
            })
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
        // GIVEN: 5 tools (>4) + 20 steps (>15) -> score = 0.20 + 0.30 = 0.50 >= 0.40
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
    // (4 parts = WEIGHT_PARTS 0.20 < 0.40, correctly classified as Direct)
    #[test]
    fn test_classify_many_input_parts_alone_returns_direct() {
        // GIVEN a simple manifest and a task with 4 input parts
        let manifest = simple_manifest();
        let task = multi_part_task();

        // WHEN: score = WEIGHT_PARTS (0.20) < ORCHESTRATED_THRESHOLD (0.40)
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN: weighted scoring reduces false positives vs old boolean OR
        assert_eq!(mode, ExecutionMode::Direct);
    }

    // explicit "orchestrated" override
    #[test]
    fn test_orchestrated_override() {
        // GIVEN a manifest with execution_mode = "orchestrated"
        let mut manifest = simple_manifest();
        manifest.execution_mode = "orchestrated".to_string();
        let task = simple_task();

        // WHEN
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN: the override wins, even for a simple agent
        assert_eq!(mode, ExecutionMode::Orchestrated);
    }

    // explicit "direct" override even with 6 tools
    #[test]
    fn test_direct_override_despite_many_tools() {
        // GIVEN a manifest with execution_mode = "direct" and 6 tools (> 4)
        let mut manifest = simple_manifest();
        manifest.execution_mode = "direct".to_string();
        manifest.tools_required = vec!["a", "b", "c", "d", "e", "f"]
            .into_iter()
            .map(String::from)
            .collect();
        let task = simple_task();

        // WHEN
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN: the override wins over the heuristic
        assert_eq!(mode, ExecutionMode::Direct);
    }

    // "auto" + 5 tools + 20 steps -> Orchestrated
    // (5 tools alone = 0.20, need steps > 15 too for 0.50 >= 0.40)
    #[test]
    fn test_auto_heuristic_orchestrated_on_many_tools_and_steps() {
        // GIVEN a manifest with 5 tools AND 20 steps
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

        // WHEN: score = WEIGHT_TOOLS (0.20) + WEIGHT_STEPS (0.30) = 0.50 >= 0.40
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN: scoring -> Orchestrated
        assert_eq!(mode, ExecutionMode::Orchestrated);
    }

    // "auto" + simple agent -> Direct
    #[test]
    fn test_auto_heuristic_direct_on_simple_agent() {
        // GIVEN a manifest with execution_mode = "auto" and 1 tool
        let mut manifest = simple_manifest();
        manifest.execution_mode = "auto".to_string();
        manifest.tools_required = vec!["file_io".to_string()];
        let task = simple_task();

        // WHEN
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN: heuristic -> Direct
        assert_eq!(mode, ExecutionMode::Direct);
    }

    // simple agent classified Direct (score ~0.0)
    #[test]
    fn test_simple_agent_classified_direct() {
        // GIVEN an agent with 2 tools, 10 max steps, no tag, short input
        let manifest = simple_manifest();
        let task = simple_task();

        // WHEN
        let score = compute_complexity_score(&manifest, &task.input, None);
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN
        assert!(score < f32::EPSILON, "score should be ~0.0, got {score}");
        assert_eq!(mode, ExecutionMode::Direct);
    }

    // complex agent classified Orchestrated (score >= 0.9)
    #[test]
    fn test_complex_agent_classified_orchestrated() {
        // GIVEN an agent with 5+ tools, 20 steps, "multi-step" tag, long input
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

        // WHEN: steps(0.30) + parts(0) + tag(0.40) + tools(0.20) + input_len(0.10) = 1.0
        let score = compute_complexity_score(&manifest, &task.input, None);
        let mode = classify(&task, &manifest, None, ORCHESTRATED_THRESHOLD);

        // THEN
        assert!(score >= 0.90, "score should be >= 0.90, got {score}");
        assert_eq!(mode, ExecutionMode::Orchestrated);
    }

    // "multi-step" tag alone triggers Orchestrated
    #[test]
    fn test_multi_step_tag_alone_triggers_orchestrated() {
        // GIVEN an agent with 1 tool, 5 steps, "multi-step" tag, short input
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

        // THEN: score = WEIGHT_MULTI_STEP_TAG (0.40) >= 0.40
        assert!(score >= 0.40, "score should be >= 0.40, got {score}");
        assert_eq!(mode, ExecutionMode::Orchestrated);
    }

    // long input + many tools contribute to the score
    #[test]
    fn test_input_length_and_tools_contribute_to_score() {
        // GIVEN an agent with 6 tools, 10 steps, no tag, 800-char input
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

        // WHEN: tools(0.20) + input_len(0.10) = 0.30
        let score = compute_complexity_score(&manifest, &task.input, None);

        // THEN
        assert!(score >= 0.30, "score should be >= 0.30, got {score}");
    }

    // serde round-trip JSON -> execution_mode + system_prompt
    #[test]
    fn test_serde_round_trip() {
        use apollia_core::AgentManifest;

        // GIVEN a JSON with execution_mode and system_prompt
        let json = r#"{"name":"a","version":"1.0.0","description":"d","tools_required":[],"execution_mode":"orchestrated","system_prompt":"Planifie."}"#;

        // WHEN
        let manifest: AgentManifest = serde_json::from_str(json).expect("deserialize");

        // THEN
        assert_eq!(manifest.execution_mode, "orchestrated");
        assert_eq!(manifest.system_prompt, Some("Planifie.".to_string()));
    }

    // default value "auto" when the field is absent
    #[test]
    fn test_default_execution_mode_is_auto() {
        use apollia_core::AgentManifest;

        // GIVEN a minimal JSON without execution_mode
        let json = r#"{"name":"a","version":"1.0.0","description":"d","tools_required":[]}"#;

        // WHEN
        let manifest: AgentManifest = serde_json::from_str(json).expect("deserialize");

        // THEN
        assert_eq!(manifest.execution_mode, "auto");
        assert_eq!(manifest.system_prompt, None);
    }

    // orchestrated_threshold matches the calibrated config default
    #[test]
    fn test_orchestrated_threshold_matches_config_default() {
        // GIVEN the calibrated ORIA config default
        let from_config = apollia_core::ORIAConfig::default().orchestrated_threshold as f32;

        // WHEN reading the public threshold helper
        let from_helper = orchestrated_threshold();

        // THEN the helper returns the config value, not a duplicated literal
        assert!((from_helper - from_config).abs() < f32::EPSILON);
        assert!(from_helper > 0.0);
    }

    // a planning-intent turn scores at or above the threshold
    #[test]
    fn test_score_turn_text_planning_intent_is_substantive() {
        // GIVEN a multi-step actionable request with a planning cue
        let input = "Plan the migration: audit the repo then open a PR";

        // WHEN scoring the turn text
        let score = score_turn_text(input);

        // THEN the planning weight pushes it to or above the threshold
        assert!(score >= orchestrated_threshold(), "score was {score}");
    }

    // a trivial greeting scores below the threshold
    #[test]
    fn test_score_turn_text_trivial_is_below_threshold() {
        // GIVEN a short greeting with no planning cue
        let input = "hi there";

        // WHEN scoring the turn text
        let score = score_turn_text(input);

        // THEN it stays below the orchestration threshold
        assert!(score < orchestrated_threshold(), "score was {score}");
    }

    // an empty turn scores zero, no panic
    #[test]
    fn test_score_turn_text_empty_is_zero() {
        // GIVEN a blank input
        // WHEN scoring whitespace
        let score = score_turn_text("   ");

        // THEN the score is exactly 0.0
        assert!(score.abs() < f32::EPSILON);
    }
}
