//! ORIA Reasoner: produces a structured `ExecutionPlan` via an LLM call.
//!
//! The Reasoner is the central component of the ORIA pipeline for Orchestrated mode.
//! It receives a [`crate::observer::ContextBundle`] enriched by the Observer, calls
//! an LLM via `Arc<dyn CompletionModel>` and returns a validated
//! [`crate::plan::ExecutionPlan`].
//!
//! On an invalid response, a correction message is injected into the prompt and
//! the call is retried up to [`MAX_ATTEMPTS`] times (fail fast after N retries).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use apollia_core::decision_point::DecisionKind;
use apollia_core::events::{EventBusSender, RuntimeEvent};
use apollia_core::plan_alternatives::{PlanAlternatives, TaskPlan, TaskPlanStep};
use apollia_core::{AIPPart, ORIAConfig};
use apollia_llm::meta_orchestrator::{DecisionPointRequest, MetaOrchestratorHandle};
use apollia_llm::{ChatMessage, CompletionModel, CompletionRequest};

use crate::observer::ContextBundle;
use crate::plan::{ExecutionPlan, PlanStep};
use crate::topo::topological_sort;

// Error types

/// Validation errors for a single plan returned by the LLM.
///
/// Returned by [`Reasoner::parse_and_validate`]; wrapped in
/// [`ReasonerError::PlanParseError`] after the retries are exhausted.
#[derive(Debug, thiserror::Error)]
pub enum PlanValidationError {
    /// The LLM response is not valid JSON.
    #[error("Invalid JSON: {0}")]
    InvalidJson(String),
    /// The JSON structure does not match the expected `{{ "steps": [...] }}` schema.
    #[error("Invalid structure: {0}")]
    InvalidStructure(String),
    /// Several steps share the same `step_id`.
    #[error("Duplicate step IDs")]
    DuplicateStepIds,
    /// A step's `depends_on` references a `step_id` that does not exist in the plan.
    #[error("Unknown dependency: step '{step_id}' depends on unknown '{dep}'")]
    UnknownDependency {
        /// Identifier of the step containing the invalid reference.
        step_id: String,
        /// Identifier of the missing dependency.
        dep: String,
    },
    /// The dependencies form a cycle, topological execution is impossible.
    #[error("Circular dependency detected")]
    CircularDependency,
}

/// ORIA Reasoner errors.
#[derive(Debug, thiserror::Error)]
pub enum ReasonerError {
    /// An LLM call failed (network, timeout, backend unavailable, etc.).
    #[error("LLM call failed: {0}")]
    LlmFailed(String),
    /// JSON parse/validation failed after N consecutive attempts.
    #[error("Plan parse/validation failed after {attempts} attempts: {reason}")]
    PlanParseError {
        /// Number of attempts made before giving up.
        attempts: u32,
        /// Last validation error encountered.
        reason: String,
    },
}

// Constants

/// Maximum number of plan-generation attempts before giving up.
const MAX_ATTEMPTS: u32 = 3;

/// System prompt for initial planning.
///
/// The placeholders `{max_steps}`, `{tool_names}`, `{llm_backend_names}`,
/// `{memory_summary}` and `{recent_history}` are interpolated by
/// `build_system_prompt()` via `str::replace`.
const PLANNER_SYSTEM_PROMPT: &str = r#"Tu es un planificateur d'exécution pour un agent IA autonome.
À partir du contexte et du system_prompt de l'agent, génère un plan d'exécution structuré.

CONTRAINTES STRICTES :
- Maximum {max_steps} étapes
- Outils disponibles : {tool_names}
- Modèles LLM disponibles : {llm_backend_names}
- Chaque step_id doit être unique (s1, s2, s3...)
- Les depends_on ne peuvent référencer que des step_ids existants dans ce plan
- Pas de dépendances circulaires
- Optionnellement, spécifie model_hint pour choisir un backend LLM par step. Omets le champ pour utiliser le backend par défaut.

RÉPONDRE UNIQUEMENT EN JSON VALIDE, sans texte avant ou après :
{"steps": [{"step_id": "s1", "description": "Description claire de l'action", "tool_hint": "nom_outil_ou_llm", "model_hint": "fast-7b", "depends_on": []}]}

Contexte mémoire disponible : {memory_summary}
Historique récent : {recent_history}"#;

/// System prompt for partial replanning after a step failure.
///
/// The placeholders `{original_plan_json}`, `{completed_steps_json}`,
/// `{failed_step_id}` and `{error_message}` are interpolated by `replan()`.
const REPLANNER_SYSTEM_PROMPT: &str = r#"Le plan d'exécution a rencontré une erreur. Génère un plan alternatif.

Plan original : {original_plan_json}
Steps complétés avec succès : {completed_steps_json}
Step en échec : {failed_step_id} - erreur : {error_message}

Génère un nouveau plan pour les steps restants uniquement.
Réutilise les outputs des steps déjà complétés si pertinent.
RÉPONDRE UNIQUEMENT EN JSON VALIDE."#;

// Reasoner

/// Produces and validates [`ExecutionPlan`]s via an LLM call.
///
/// The `Reasoner` receives an injected `Arc<dyn CompletionModel>` (for testability)
/// and a `max_steps` bounding the size of generated plans (non-negotiable safety
/// guardrail).
///
/// On an invalid JSON response, the error message is injected into the next prompt
/// and the call is retried up to [`MAX_ATTEMPTS`] times.
pub struct Reasoner {
    model: Arc<dyn CompletionModel>,
    max_steps: u32,
    /// Optional EventBus to emit `ThinkingStarted` / `ThinkingEnded` around the
    /// Reasoner phase. Injected via [`Reasoner::with_event_bus`].
    event_bus: Option<EventBusSender>,
    /// Optional handle to the `MetaLlmOrchestrator`, used to extract the
    /// alternative branches from the thinking trace and emit a
    /// `DecisionPointRecorded`. Opt-in via the
    /// `routines.decision_branches` toggle.
    meta_orchestrator: Option<MetaOrchestratorHandle>,
}

impl Reasoner {
    /// Create a `Reasoner` with the injected LLM model and a maximum step budget.
    ///
    /// `max_steps` bounds the plan size the LLM is allowed to generate.
    /// It is typically derived from the agent's `StepBudget` via `from_capped()`.
    pub fn new(model: Arc<dyn CompletionModel>, max_steps: u32) -> Self {
        Self {
            model,
            max_steps,
            event_bus: None,
            meta_orchestrator: None,
        }
    }

    /// Attach an `EventBusSender` to emit the transparency events
    /// `ThinkingStarted` / `ThinkingEnded`.
    #[must_use]
    pub fn with_event_bus(mut self, bus: EventBusSender) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Attach a `MetaOrchestratorHandle` to enable extracting alternative branches
    /// from the thinking trace. Opt-in: the `GenerateAlternativeBranches` routine
    /// must be enabled in `MetaLlmSettings` (off by default). Without this handle,
    /// no `DecisionPointRecorded` is emitted.
    #[must_use]
    pub fn with_meta_orchestrator(mut self, handle: MetaOrchestratorHandle) -> Self {
        self.meta_orchestrator = Some(handle);
        self
    }

    /// Unix timestamp in milliseconds, used to time-stamp thinking events.
    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// Emit `ThinkingStarted` on the bus if configured, silent when the bus is absent.
    fn emit_thinking_started(&self, turn_id: &str) -> (u64, std::time::Instant) {
        let ts = Self::now_ms();
        let start = std::time::Instant::now();
        if let Some(bus) = &self.event_bus {
            let _ = bus.send(RuntimeEvent::ThinkingStarted {
                turn_id: turn_id.to_owned(),
                ts_ms: ts,
            });
        }
        (ts, start)
    }

    /// Emit `ThinkingEnded` with the raw content and consumed tokens.
    fn emit_thinking_ended(
        &self,
        turn_id: &str,
        start: std::time::Instant,
        raw_content: String,
        tokens: u32,
    ) {
        if let Some(bus) = &self.event_bus {
            let duration_ms = start.elapsed().as_millis() as u64;
            let _ = bus.send(RuntimeEvent::ThinkingEnded {
                turn_id: turn_id.to_owned(),
                ts_ms: Self::now_ms(),
                duration_ms,
                raw_content,
                tokens,
            });
        }
    }

    /// Extract alternative branches from the thinking trace via the meta routine
    /// `GenerateAlternativeBranches`, then emit `DecisionPointRecorded`.
    ///
    /// No-op when:
    /// - no `MetaOrchestratorHandle` is wired,
    /// - no `EventBusSender` is wired,
    /// - the routine is disabled / budget exhausted / timeout / parse fails.
    ///
    /// Kind used: [`DecisionKind::ToolChoice`], the root step of a plan represents
    /// the main tool choice of this turn.
    async fn maybe_emit_decision_point(
        &self,
        turn_id: &str,
        thinking_raw: &str,
        plan: &ExecutionPlan,
    ) {
        let Some(orchestrator) = &self.meta_orchestrator else {
            return;
        };
        let Some(bus) = &self.event_bus else {
            return;
        };
        let chosen = plan
            .steps
            .first()
            .map(|s| s.tool_hint.clone().unwrap_or_else(|| s.description.clone()))
            .unwrap_or_else(|| "(no step)".to_owned());

        if let Some(point) = orchestrator
            .generate_decision_point(
                DecisionPointRequest {
                    turn_id,
                    kind: DecisionKind::ToolChoice,
                    thinking_raw,
                    chosen_action: &chosen,
                },
                turn_id,
            )
            .await
        {
            let _ = bus.send(RuntimeEvent::DecisionPointRecorded { point });
        }
    }

    /// Generate an initial execution plan from the [`ContextBundle`].
    ///
    /// Delegates to [`plan_internal`] with the default temperature (`None`).
    /// Returns [`ReasonerError::PlanParseError`] after [`MAX_ATTEMPTS`] attempts.
    pub async fn plan(&self, ctx: &ContextBundle) -> Result<ExecutionPlan, ReasonerError> {
        self.plan_internal(ctx, None, None).await
    }

    /// Generate a plan that incorporates rejection feedback from a prior attempt.
    ///
    /// The `previous_plan_id` and optional `feedback` are injected into the user
    /// prompt to steer the new plan away from the rejected one. Uses the same
    /// temperature and retry policy as [`Self::plan`].
    ///
    /// # Errors
    ///
    /// Returns [`ReasonerError`] when planning fails after the internal retries.
    pub async fn plan_with_feedback(
        &self,
        ctx: &ContextBundle,
        previous_plan_id: &str,
        feedback: Option<&str>,
    ) -> Result<ExecutionPlan, ReasonerError> {
        let note = match feedback {
            Some(fb) => format!(
                "Le plan précédent (identifiant {previous_plan_id}) a été rejeté par l'opérateur.\nRetour : {fb}\nGénère un nouveau plan qui corrige ce point et renvoie uniquement du JSON valide."
            ),
            None => format!(
                "Le plan précédent (identifiant {previous_plan_id}) a été rejeté par l'opérateur sans précision.\nGénère un nouveau plan sensiblement différent et renvoie uniquement du JSON valide."
            ),
        };
        self.plan_internal(ctx, None, Some(&note)).await
    }

    /// Generate two alternative plans in parallel via `tokio::join!`.
    ///
    /// Plan A is produced at `config.plan_alternatives_temp_a` (low temperature:
    /// deterministic, conservative). Plan B is produced at `config.plan_alternatives_temp_b`
    /// (high temperature: creative, exploratory).
    ///
    /// Both LLM calls are concurrent: total duration is roughly 1 call, not 2.
    /// Returns [`ReasonerError`] if either plan is invalid after retries.
    pub async fn plan_with_alternatives(
        &self,
        ctx: &ContextBundle,
        config: &ORIAConfig,
    ) -> Result<PlanAlternatives, ReasonerError> {
        let session_id = uuid::Uuid::new_v4().to_string();

        let (plan_a_result, plan_b_result) = tokio::join!(
            self.plan_internal(ctx, Some(config.plan_alternatives_temp_a), None),
            self.plan_internal(ctx, Some(config.plan_alternatives_temp_b), None),
        );

        let plan_a = plan_a_result.map(execution_plan_to_task_plan)?;
        let plan_b = plan_b_result.map(execution_plan_to_task_plan)?;

        let generated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        tracing::info!(
            session_id = %session_id,
            steps_a = plan_a.steps.len(),
            steps_b = plan_b.steps.len(),
            temp_a = config.plan_alternatives_temp_a,
            temp_b = config.plan_alternatives_temp_b,
            "plan alternatives generated"
        );

        Ok(PlanAlternatives {
            plan_a,
            plan_b,
            session_id,
            generated_at,
        })
    }

    /// Generate an execution plan with an explicit LLM temperature.
    ///
    /// Shared implementation used by [`plan`] and [`plan_with_alternatives`].
    /// Applies up to [`MAX_ATTEMPTS`] retries on invalid JSON.
    ///
    /// `temperature` is passed to [`CompletionRequest`] when `Some`; the backend
    /// uses its default value when `None`.
    async fn plan_internal(
        &self,
        ctx: &ContextBundle,
        temperature: Option<f32>,
        feedback: Option<&str>,
    ) -> Result<ExecutionPlan, ReasonerError> {
        let mut last_error = String::new();
        let turn_id = ctx.task.task_id.as_str().to_owned();
        let (_start_ts, start_instant) = self.emit_thinking_started(&turn_id);
        let mut total_tokens: u32 = 0;
        let mut last_raw = String::new();

        for attempt in 0..MAX_ATTEMPTS {
            let system = self.build_system_prompt(ctx);
            let base_user = self.build_user_prompt(ctx);
            let user = if attempt == 0 {
                match feedback {
                    Some(note) => format!("{base_user}\n\n{note}"),
                    None => base_user,
                }
            } else {
                format!(
                    "{}\n\nATTENTION : ta réponse précédente était invalide.\nErreur : {}\nCorrige et renvoie uniquement du JSON valide.",
                    base_user, last_error
                )
            };

            let response = match self
                .model
                .complete(CompletionRequest {
                    messages: vec![ChatMessage::system(system), ChatMessage::user(user)],
                    temperature,
                    ..Default::default()
                })
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    self.emit_thinking_ended(
                        &turn_id,
                        start_instant,
                        last_raw.clone(),
                        total_tokens,
                    );
                    return Err(ReasonerError::LlmFailed(e.to_string()));
                }
            };

            total_tokens = total_tokens.saturating_add(response.usage.completion_tokens);
            last_raw.clone_from(&response.content);

            match self.parse_and_validate(&response.content, &ctx.task.task_id) {
                Ok(plan) => {
                    tracing::info!(
                        attempt = attempt + 1,
                        steps = plan.steps.len(),
                        "ExecutionPlan ready"
                    );
                    self.emit_thinking_ended(
                        &turn_id,
                        start_instant,
                        last_raw.clone(),
                        total_tokens,
                    );
                    self.maybe_emit_decision_point(&turn_id, &last_raw, &plan)
                        .await;
                    return Ok(plan);
                }
                Err(e) => {
                    tracing::warn!(attempt = attempt + 1, error = %e, "plan invalide, retry");
                    last_error = e.to_string();
                }
            }
        }

        self.emit_thinking_ended(&turn_id, start_instant, last_raw, total_tokens);
        Err(ReasonerError::PlanParseError {
            attempts: MAX_ATTEMPTS,
            reason: last_error,
        })
    }

    /// Partial replanning after a step failure.
    ///
    /// Generates only the remaining steps, accounting for the completed steps'
    /// outputs (`completed_outputs`) and the failure reason of `failed_step_id`.
    ///
    /// A single LLM call without its own retry: replan retry management is the
    /// `ActorLoop`'s responsibility.
    pub async fn replan(
        &self,
        ctx: &ContextBundle,
        completed_outputs: &HashMap<String, String>,
        failed_step_id: &str,
        error_message: &str,
    ) -> Result<ExecutionPlan, ReasonerError> {
        let original_plan_json = serde_json::to_string(&ctx.task).unwrap_or_default();
        let completed_steps_json = serde_json::to_string(completed_outputs).unwrap_or_default();

        let system = REPLANNER_SYSTEM_PROMPT
            .replace("{original_plan_json}", &original_plan_json)
            .replace("{completed_steps_json}", &completed_steps_json)
            .replace("{failed_step_id}", failed_step_id)
            .replace("{error_message}", error_message);

        let response = self
            .model
            .complete(CompletionRequest {
                messages: vec![ChatMessage::system(system)],
                ..Default::default()
            })
            .await
            .map_err(|e| ReasonerError::LlmFailed(e.to_string()))?;

        self.parse_and_validate(&response.content, &ctx.task.task_id)
            .map_err(|e| ReasonerError::PlanParseError {
                attempts: 1,
                reason: e.to_string(),
            })
    }

    /// Validate a raw JSON plan returned by the LLM.
    ///
    /// 5 sequential validations, returning `Err` at the first problem encountered:
    /// 1. Strip any Markdown backticks and parse the JSON
    /// 2. Deserialize the [`PlanStep`] array from the `"steps"` field
    /// 3. Check that all `step_id`s are unique
    /// 4. Check that each `depends_on` references a `step_id` present in the plan
    /// 5. Detect cycles via [`topological_sort`] (Kahn's algorithm)
    ///
    /// This function is public and pure, testable independently of the LLM.
    pub fn parse_and_validate(
        &self,
        raw: &str,
        task_id: &str,
    ) -> Result<ExecutionPlan, PlanValidationError> {
        // 1. Strip Markdown backticks and trim whitespace
        let clean = raw
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        // 2. Parse JSON and extract steps array
        let parsed: serde_json::Value = serde_json::from_str(clean)
            .map_err(|e| PlanValidationError::InvalidJson(e.to_string()))?;

        let steps: Vec<PlanStep> = serde_json::from_value(parsed["steps"].clone())
            .map_err(|e| PlanValidationError::InvalidStructure(e.to_string()))?;

        // 3. Validate unique step_ids
        let ids: HashSet<&str> = steps.iter().map(|s| s.step_id.as_str()).collect();
        if ids.len() != steps.len() {
            return Err(PlanValidationError::DuplicateStepIds);
        }

        // 4. Validate all depends_on reference existing step_ids
        for step in &steps {
            for dep in &step.depends_on {
                if !ids.contains(dep.as_str()) {
                    return Err(PlanValidationError::UnknownDependency {
                        step_id: step.step_id.clone(),
                        dep: dep.clone(),
                    });
                }
            }
        }

        // 5. Detect cycles via topological sort (Kahn BFS)
        topological_sort(&steps).map_err(|_| PlanValidationError::CircularDependency)?;

        Ok(ExecutionPlan {
            plan_id: uuid::Uuid::new_v4().to_string(),
            task_id: task_id.to_string(),
            steps,
        })
    }

    fn build_system_prompt(&self, ctx: &ContextBundle) -> String {
        let tool_names = if ctx.available_tools.is_empty() {
            "Aucun outil disponible.".to_string()
        } else {
            ctx.available_tools.join(", ")
        };

        let memory_summary = ctx.memory_snapshot.as_ref().map_or_else(
            || "Aucun contexte mémoriel disponible.".to_string(),
            |m| {
                let episodes = if m.episodic_recent.is_empty() {
                    "aucun".to_string()
                } else {
                    m.episodic_recent.join("; ")
                };
                let facts = if m.semantic_relevant.is_empty() {
                    "aucun".to_string()
                } else {
                    m.semantic_relevant
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                format!("Épisodes: {episodes}. Faits: {facts}")
            },
        );

        let llm_backend_names = if ctx.llm_backend_names.is_empty() {
            "Aucun modèle LLM disponible.".to_string()
        } else {
            ctx.llm_backend_names.join(", ")
        };

        PLANNER_SYSTEM_PROMPT
            .replace("{max_steps}", &self.max_steps.to_string())
            .replace("{tool_names}", &tool_names)
            .replace("{llm_backend_names}", &llm_backend_names)
            .replace("{memory_summary}", &memory_summary)
            .replace("{recent_history}", "")
    }

    fn build_user_prompt(&self, ctx: &ContextBundle) -> String {
        let system_prompt = ctx
            .manifest_system_prompt
            .as_deref()
            .unwrap_or("Exécute la tâche demandée de manière optimale.");

        let task_input = ctx
            .task
            .input
            .parts
            .iter()
            .filter_map(|p| {
                if let AIPPart::Text(t) = p {
                    Some(t.text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!("Agent system prompt:\n{system_prompt}\n\nTâche: {task_input}")
    }
}

// ─────────────────────────────────────────────
// Conversion helpers
// ─────────────────────────────────────────────

/// Converts an [`ExecutionPlan`] (internal ORIA type) to the shared [`TaskPlan`] type.
///
/// Used by [`Reasoner::plan_with_alternatives`] to produce values that can be carried
/// by `RuntimeEvent::PlanAlternativesGenerated` without creating a circular dependency
/// between `apollia-oria` and the crates that consume events.
fn execution_plan_to_task_plan(plan: ExecutionPlan) -> TaskPlan {
    TaskPlan {
        plan_id: plan.plan_id,
        task_id: plan.task_id,
        steps: plan
            .steps
            .into_iter()
            .map(|s| TaskPlanStep {
                step_id: s.step_id,
                description: s.description,
                tool_hint: s.tool_hint,
                depends_on: s.depends_on,
                model_hint: s.model_hint,
            })
            .collect(),
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_llm::{
        CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk, TokenUsage,
    };
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    // Mock LLM

    /// Mock `CompletionModel` for Reasoner tests.
    ///
    /// Consumes `queue` responses in order; uses `fallback` once the queue is
    /// exhausted. Counts the number of calls via `call_count`.
    struct MockCompletionModel {
        queue: Mutex<VecDeque<String>>,
        fallback: String,
        call_count: AtomicU32,
    }

    impl MockCompletionModel {
        fn sequence(responses: Vec<&str>) -> Arc<Self> {
            Arc::new(Self {
                queue: Mutex::new(responses.iter().map(|s| s.to_string()).collect()),
                fallback: "{}".to_string(),
                call_count: AtomicU32::new(0),
            })
        }

        fn calls(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockCompletionModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let content = {
                let mut q = self.queue.lock().expect("mock lock poisoned");
                q.pop_front().unwrap_or_else(|| self.fallback.clone())
            };
            Ok(CompletionResponse {
                content,
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    cost_usd: None,
                    ..Default::default()
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 0,
                ttft_ms: None,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<
            Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>,
            LlmError,
        > {
            Err(LlmError::InferenceError(
                "mock does not support streaming".into(),
            ))
        }

        fn is_available(&self) -> bool {
            true
        }

        fn backend_name(&self) -> &str {
            "mock"
        }

        fn model_id(&self) -> &str {
            "mock-model"
        }
    }

    // ─── JSON fixtures ────────────────────────

    const VALID_PLAN_2_STEPS: &str = r#"{"steps":[
        {"step_id":"s1","description":"Step 1","tool_hint":"file_io","depends_on":[]},
        {"step_id":"s2","description":"Step 2","tool_hint":"llm","depends_on":["s1"]}
    ]}"#;

    const VALID_PLAN_3_STEPS: &str = r#"{"steps":[
        {"step_id":"s1","description":"Step 1","depends_on":[]},
        {"step_id":"s2","description":"Step 2","depends_on":["s1"]},
        {"step_id":"s3","description":"Step 3","depends_on":["s2"]}
    ]}"#;

    const CYCLIC_PLAN: &str = r#"{"steps":[
        {"step_id":"s1","description":"Step 1","depends_on":["s2"]},
        {"step_id":"s2","description":"Step 2","depends_on":["s1"]}
    ]}"#;

    // ─── Plan valide depuis mock LLM ───

    /// GIVEN a Reasoner with a mock CompletionModel returning valid JSON
    /// WHEN reasoner.plan(&ctx).await is called
    /// THEN Ok(ExecutionPlan) is returned with 2 steps
    ///   AND the mock was called exactly once
    #[tokio::test]
    async fn test_plan_valide_depuis_mock_llm() {
        // GIVEN
        let model = MockCompletionModel::sequence(vec![VALID_PLAN_2_STEPS]);
        let reasoner = Reasoner::new(model.clone(), 10);
        let ctx = ContextBundle::default();

        // WHEN
        let result = reasoner.plan(&ctx).await;

        // THEN
        let plan = result.expect("expected Ok(ExecutionPlan)");
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].step_id, "s1");
        assert_eq!(plan.steps[1].depends_on, vec!["s1"]);
        assert_eq!(model.calls(), 1);
    }

    // Retry x3 on invalid JSON

    /// GIVEN a mock returning non-JSON text 3 times
    /// WHEN reasoner.plan(&ctx).await is called
    /// THEN Err(PlanParseError { attempts: 3 }) is returned
    ///   AND the mock was called exactly 3 times
    #[tokio::test]
    async fn test_retry_3_fois_sur_json_invalide() {
        // GIVEN
        let model = MockCompletionModel::sequence(vec!["not json", "still not json", "nope"]);
        let reasoner = Reasoner::new(model.clone(), 10);
        let ctx = ContextBundle::default();

        // WHEN
        let result = reasoner.plan(&ctx).await;

        // THEN
        assert!(
            matches!(
                result,
                Err(ReasonerError::PlanParseError { attempts: 3, .. })
            ),
            "expected PlanParseError{{attempts:3}}, got: {result:?}"
        );
        assert_eq!(model.calls(), 3);
    }

    // Circular dependency detected: retry

    /// GIVEN a mock returning a cyclic plan 3 times
    /// WHEN reasoner.plan(&ctx).await is called
    /// THEN PlanParseError after 3 attempts
    #[tokio::test]
    async fn test_cycle_detecte_et_retry() {
        // GIVEN
        let model = MockCompletionModel::sequence(vec![CYCLIC_PLAN, CYCLIC_PLAN, CYCLIC_PLAN]);
        let reasoner = Reasoner::new(model.clone(), 10);
        let ctx = ContextBundle::default();

        // WHEN
        let result = reasoner.plan(&ctx).await;

        // THEN
        assert!(
            matches!(
                result,
                Err(ReasonerError::PlanParseError { attempts: 3, .. })
            ),
            "expected PlanParseError{{attempts:3}}, got: {result:?}"
        );
        assert_eq!(model.calls(), 3);
    }

    // continued: cycle then valid plan

    /// GIVEN a mock returning a cyclic plan then a valid plan
    /// WHEN reasoner.plan(&ctx).await is called
    /// THEN Ok(ExecutionPlan) on the 2nd attempt
    #[tokio::test]
    async fn test_cycle_puis_plan_valide() {
        // GIVEN
        let model = MockCompletionModel::sequence(vec![CYCLIC_PLAN, VALID_PLAN_2_STEPS]);
        let reasoner = Reasoner::new(model.clone(), 10);
        let ctx = ContextBundle::default();

        // WHEN
        let result = reasoner.plan(&ctx).await;

        // THEN
        assert!(result.is_ok());
        assert_eq!(model.calls(), 2);
    }

    // replan() generates a partial plan

    /// GIVEN a mock returning a valid replacement plan
    /// WHEN reasoner.replan(&ctx, &outputs, "s3", "timeout").await is called
    /// THEN Ok(ExecutionPlan) with the new steps
    #[tokio::test]
    async fn test_replan_retourne_plan_valide() {
        // GIVEN
        let replacement_plan = r#"{"steps":[
            {"step_id":"s3b","description":"Retry step","depends_on":[]},
            {"step_id":"s4","description":"Final step","depends_on":["s3b"]}
        ]}"#;
        let model = MockCompletionModel::sequence(vec![replacement_plan]);
        let reasoner = Reasoner::new(model.clone(), 10);
        let ctx = ContextBundle::default();
        let mut completed_outputs = HashMap::new();
        completed_outputs.insert("s1".to_string(), "output1".to_string());
        completed_outputs.insert("s2".to_string(), "output2".to_string());

        // WHEN
        let result = reasoner
            .replan(&ctx, &completed_outputs, "s3", "timeout")
            .await;

        // THEN
        let plan = result.expect("expected Ok(ExecutionPlan)");
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].step_id, "s3b");
        assert_eq!(model.calls(), 1);
    }

    // Markdown backticks stripped

    /// GIVEN an LLM response with Markdown backticks
    /// WHEN parse_and_validate() is called
    /// THEN the JSON is parsed correctly
    #[test]
    fn test_backticks_strippes() {
        // GIVEN
        let reasoner = Reasoner::new(MockCompletionModel::sequence(vec![]), 10);
        let raw = "```json\n{\"steps\":[{\"step_id\":\"s1\",\"description\":\"d\",\"depends_on\":[]}]}\n```";

        // WHEN
        let result = reasoner.parse_and_validate(raw, "task-001");

        // THEN
        assert!(result.is_ok());
        assert_eq!(result.unwrap().steps[0].step_id, "s1");
    }

    // Direct validation of parse_and_validate

    /// GIVEN a plan with a dependency on a nonexistent step
    /// WHEN parse_and_validate() is called
    /// THEN Err(UnknownDependency)
    #[test]
    fn test_unknown_dependency() {
        // GIVEN
        let reasoner = Reasoner::new(MockCompletionModel::sequence(vec![]), 10);
        let raw = r#"{"steps":[{"step_id":"s1","description":"d","depends_on":["s99"]}]}"#;

        // WHEN
        let result = reasoner.parse_and_validate(raw, "task-001");

        // THEN
        assert!(
            matches!(result, Err(PlanValidationError::UnknownDependency { .. })),
            "expected UnknownDependency, got: {result:?}"
        );
    }

    /// GIVEN a plan with two steps sharing the same step_id
    /// WHEN parse_and_validate() is called
    /// THEN Err(DuplicateStepIds)
    #[test]
    fn test_duplicate_step_ids() {
        // GIVEN
        let reasoner = Reasoner::new(MockCompletionModel::sequence(vec![]), 10);
        let raw = r#"{"steps":[
            {"step_id":"s1","description":"d","depends_on":[]},
            {"step_id":"s1","description":"e","depends_on":[]}
        ]}"#;

        // WHEN
        let result = reasoner.parse_and_validate(raw, "task-001");

        // THEN
        assert!(
            matches!(result, Err(PlanValidationError::DuplicateStepIds)),
            "expected DuplicateStepIds, got: {result:?}"
        );
    }

    /// GIVEN a linear 3-step plan (s1->s2->s3)
    /// WHEN parse_and_validate() is called
    /// THEN Ok(ExecutionPlan) with 3 steps and the correct task_id
    #[test]
    fn test_parse_and_validate_plan_lineaire() {
        // GIVEN
        let reasoner = Reasoner::new(MockCompletionModel::sequence(vec![]), 10);

        // WHEN
        let result = reasoner.parse_and_validate(VALID_PLAN_3_STEPS, "task-abc");

        // THEN
        let plan = result.expect("expected Ok");
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.task_id, "task-abc");
        assert!(!plan.plan_id.is_empty());
    }

    // Deserialization without model_hint

    /// GIVEN a PlanStep JSON without a model_hint field
    /// WHEN deserializing
    /// THEN model_hint == None (serde default)
    #[test]
    fn test_deserialize_plan_without_model_hint() {
        // GIVEN
        let json = r#"{"steps":[
            {"step_id":"s1","description":"Step 1","tool_hint":"file_io","depends_on":[]}
        ]}"#;

        // WHEN
        let parsed: serde_json::Value = serde_json::from_str(json).expect("valid JSON");
        let steps: Vec<PlanStep> =
            serde_json::from_value(parsed["steps"].clone()).expect("valid steps");

        // THEN
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].model_hint, None);
    }

    // Deserialization with model_hint

    /// GIVEN a PlanStep JSON with "model_hint": "fast-7b"
    /// WHEN deserializing
    /// THEN model_hint == Some("fast-7b")
    #[test]
    fn test_deserialize_plan_with_model_hint() {
        // GIVEN
        let json = r#"{"steps":[
            {"step_id":"s1","description":"Step 1","tool_hint":"llm","model_hint":"fast-7b","depends_on":[]}
        ]}"#;

        // WHEN
        let parsed: serde_json::Value = serde_json::from_str(json).expect("valid JSON");
        let steps: Vec<PlanStep> =
            serde_json::from_value(parsed["steps"].clone()).expect("valid steps");

        // THEN
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].model_hint, Some("fast-7b".to_string()));
    }

    // ─── Prompt contient {llm_backend_names} ───

    /// GIVEN la constante PLANNER_SYSTEM_PROMPT
    /// WHEN on inspecte son contenu
    /// THEN il contient "{llm_backend_names}" et une instruction sur model_hint
    #[test]
    fn test_prompt_contains_llm_backend_names_placeholder() {
        // GIVEN / WHEN / THEN
        assert!(
            PLANNER_SYSTEM_PROMPT.contains("{llm_backend_names}"),
            "PLANNER_SYSTEM_PROMPT must contain {{llm_backend_names}} placeholder"
        );
        assert!(
            PLANNER_SYSTEM_PROMPT.contains("model_hint"),
            "PLANNER_SYSTEM_PROMPT must mention model_hint"
        );
    }

    // Two plans generated in parallel

    /// GIVEN a Reasoner with a mock CompletionModel providing two valid plans
    /// WHEN plan_with_alternatives(&ctx, &config) is called
    /// THEN PlanAlternatives is returned with non-empty plan_a and plan_b
    ///   AND the mock was called exactly twice (one per plan)
    #[tokio::test]
    async fn test_plan_with_alternatives_generates_two_plans() {
        // GIVEN
        let model = MockCompletionModel::sequence(vec![VALID_PLAN_2_STEPS, VALID_PLAN_3_STEPS]);
        let reasoner = Reasoner::new(model.clone(), 10);
        let ctx = ContextBundle::default();
        let config = apollia_core::ORIAConfig::default();

        // WHEN
        let result = reasoner.plan_with_alternatives(&ctx, &config).await;

        // THEN
        let alts = result.expect("expected Ok(PlanAlternatives)");
        assert_eq!(alts.plan_a.steps.len(), 2, "plan_a should have 2 steps");
        assert_eq!(alts.plan_b.steps.len(), 3, "plan_b should have 3 steps");
        assert!(
            !alts.session_id.is_empty(),
            "session_id should be a non-empty UUID"
        );
        assert_eq!(
            model.calls(),
            2,
            "mock should have been called exactly twice"
        );
    }

    // ThinkingStarted / ThinkingEnded emission

    /// GIVEN a Reasoner wired to an EventBus and a mock returning a valid plan
    /// WHEN reasoner.plan(&ctx).await is called
    /// THEN exactly one `ThinkingStarted` followed by one `ThinkingEnded` are emitted
    ///   AND the events' `turn_id` matches the ContextBundle's `task_id`
    ///   AND `ThinkingEnded.raw_content` carries the last LLM content
    #[tokio::test]
    async fn emits_thinking_started_and_ended_on_success() {
        use apollia_core::events::RuntimeEvent;
        use tokio::sync::broadcast;

        // GIVEN
        let model = MockCompletionModel::sequence(vec![VALID_PLAN_2_STEPS]);
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);
        let reasoner = Reasoner::new(model, 10).with_event_bus(tx);
        let mut ctx = ContextBundle::default();
        ctx.task.task_id = "turn-xyz".into();

        // WHEN
        let plan = reasoner.plan(&ctx).await.expect("plan ok");
        assert_eq!(plan.steps.len(), 2);

        // THEN: ThinkingStarted then ThinkingEnded with the correct turn_id
        let started = rx.recv().await.expect("started");
        match started {
            RuntimeEvent::ThinkingStarted { turn_id, ts_ms } => {
                assert_eq!(turn_id, "turn-xyz");
                assert!(ts_ms > 0);
            }
            other => panic!("expected ThinkingStarted, got {other:?}"),
        }
        let ended = rx.recv().await.expect("ended");
        match ended {
            RuntimeEvent::ThinkingEnded {
                turn_id,
                raw_content,
                ..
            } => {
                assert_eq!(turn_id, "turn-xyz");
                assert!(raw_content.contains("steps"));
            }
            other => panic!("expected ThinkingEnded, got {other:?}"),
        }
    }

    /// GIVEN a Reasoner wired to an EventBus and a mock returning invalid JSON
    /// WHEN reasoner.plan(&ctx).await fails after MAX_ATTEMPTS
    /// THEN `ThinkingEnded` is still emitted (phase-end guarantee)
    #[tokio::test]
    async fn emits_thinking_ended_on_parse_failure() {
        use apollia_core::events::RuntimeEvent;
        use tokio::sync::broadcast;

        let model = MockCompletionModel::sequence(vec!["not json", "nope", "still no"]);
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);
        let reasoner = Reasoner::new(model, 10).with_event_bus(tx);
        let ctx = ContextBundle::default();

        let _ = reasoner.plan(&ctx).await;

        // Drain until we see ThinkingEnded: there must be exactly one.
        let mut saw_started = false;
        let mut saw_ended = false;
        while let Ok(evt) = rx.try_recv() {
            match evt {
                RuntimeEvent::ThinkingStarted { .. } => saw_started = true,
                RuntimeEvent::ThinkingEnded { .. } => saw_ended = true,
                _ => {}
            }
        }
        assert!(saw_started, "ThinkingStarted should have been emitted");
        assert!(saw_ended, "ThinkingEnded must fire on failure path too");
    }

    /// GIVEN a mock providing plans whose steps have descriptions
    /// WHEN plan_with_alternatives() is called
    /// THEN the descriptions are correctly converted into TaskPlanStep
    #[tokio::test]
    async fn test_plan_alternatives_step_descriptions_preserved() {
        // GIVEN
        let model = MockCompletionModel::sequence(vec![VALID_PLAN_2_STEPS, VALID_PLAN_2_STEPS]);
        let reasoner = Reasoner::new(model.clone(), 10);
        let ctx = ContextBundle::default();
        let config = apollia_core::ORIAConfig::default();

        // WHEN
        let alts = reasoner
            .plan_with_alternatives(&ctx, &config)
            .await
            .expect("expected Ok");

        // THEN
        assert_eq!(alts.plan_a.steps[0].step_id, "s1");
        assert_eq!(alts.plan_b.steps[0].step_id, "s1");
    }
}
