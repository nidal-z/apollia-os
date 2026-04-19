//! ORIA Reasoner — produit un `ExecutionPlan` structuré via un appel LLM.
//!
//! Le Reasoner est le composant central du pipeline ORIA pour le mode Orchestrated.
//! Il reçoit un [`crate::observer::ContextBundle`] enrichi par l'Observer, appelle
//! un LLM via `Arc<dyn CompletionModel>` et retourne un [`crate::plan::ExecutionPlan`]
//! validé.
//!
//! En cas de réponse invalide, un message de correction est injecté dans le prompt
//! et l'appel est retenté jusqu'à [`MAX_ATTEMPTS`] fois (principe #4 — Fail fast
//! après N retries).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use apollia_core::decision_point::DecisionKind;
use apollia_core::events::{EventBusSender, RuntimeEvent};
use apollia_core::plan_alternatives::{PlanAlternatives, TaskPlan, TaskPlanStep};
use apollia_core::{AIPPart, ORIAConfig};
use apollia_llm::meta_orchestrator::MetaOrchestratorHandle;
use apollia_llm::{ChatMessage, CompletionModel, CompletionRequest};

use crate::observer::ContextBundle;
use crate::plan::{ExecutionPlan, PlanStep};
use crate::topo::topological_sort;

// ─────────────────────────────────────────────
// Error types
// ─────────────────────────────────────────────

/// Erreurs de validation d'un plan individuel retourné par le LLM.
///
/// Retournées par [`Reasoner::parse_and_validate`] ; encapsulées dans
/// [`ReasonerError::PlanParseError`] après épuisement des tentatives.
#[derive(Debug, thiserror::Error)]
pub enum PlanValidationError {
    /// La réponse du LLM n'est pas du JSON valide.
    #[error("Invalid JSON: {0}")]
    InvalidJson(String),
    /// La structure JSON ne correspond pas au schéma `{{ "steps": [...] }}` attendu.
    #[error("Invalid structure: {0}")]
    InvalidStructure(String),
    /// Plusieurs steps partagent le même `step_id`.
    #[error("Duplicate step IDs")]
    DuplicateStepIds,
    /// Un step référence dans ses `depends_on` un `step_id` inexistant dans le plan.
    #[error("Unknown dependency: step '{step_id}' depends on unknown '{dep}'")]
    UnknownDependency {
        /// Identifiant du step qui contient la référence invalide.
        step_id: String,
        /// Identifiant de la dépendance introuvable.
        dep: String,
    },
    /// Les dépendances forment un cycle — exécution topologique impossible.
    #[error("Circular dependency detected")]
    CircularDependency,
}

/// Erreurs du Reasoner ORIA.
#[derive(Debug, thiserror::Error)]
pub enum ReasonerError {
    /// Échec d'un appel LLM (réseau, timeout, backend indisponible…).
    #[error("LLM call failed: {0}")]
    LlmFailed(String),
    /// Échec de parsage/validation JSON après N tentatives consécutives.
    #[error("Plan parse/validation failed after {attempts} attempts: {reason}")]
    PlanParseError {
        /// Nombre de tentatives effectuées avant l'abandon.
        attempts: u32,
        /// Dernière erreur de validation rencontrée.
        reason: String,
    },
}

// ─────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────

/// Nombre maximum de tentatives de génération de plan avant abandon.
const MAX_ATTEMPTS: u32 = 3;

/// Prompt système pour la planification initiale.
///
/// Les placeholders `{max_steps}`, `{tool_names}`, `{llm_backend_names}`,
/// `{memory_summary}` et `{recent_history}` sont interpolés par
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

/// Prompt système pour la replanification partielle après l'échec d'un step.
///
/// Les placeholders `{original_plan_json}`, `{completed_steps_json}`,
/// `{failed_step_id}` et `{error_message}` sont interpolés par `replan()`.
const REPLANNER_SYSTEM_PROMPT: &str = r#"Le plan d'exécution a rencontré une erreur. Génère un plan alternatif.

Plan original : {original_plan_json}
Steps complétés avec succès : {completed_steps_json}
Step en échec : {failed_step_id} — erreur : {error_message}

Génère un nouveau plan pour les steps restants uniquement.
Réutilise les outputs des steps déjà complétés si pertinent.
RÉPONDRE UNIQUEMENT EN JSON VALIDE."#;

// ─────────────────────────────────────────────
// Reasoner
// ─────────────────────────────────────────────

/// Produit et valide des [`ExecutionPlan`] via un appel LLM.
///
/// Le `Reasoner` reçoit un `Arc<dyn CompletionModel>` injecté (pattern ADR-016
/// pour la testabilité) et un `max_steps` bornant la taille des plans générés
/// (principe #7 — Garde-fous non-négociables).
///
/// En cas de réponse JSON invalide, le message d'erreur est injecté dans le prompt
/// suivant et l'appel est retenté jusqu'à [`MAX_ATTEMPTS`] fois.
pub struct Reasoner {
    model: Arc<dyn CompletionModel>,
    max_steps: u32,
    /// Optional EventBus pour émettre `ThinkingStarted` / `ThinkingEnded` autour
    /// de la phase Reasoner (US-SP42-037). Injecté via [`Reasoner::with_event_bus`].
    event_bus: Option<EventBusSender>,
    /// Optional handle vers le `MetaLlmOrchestrator` — utilisé pour extraire
    /// les branches alternatives de la trace de thinking et émettre un
    /// `DecisionPointRecorded` (US-SP42-041, Pattern P5). Opt-in via le
    /// toggle `routines.decision_branches`.
    meta_orchestrator: Option<MetaOrchestratorHandle>,
}

impl Reasoner {
    /// Crée un `Reasoner` avec le modèle LLM injecté et un budget maximum de steps.
    ///
    /// `max_steps` borne la taille du plan que le LLM est autorisé à générer.
    /// Il est typiquement dérivé du `StepBudget` de l'agent via `from_capped()`.
    pub fn new(model: Arc<dyn CompletionModel>, max_steps: u32) -> Self {
        Self {
            model,
            max_steps,
            event_bus: None,
            meta_orchestrator: None,
        }
    }

    /// Attache un `EventBusSender` pour émettre les événements de transparence
    /// `ThinkingStarted` / `ThinkingEnded` (US-SP42-037).
    #[must_use]
    pub fn with_event_bus(mut self, bus: EventBusSender) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Attache un `MetaOrchestratorHandle` pour activer l'extraction des
    /// branches alternatives du thinking (US-SP42-041). Opt-in : la routine
    /// `GenerateAlternativeBranches` doit être activée dans `MetaLlmSettings`
    /// (par défaut off). Sans ce handle, aucun `DecisionPointRecorded` n'est émis.
    #[must_use]
    pub fn with_meta_orchestrator(mut self, handle: MetaOrchestratorHandle) -> Self {
        self.meta_orchestrator = Some(handle);
        self
    }

    /// Timestamp Unix en millisecondes — utilisé pour horodater les events thinking.
    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// Émet `ThinkingStarted` sur le bus si configuré — silencieux si le bus est absent.
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

    /// Émet `ThinkingEnded` avec le raw content et les tokens consommés.
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

    /// Extrait les branches alternatives du thinking via la routine méta
    /// `GenerateAlternativeBranches` puis émet `DecisionPointRecorded`.
    ///
    /// Silencieux (no-op) si :
    /// - aucun `MetaOrchestratorHandle` n'est branché,
    /// - aucun `EventBusSender` n'est branché,
    /// - la routine est désactivée / budget épuisé / timeout / parse échoue.
    ///
    /// Kind utilisé : [`DecisionKind::ToolChoice`] — le step racine d'un plan
    /// représente le choix d'outil principal de ce tour.
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
            .map(|s| {
                s.tool_hint
                    .clone()
                    .unwrap_or_else(|| s.description.clone())
            })
            .unwrap_or_else(|| "(no step)".to_owned());

        if let Some(point) = orchestrator
            .generate_decision_point(
                turn_id,
                DecisionKind::ToolChoice,
                thinking_raw,
                &chosen,
                turn_id,
            )
            .await
        {
            let _ = bus.send(RuntimeEvent::DecisionPointRecorded { point });
        }
    }

    /// Génère un plan d'exécution initial depuis le [`ContextBundle`].
    ///
    /// Délègue à [`plan_internal`] avec la température par défaut (`None`).
    /// Retourne [`ReasonerError::PlanParseError`] après [`MAX_ATTEMPTS`] tentatives.
    pub async fn plan(&self, ctx: &ContextBundle) -> Result<ExecutionPlan, ReasonerError> {
        self.plan_internal(ctx, None).await
    }

    /// Génère deux plans alternatifs en parallèle via `tokio::join!`.
    ///
    /// Plan A est produit à `config.plan_alternatives_temp_a` (basse température —
    /// déterministe, conservateur). Plan B est produit à `config.plan_alternatives_temp_b`
    /// (haute température — créatif, exploratoire).
    ///
    /// Les deux appels LLM sont concurrents : la durée totale est ≈ 1 appel, pas 2.
    /// Retourne [`ReasonerError`] si l'un des deux plans est invalide après retries.
    pub async fn plan_with_alternatives(
        &self,
        ctx: &ContextBundle,
        config: &ORIAConfig,
    ) -> Result<PlanAlternatives, ReasonerError> {
        let session_id = uuid::Uuid::new_v4().to_string();

        let (plan_a_result, plan_b_result) = tokio::join!(
            self.plan_internal(ctx, Some(config.plan_alternatives_temp_a)),
            self.plan_internal(ctx, Some(config.plan_alternatives_temp_b)),
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

    /// Génère un plan d'exécution avec une température LLM explicite.
    ///
    /// Implémentation commune partagée par [`plan`] et [`plan_with_alternatives`].
    /// Applique jusqu'à [`MAX_ATTEMPTS`] retries en cas de JSON invalide.
    ///
    /// `temperature` est transmis à [`CompletionRequest`] si `Some` ; le backend
    /// utilise sa valeur par défaut si `None`.
    async fn plan_internal(
        &self,
        ctx: &ContextBundle,
        temperature: Option<f32>,
    ) -> Result<ExecutionPlan, ReasonerError> {
        let mut last_error = String::new();
        let turn_id = ctx.task.task_id.as_str().to_owned();
        let (_start_ts, start_instant) = self.emit_thinking_started(&turn_id);
        let mut total_tokens: u32 = 0;
        let mut last_raw = String::new();

        for attempt in 0..MAX_ATTEMPTS {
            let system = self.build_system_prompt(ctx);
            let user = if attempt == 0 {
                self.build_user_prompt(ctx)
            } else {
                format!(
                    "{}\n\nATTENTION : ta réponse précédente était invalide.\nErreur : {}\nCorrige et renvoie uniquement du JSON valide.",
                    self.build_user_prompt(ctx),
                    last_error
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
                    self.maybe_emit_decision_point(&turn_id, &last_raw, &plan).await;
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

    /// Replanification partielle après l'échec d'un step.
    ///
    /// Génère uniquement les steps restants en tenant compte des outputs des steps
    /// complétés (`completed_outputs`) et de la raison d'échec du step `failed_step_id`.
    ///
    /// Un seul appel LLM sans retry propre — la gestion des retries de replanification
    /// incombe à l'`ActorLoop`.
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

    /// Valide un plan JSON brut retourné par le LLM.
    ///
    /// 5 validations séquentielles — retourne `Err` au premier problème rencontré :
    /// 1. Strip les backticks Markdown éventuels et parse le JSON
    /// 2. Désérialise le tableau de [`PlanStep`] depuis le champ `"steps"`
    /// 3. Vérifie que tous les `step_id` sont uniques
    /// 4. Vérifie que chaque `depends_on` référence un `step_id` existant dans le plan
    /// 5. Détecte les cycles via [`topological_sort`] (algorithme de Kahn)
    ///
    /// Cette fonction est publique et pure — testable indépendamment du LLM.
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

    // ─── Mock LLM ─────────────────────────────

    /// Mock `CompletionModel` pour les tests du Reasoner.
    ///
    /// Consomme les réponses de la `queue` dans l'ordre ; utilise `fallback`
    /// une fois la queue épuisée. Compte le nombre d'appels via `call_count`.
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

    /// GIVEN un Reasoner avec un mock CompletionModel qui retourne un JSON valide
    /// WHEN reasoner.plan(&ctx).await est appelé
    /// THEN Ok(ExecutionPlan) est retourné avec 2 steps
    ///   ET le mock a été appelé exactement 1 fois
    #[tokio::test]
    async fn test_ac1_plan_valide_depuis_mock_llm() {
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

    // ─── Retry ×3 sur JSON invalide ───

    /// GIVEN un mock qui retourne du texte non-JSON 3 fois
    /// WHEN reasoner.plan(&ctx).await est appelé
    /// THEN Err(PlanParseError { attempts: 3 }) est retourné
    ///   ET le mock a été appelé exactement 3 fois
    #[tokio::test]
    async fn test_ac2_retry_3_fois_sur_json_invalide() {
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

    // ─── Détection dépendance circulaire → retry ───

    /// GIVEN un mock qui retourne un plan cyclique 3 fois
    /// WHEN reasoner.plan(&ctx).await est appelé
    /// THEN PlanParseError après 3 tentatives
    #[tokio::test]
    async fn test_ac3_cycle_detecte_et_retry() {
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

    // ─── suite — cycle puis plan valide ───

    /// GIVEN un mock qui retourne un plan cyclique puis un plan valide
    /// WHEN reasoner.plan(&ctx).await est appelé
    /// THEN Ok(ExecutionPlan) au 2ème essai
    #[tokio::test]
    async fn test_ac3_cycle_puis_plan_valide() {
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

    // ─── replan() génère un plan partiel ───

    /// GIVEN un mock qui retourne un plan de remplacement valide
    /// WHEN reasoner.replan(&ctx, &outputs, "s3", "timeout").await est appelé
    /// THEN Ok(ExecutionPlan) avec les nouveaux steps
    #[tokio::test]
    async fn test_ac5_replan_retourne_plan_valide() {
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

    // ─── Backticks Markdown strippés ───

    /// GIVEN une réponse LLM avec backticks Markdown
    /// WHEN parse_and_validate() est appelé
    /// THEN le JSON est parsé correctement
    #[test]
    fn test_ac6_backticks_strippes() {
        // GIVEN
        let reasoner = Reasoner::new(MockCompletionModel::sequence(vec![]), 10);
        let raw = "```json\n{\"steps\":[{\"step_id\":\"s1\",\"description\":\"d\",\"depends_on\":[]}]}\n```";

        // WHEN
        let result = reasoner.parse_and_validate(raw, "task-001");

        // THEN
        assert!(result.is_ok());
        assert_eq!(result.unwrap().steps[0].step_id, "s1");
    }

    // ─── Validation directe de parse_and_validate ───

    /// GIVEN un plan avec dépendance vers step inexistant
    /// WHEN parse_and_validate() est appelé
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

    /// GIVEN un plan avec deux steps ayant le même step_id
    /// WHEN parse_and_validate() est appelé
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

    /// GIVEN un plan 3 steps linéaires (s1→s2→s3)
    /// WHEN parse_and_validate() est appelé
    /// THEN Ok(ExecutionPlan) avec 3 steps et task_id correct
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

    // ─── Désérialisation sans model_hint ───

    /// GIVEN un JSON de PlanStep sans champ model_hint
    /// WHEN on désérialise
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

    // ─── Désérialisation avec model_hint ───

    /// GIVEN un JSON de PlanStep avec "model_hint": "fast-7b"
    /// WHEN on désérialise
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

    // ─── Deux plans générés en parallèle ───

    /// GIVEN un Reasoner avec un mock CompletionModel fournissant deux plans valides
    /// WHEN plan_with_alternatives(&ctx, &config) est appelé
    /// THEN PlanAlternatives retourné avec plan_a et plan_b non-vides
    ///   ET le mock a été appelé exactement 2 fois (un par plan)
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

    // ─── US-SP42-037 — ThinkingStarted / ThinkingEnded emission ───

    /// GIVEN un Reasoner branché sur un EventBus et un mock qui retourne un plan valide
    /// WHEN reasoner.plan(&ctx).await est appelé
    /// THEN exactement un `ThinkingStarted` suivi d'un `ThinkingEnded` sont émis
    ///   ET le `turn_id` des events correspond au `task_id` du ContextBundle
    ///   ET `ThinkingEnded.raw_content` transporte le dernier contenu LLM
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

        // THEN — ThinkingStarted puis ThinkingEnded avec le bon turn_id
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

    /// GIVEN un Reasoner branché sur un EventBus et un mock qui retourne du JSON invalide
    /// WHEN reasoner.plan(&ctx).await échoue après MAX_ATTEMPTS
    /// THEN `ThinkingEnded` est quand même émis (garantie de fin de phase)
    #[tokio::test]
    async fn emits_thinking_ended_on_parse_failure() {
        use apollia_core::events::RuntimeEvent;
        use tokio::sync::broadcast;

        let model = MockCompletionModel::sequence(vec!["not json", "nope", "still no"]);
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);
        let reasoner = Reasoner::new(model, 10).with_event_bus(tx);
        let ctx = ContextBundle::default();

        let _ = reasoner.plan(&ctx).await;

        // Drain until we see ThinkingEnded — there must be exactly one.
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

    /// GIVEN un mock qui fournit des plans avec des steps ayant des descriptions
    /// WHEN plan_with_alternatives() est appelé
    /// THEN les descriptions sont correctement converties en TaskPlanStep
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
