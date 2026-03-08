//! ORIA Reasoner — produit un `ExecutionPlan` structuré via un appel LLM.
//!
//! Le Reasoner est le composant central du pipeline ORIA pour le mode Orchestrated.
//! Il reçoit un [`ContextBundle`] enrichi par l'Observer, appelle un LLM via
//! `Arc<dyn CompletionModel>` et retourne un [`ExecutionPlan`] parsé depuis JSON.
//!
//! En cas de réponse JSON invalide, un message de correction est ajouté à l'historique
//! et l'appel est retenté jusqu'à 3 fois (principe #4 — Fail fast après N retries).

use std::sync::Arc;

use apollia_core::AIPPart;
use apollia_llm::{ChatMessage, CompletionModel, CompletionRequest, LlmError};

use crate::observer::ContextBundle;

// ─────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────

/// Plan d'exécution multi-étapes produit par le Reasoner.
///
/// Sérialisable en JSON pour être transmis au LLM et parsé depuis sa réponse.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ExecutionPlan {
    /// Objectif global du plan (résumé de haut niveau).
    pub goal: String,
    /// Étapes ordonnées du plan d'exécution.
    pub steps: Vec<PlanStep>,
}

/// Étape individuelle dans un [`ExecutionPlan`].
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PlanStep {
    /// Identifiant unique de l'étape (ex: `"s1"`, `"s2"`).
    pub id: String,
    /// Description lisible de l'action à réaliser lors de cette étape.
    pub description: String,
    /// Outil à utiliser pour cette étape (`None` si aucun outil requis).
    pub tool: Option<String>,
    /// Identifiants des étapes dont cette étape dépend (pour le DAG d'exécution).
    pub depends_on: Vec<String>,
}

/// Erreurs du Reasoner ORIA.
#[derive(thiserror::Error, Debug)]
pub enum ReasonerError {
    /// Échec de parsage JSON de l'`ExecutionPlan` après N tentatives.
    #[error("plan parse failed after {attempts} attempts, last: {last_response}")]
    PlanParseError {
        /// Nombre de tentatives effectuées avant l'abandon.
        attempts: u32,
        /// Dernière réponse brute reçue du LLM (pour le débogage).
        last_response: String,
    },

    /// Erreur LLM pendant la phase de planification.
    #[error("LLM error during planning: {0}")]
    LlmError(#[from] LlmError),
}

// ─────────────────────────────────────────────
// Reasoner
// ─────────────────────────────────────────────

/// Produit un [`ExecutionPlan`] structuré via un appel LLM.
///
/// Le `Reasoner` reçoit un `Arc<dyn CompletionModel>` injecté (pattern ADR-016
/// pour la testabilité) et construit un plan multi-étapes à partir d'un [`ContextBundle`].
///
/// En cas de réponse JSON invalide, il ajoute un message de correction et retente
/// jusqu'à [`MAX_RETRIES`] fois. Après épuisement des tentatives, retourne
/// [`ReasonerError::PlanParseError`].
pub struct Reasoner {
    model: Arc<dyn CompletionModel>,
}

/// Nombre maximum de tentatives de parsage JSON avant abandon.
const MAX_RETRIES: u32 = 3;

impl Reasoner {
    /// Crée un `Reasoner` avec le modèle LLM injecté.
    pub fn new(model: Arc<dyn CompletionModel>) -> Self {
        Self { model }
    }

    /// Construit le prompt planner à partir d'un [`ContextBundle`].
    ///
    /// Retourne un vecteur de [`ChatMessage`] prêt à être envoyé au LLM :
    /// 1. Un message système décrivant le format JSON `ExecutionPlan` attendu
    ///    avec un exemple few-shot pour guider le modèle.
    /// 2. Un message utilisateur avec le texte de la tâche et les outils disponibles.
    ///
    /// Cette fonction est publique et testable unitairement sans appel LLM.
    pub fn build_prompt(bundle: &ContextBundle) -> Vec<ChatMessage> {
        let schema_example = r#"{"goal":"short goal description","steps":[{"id":"s1","description":"what to do","tool":null,"depends_on":[]},{"id":"s2","description":"next step","tool":"file_io","depends_on":["s1"]}]}"#;

        let system = format!(
            "You are a task planning assistant for an autonomous agent runtime.\n\
             Given a task and the list of available tools, produce an ExecutionPlan as JSON.\n\
             Return ONLY valid JSON matching this exact schema:\n{schema_example}\n\
             Rules:\n\
             - No prose, no markdown, no explanation — pure JSON object only.\n\
             - \"tool\" must be null or one of the available tool names.\n\
             - \"depends_on\" lists step ids that must complete before this step.\n\
             - Each step id must be unique (s1, s2, ...)."
        );

        let task_text: String = bundle
            .task
            .input
            .parts
            .iter()
            .filter_map(|part| match part {
                AIPPart::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");

        let tools_section = if bundle.available_tools.is_empty() {
            "No tools available.".to_string()
        } else {
            format!("Available tools: {}", bundle.available_tools.join(", "))
        };

        let user = format!("Task: {task_text}\n{tools_section}");

        vec![ChatMessage::system(system), ChatMessage::user(user)]
    }

    /// Génère un [`ExecutionPlan`] depuis le [`ContextBundle`].
    ///
    /// Appelle `CompletionModel::complete()` et tente de parser la réponse en JSON.
    /// Si le parsage échoue, ajoute un message de correction à l'historique et retente.
    /// Après [`MAX_RETRIES`] tentatives infructueuses, retourne [`ReasonerError::PlanParseError`].
    pub async fn plan(&self, bundle: &ContextBundle) -> Result<ExecutionPlan, ReasonerError> {
        let mut messages = Self::build_prompt(bundle);
        let mut last_response = String::new();

        for attempt in 1..=MAX_RETRIES {
            let req = CompletionRequest {
                messages: messages.clone(),
                ..Default::default()
            };
            let response = self.model.complete(req).await?;
            last_response = response.content.clone();

            match serde_json::from_str::<ExecutionPlan>(&response.content) {
                Ok(plan) => {
                    tracing::info!(
                        attempts = attempt,
                        steps = plan.steps.len(),
                        "ExecutionPlan ready"
                    );
                    return Ok(plan);
                }
                Err(e) => {
                    tracing::warn!(attempt, error = %e, "invalid JSON plan, retrying");
                    messages.push(ChatMessage::user(
                        "Your response was not valid JSON. Return ONLY a JSON object \
                         matching the ExecutionPlan schema. No prose, no markdown.",
                    ));
                }
            }
        }

        Err(ReasonerError::PlanParseError {
            attempts: MAX_RETRIES,
            last_response,
        })
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_core::{AIPInput, AIPPart, AIPTask, TextPart};
    use apollia_llm::{CompletionResponse, FinishReason, MessageContent, TokenUsage};
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    use crate::observer::{ContextBundle, ExecutionMode};

    // ─── Mock LLM ─────────────────────────────

    /// Mock `CompletionModel` pour les tests du Reasoner.
    ///
    /// Supporte trois modes :
    /// - `returning(json)` / `always_returning(json)` : retourne toujours la même réponse
    /// - `sequence(vec)` : consomme les réponses dans l'ordre, puis retourne `fallback`
    struct MockCompletionModel {
        queue: Mutex<VecDeque<String>>,
        fallback: String,
        call_count: AtomicU32,
    }

    impl MockCompletionModel {
        fn returning(json: &str) -> Self {
            Self {
                queue: Mutex::new(VecDeque::new()),
                fallback: json.to_string(),
                call_count: AtomicU32::new(0),
            }
        }

        fn always_returning(json: &str) -> Self {
            Self::returning(json)
        }

        fn sequence(responses: Vec<&str>) -> Self {
            Self {
                queue: Mutex::new(responses.iter().map(|s| s.to_string()).collect()),
                fallback: "{}".to_string(),
                call_count: AtomicU32::new(0),
            }
        }

        fn call_count(&self) -> u32 {
            self.call_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockCompletionModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let content = {
                let mut queue = self.queue.lock().expect("mock lock poisoned");
                queue.pop_front().unwrap_or_else(|| self.fallback.clone())
            };
            Ok(CompletionResponse {
                content,
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    cost_usd: None,
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 0,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<String, LlmError>> + Send>>, LlmError>
        {
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

    // ─── Helper ───────────────────────────────

    fn make_context_bundle(task_text: &str) -> ContextBundle {
        ContextBundle {
            task: AIPTask {
                task_id: "task-001".into(),
                context_id: "ctx-001".into(),
                input: AIPInput {
                    parts: vec![AIPPart::Text(TextPart {
                        text: task_text.into(),
                    })],
                },
                history: vec![],
                timeout_seconds: None,
            },
            memory_snapshot: None,
            execution_mode: ExecutionMode::Orchestrated,
            available_tools: vec![],
        }
    }

    // ─── AC-1 ─────────────────────────────────

    /// GIVEN un mock `CompletionModel` qui retourne un JSON `ExecutionPlan` bien formé
    /// WHEN on appelle `reasoner.plan(&context_bundle).await`
    /// THEN `Ok(ExecutionPlan { steps, ... })` est retourné avec les steps corrects
    #[tokio::test]
    async fn test_ac1_valid_json_returns_plan() {
        // GIVEN
        let valid_json = r#"{"goal":"test","steps":[{"id":"s1","description":"step 1","tool":null,"depends_on":[]}]}"#;
        let model = Arc::new(MockCompletionModel::returning(valid_json));
        let reasoner = Reasoner::new(model);
        let bundle = make_context_bundle("plan this task");

        // WHEN
        let result = reasoner.plan(&bundle).await;

        // THEN
        let plan = result.expect("expected Ok(ExecutionPlan)");
        assert_eq!(plan.goal, "test");
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].id, "s1");
        assert_eq!(plan.steps[0].description, "step 1");
        assert!(plan.steps[0].tool.is_none());
        assert!(plan.steps[0].depends_on.is_empty());
    }

    // ─── AC-2 ─────────────────────────────────

    /// GIVEN un mock LLM qui retourne 2 fois du JSON invalide, puis un JSON valide
    /// WHEN on appelle `reasoner.plan(&context_bundle).await`
    /// THEN `Ok(ExecutionPlan)` est retourné après le 3ème appel
    ///      ET `complete()` a été appelé exactement 3 fois
    #[tokio::test]
    async fn test_ac2_retry_on_invalid_json() {
        // GIVEN — 2 JSON invalides, puis 1 valide
        let valid_json = r#"{"goal":"ok","steps":[]}"#;
        let model = Arc::new(MockCompletionModel::sequence(vec![
            "not json",
            "also not json",
            valid_json,
        ]));
        let reasoner = Reasoner::new(Arc::clone(&model) as Arc<dyn CompletionModel>);
        let bundle = make_context_bundle("test");

        // WHEN
        let result = reasoner.plan(&bundle).await;

        // THEN
        assert!(result.is_ok(), "expected Ok but got: {:?}", result.err());
        assert_eq!(model.call_count(), 3, "expected exactly 3 LLM calls");
    }

    // ─── AC-3 ─────────────────────────────────

    /// GIVEN un mock LLM qui retourne toujours du JSON invalide
    /// WHEN on appelle `reasoner.plan(&context_bundle).await`
    /// THEN `Err(ReasonerError::PlanParseError { attempts: 3, .. })` est retourné
    ///      ET `complete()` a été appelé exactement 3 fois
    #[tokio::test]
    async fn test_ac3_error_after_3_retries() {
        // GIVEN — toujours JSON invalide
        let model = Arc::new(MockCompletionModel::always_returning("invalid json !"));
        let reasoner = Reasoner::new(Arc::clone(&model) as Arc<dyn CompletionModel>);
        let bundle = make_context_bundle("test");

        // WHEN
        let result = reasoner.plan(&bundle).await;

        // THEN
        assert!(
            matches!(
                result,
                Err(ReasonerError::PlanParseError { attempts: 3, .. })
            ),
            "expected PlanParseError{{attempts:3}}, got: {:?}",
            result
        );
        assert_eq!(model.call_count(), 3, "expected exactly 3 LLM calls");
    }

    // ─── AC-4 ─────────────────────────────────

    /// GIVEN un `ContextBundle` avec `task.input` contenant un texte
    /// WHEN `Reasoner::build_prompt()` est appelé
    /// ALORS le prompt contient le texte de la tâche
    #[test]
    fn test_ac4_build_prompt_contains_task_text() {
        // GIVEN
        let bundle = make_context_bundle("calculate the devis for client X");

        // WHEN
        let messages = Reasoner::build_prompt(&bundle);

        // THEN — le texte de la tâche doit apparaître dans les messages
        let all_content: String = messages
            .iter()
            .filter_map(|m| {
                if let MessageContent::Text(t) = &m.content {
                    Some(t.clone())
                } else {
                    None
                }
            })
            .collect();
        assert!(
            all_content.contains("calculate the devis"),
            "prompt must contain task text, got: {all_content}"
        );
    }

    /// GIVEN un `ContextBundle` avec 2 outils disponibles
    /// WHEN `Reasoner::build_prompt()` est appelé
    /// ALORS le prompt contient les noms des 2 outils
    #[test]
    fn test_ac4_build_prompt_contains_tool_names() {
        // GIVEN
        let mut bundle = make_context_bundle("do something");
        bundle.available_tools = vec!["file_io".into(), "bash_executor".into()];

        // WHEN
        let messages = Reasoner::build_prompt(&bundle);

        // THEN
        let all_content: String = messages
            .iter()
            .filter_map(|m| {
                if let MessageContent::Text(t) = &m.content {
                    Some(t.clone())
                } else {
                    None
                }
            })
            .collect();
        assert!(
            all_content.contains("file_io"),
            "prompt must contain tool name 'file_io', got: {all_content}"
        );
        assert!(
            all_content.contains("bash_executor"),
            "prompt must contain tool name 'bash_executor', got: {all_content}"
        );
    }
}
