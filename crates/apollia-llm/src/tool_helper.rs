//! `ToolCallHelper` — boucle ReAct automatique avec exécution d'outils.
//!
//! Orchestre les appels LLM successifs et l'invocation du [`ToolInvoker`].

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use crate::types::{
    ChatMessage, CompletionModel, CompletionRequest, FinishReason, LlmError, ToolSpec,
};

// ─────────────────────────────────────────────
// StepBudgetView
// ─────────────────────────────────────────────

/// Vue read-only du `StepBudget` exposée à la boucle ReAct.
///
/// Partagée via `Arc<AtomicU32>` avec le budget mutable du runtime.
/// Aucune mutation n'est possible depuis cette vue.
pub struct StepBudgetView {
    step_count: Arc<AtomicU32>,
    step_limit: u32,
}

impl StepBudgetView {
    /// Crée une vue à partir d'un compteur atomique partagé et d'une limite.
    pub fn new(step_count: Arc<AtomicU32>, step_limit: u32) -> Self {
        Self {
            step_count,
            step_limit,
        }
    }

    /// Crée une vue sans limite de budget — pour les tests unitaires.
    pub fn unlimited() -> Self {
        Self {
            step_count: Arc::new(AtomicU32::new(0)),
            step_limit: u32::MAX,
        }
    }

    /// Retourne `true` si le budget de steps a été épuisé.
    pub fn is_exhausted(&self) -> bool {
        self.step_count.load(Ordering::Relaxed) >= self.step_limit
    }

    /// Steps restants avant d'atteindre la limite.
    ///
    /// Retourne 0 si le budget est déjà épuisé (pas de valeur négative).
    pub fn steps_remaining(&self) -> i64 {
        let used = self.step_count.load(Ordering::Relaxed) as i64;
        let limit = self.step_limit as i64;
        (limit - used).max(0)
    }
}

// ─────────────────────────────────────────────
// ToolInvoker trait
// ─────────────────────────────────────────────

/// Abstraction pour l'invocation d'outils depuis la boucle ReAct.
///
/// Pattern ADR-015 : dépendance via trait, pas via type concret.
/// Cela évite toute dépendance directe de `apollia-llm` vers `apollia-tools`
/// et permet l'injection d'un mock en test.
///
/// L'implémentation concrète wrappant `ToolRegistryHandle` est créée.
#[async_trait::async_trait]
pub trait ToolInvoker: Send + Sync {
    /// Invoque un outil par son nom avec les arguments JSON fournis.
    ///
    /// Retourne `Ok(résultat)` sous forme de chaîne ou `Err(description)`.
    /// Les erreurs sont absorbées par [`ToolCallHelper`] comme résultats texte —
    /// elles ne sont jamais fatales pour la boucle.
    async fn invoke(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<String, String>;
}

// ─────────────────────────────────────────────
// ToolCallHelper
// ─────────────────────────────────────────────

/// Orchestre la boucle ReAct : LLM → outil(s) → LLM → ... → réponse finale.
///
/// Applique deux garde-fous non-négociables (Principe #7) :
/// - `max_iters` : nombre maximal d'appels LLM avant [`LlmError::MaxIterationsReached`]
/// - [`StepBudgetView`] : budget d'agent vérifié en premier à chaque itération
///
/// Principe #5 — Un acteur, une responsabilité : `ToolCallHelper` orchestre
/// mais n'exécute pas directement les outils (délégué au [`ToolInvoker`]).
pub struct ToolCallHelper {
    model: Arc<dyn CompletionModel>,
    invoker: Arc<dyn ToolInvoker>,
}

impl ToolCallHelper {
    /// Crée un `ToolCallHelper` à partir d'un backend LLM et d'un invoqueur d'outils.
    pub fn new(model: Arc<dyn CompletionModel>, invoker: Arc<dyn ToolInvoker>) -> Self {
        Self { model, invoker }
    }

    /// Exécute la boucle ReAct complète.
    ///
    /// Itère jusqu'à [`FinishReason::Stop`] ou épuisement d'un garde-fou.
    /// Les tool_calls sont exécutés séquentiellement pour garantir l'ordre.
    /// Les erreurs d'outil sont absorbées comme résultats texte (jamais fatales).
    ///
    /// # Errors
    ///
    /// - [`LlmError::BudgetExceeded`] si le `StepBudget` est épuisé avant un appel LLM
    /// - [`LlmError::MaxIterationsReached`] si la boucle dépasse `max_iters` itérations
    /// - [`LlmError::MaxTokensReached`] si le LLM retourne [`FinishReason::Length`]
    /// - [`LlmError::InferenceError`] si le LLM retourne [`FinishReason::Error`]
    pub async fn run_tools(
        &self,
        mut messages: Vec<ChatMessage>,
        tools: Vec<ToolSpec>,
        max_iters: u32,
        budget: &StepBudgetView,
    ) -> Result<String, LlmError> {
        for _iter in 0..max_iters {
            // Principe #7 — garde-fou budget vérifié en première position
            if budget.is_exhausted() {
                return Err(LlmError::BudgetExceeded);
            }

            let response = self
                .model
                .complete(CompletionRequest {
                    messages: messages.clone(),
                    tools: tools.clone(),
                    ..Default::default()
                })
                .await?;

            match response.finish_reason {
                FinishReason::Stop => return Ok(response.content),

                FinishReason::ToolCalls => {
                    messages.push(ChatMessage::assistant_with_calls(
                        &response.content,
                        &response.tool_calls,
                    ));
                    for call in &response.tool_calls {
                        let result = self
                            .invoker
                            .invoke(&call.name, &call.arguments)
                            .await
                            .unwrap_or_else(|e| format!("tool error: {e}"));
                        messages.push(ChatMessage::tool_result(&call.id, &result));
                    }
                }

                FinishReason::Length => return Err(LlmError::MaxTokensReached),
                FinishReason::Error => {
                    return Err(LlmError::InferenceError(
                        "backend returned error finish_reason".into(),
                    ))
                }
            }
        }
        Err(LlmError::MaxIterationsReached {
            iterations: max_iters,
        })
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CompletionResponse, StreamChunk, TokenUsage, ToolCall, ToolSpec};
    use std::pin::Pin;
    use std::sync::atomic::AtomicU32;

    use futures::Stream;

    // ── Helpers de budget ──────────────────────────────────────────────────

    fn unlimited_budget() -> StepBudgetView {
        StepBudgetView {
            step_count: Arc::new(AtomicU32::new(0)),
            step_limit: 100,
        }
    }

    fn exhausted_budget() -> StepBudgetView {
        StepBudgetView {
            step_count: Arc::new(AtomicU32::new(100)),
            step_limit: 100,
        }
    }

    // ── Helpers de réponse ────────────────────────────────────────────────

    fn stop_response(content: impl Into<String>) -> CompletionResponse {
        CompletionResponse {
            content: content.into(),
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
        }
    }

    fn tool_calls_response(calls: Vec<ToolCall>) -> CompletionResponse {
        CompletionResponse {
            content: String::new(),
            tool_calls: calls,
            usage: TokenUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
                cost_usd: None,
                ..Default::default()
            },
            finish_reason: FinishReason::ToolCalls,
            latency_ms: 0,
            ttft_ms: None,
        }
    }

    // ── Mock CompletionModel : Stop immédiat ──────────────────────────────

    struct MockStopModel {
        content: String,
    }

    impl MockStopModel {
        fn new(content: impl Into<String>) -> Self {
            Self {
                content: content.into(),
            }
        }
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockStopModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(stop_response(self.content.clone()))
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            unimplemented!("not used in tests")
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "mock-stop"
        }
        fn model_id(&self) -> &str {
            "mock"
        }
    }

    // ── Mock CompletionModel : ToolCalls puis Stop ────────────────────────

    struct MockReActModel {
        calls: Vec<ToolCall>,
        final_content: String,
        iteration: AtomicU32,
    }

    impl MockReActModel {
        fn new(calls: Vec<ToolCall>, final_content: impl Into<String>) -> Self {
            Self {
                calls,
                final_content: final_content.into(),
                iteration: AtomicU32::new(0),
            }
        }
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockReActModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            let current = self.iteration.fetch_add(1, Ordering::SeqCst);
            if current == 0 {
                Ok(tool_calls_response(self.calls.clone()))
            } else {
                Ok(stop_response(self.final_content.clone()))
            }
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            unimplemented!("not used in tests")
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "mock-react"
        }
        fn model_id(&self) -> &str {
            "mock"
        }
    }

    // ── Mock CompletionModel : toujours ToolCalls (infini) ────────────────

    struct MockInfiniteToolCallModel;

    #[async_trait::async_trait]
    impl CompletionModel for MockInfiniteToolCallModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(tool_calls_response(vec![]))
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            unimplemented!("not used in tests")
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "mock-infinite"
        }
        fn model_id(&self) -> &str {
            "mock"
        }
    }

    // ── Mock ToolInvoker ──────────────────────────────────────────────────

    struct MockToolInvoker {
        result: Option<String>,
        call_count: Arc<AtomicU32>,
    }

    impl MockToolInvoker {
        fn new() -> Self {
            Self {
                result: None,
                call_count: Arc::new(AtomicU32::new(0)),
            }
        }

        fn with_result(result: impl Into<String>) -> Self {
            Self {
                result: Some(result.into()),
                call_count: Arc::new(AtomicU32::new(0)),
            }
        }

        fn call_count(&self) -> u32 {
            self.call_count.load(Ordering::Relaxed)
        }
    }

    #[async_trait::async_trait]
    impl ToolInvoker for MockToolInvoker {
        async fn invoke(
            &self,
            _tool_name: &str,
            _arguments: &serde_json::Value,
        ) -> Result<String, String> {
            self.call_count.fetch_add(1, Ordering::Relaxed);
            match &self.result {
                Some(r) => Ok(r.clone()),
                None => Ok("ok".into()),
            }
        }
    }

    // ── Mock ToolInvoker qui échoue toujours ──────────────────────────────

    struct MockFailingToolInvoker;

    #[async_trait::async_trait]
    impl ToolInvoker for MockFailingToolInvoker {
        async fn invoke(
            &self,
            _tool_name: &str,
            _arguments: &serde_json::Value,
        ) -> Result<String, String> {
            Err("outil non disponible".into())
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────

    /// LLM répond directement sans appel d'outil.
    #[tokio::test]
    async fn test_ac1_stop_immediately_no_tool_call() {
        // GIVEN
        let model = Arc::new(MockStopModel::new("réponse finale"));
        let invoker = Arc::new(MockToolInvoker::new());
        let helper = ToolCallHelper::new(model, invoker.clone());

        // WHEN
        let result = helper
            .run_tools(
                vec![ChatMessage::user("question")],
                vec![],
                5,
                &unlimited_budget(),
            )
            .await;

        // THEN
        assert_eq!(result.unwrap(), "réponse finale");
        assert_eq!(invoker.call_count(), 0);
    }

    /// Boucle ReAct : LLM appelle 1 outil puis répond.
    #[tokio::test]
    async fn test_ac2_one_tool_call_then_stop() {
        // GIVEN
        let model = Arc::new(MockReActModel::new(
            vec![ToolCall {
                id: "c1".into(),
                name: "echo".into(),
                arguments: serde_json::json!({}),
            }],
            "réponse après outil",
        ));
        let invoker = Arc::new(MockToolInvoker::with_result("résultat_outil"));
        let helper = ToolCallHelper::new(model, invoker.clone());

        // WHEN
        let result = helper
            .run_tools(
                vec![ChatMessage::user("question")],
                vec![ToolSpec {
                    name: "echo".into(),
                    description: String::new(),
                    parameters: serde_json::json!({}),
                }],
                5,
                &unlimited_budget(),
            )
            .await;

        // THEN
        assert_eq!(result.unwrap(), "réponse après outil");
        assert_eq!(invoker.call_count(), 1);
    }

    /// Garde-fou `max_iterations` respecté.
    #[tokio::test]
    async fn test_ac3_max_iterations_reached() {
        // GIVEN — LLM retourne toujours ToolCalls
        let model = Arc::new(MockInfiniteToolCallModel);
        let invoker = Arc::new(MockToolInvoker::new());
        let helper = ToolCallHelper::new(model, invoker);

        // WHEN
        let result = helper
            .run_tools(vec![ChatMessage::user("q")], vec![], 3, &unlimited_budget())
            .await;

        // THEN
        assert!(matches!(
            result,
            Err(LlmError::MaxIterationsReached { iterations: 3 })
        ));
    }

    /// Garde-fou `StepBudget` respecté : aucun appel LLM si budget épuisé.
    #[tokio::test]
    async fn test_ac4_budget_exhausted_stops_immediately() {
        // GIVEN
        let model = Arc::new(MockStopModel::new("should not be reached"));
        let invoker = Arc::new(MockToolInvoker::new());
        let helper = ToolCallHelper::new(model, invoker);

        // WHEN
        let result = helper
            .run_tools(vec![ChatMessage::user("q")], vec![], 5, &exhausted_budget())
            .await;

        // THEN
        assert!(matches!(result, Err(LlmError::BudgetExceeded)));
    }

    /// Erreur d'outil absorbée comme texte, boucle continue.
    #[tokio::test]
    async fn test_ac5_tool_error_absorbed_as_text() {
        // GIVEN — ToolInvoker retourne toujours une erreur
        let model = Arc::new(MockReActModel::new(
            vec![ToolCall {
                id: "c1".into(),
                name: "fail_tool".into(),
                arguments: serde_json::json!({}),
            }],
            "réponse malgré erreur",
        ));
        let invoker = Arc::new(MockFailingToolInvoker);
        let helper = ToolCallHelper::new(model, invoker);

        // WHEN
        let result = helper
            .run_tools(vec![ChatMessage::user("q")], vec![], 5, &unlimited_budget())
            .await;

        // THEN — l'erreur d'outil n'est pas fatale
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "réponse malgré erreur");
    }
}
