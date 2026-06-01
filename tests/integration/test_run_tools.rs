//! Tests e2e - boucle ReAct via `ToolCallHelper`.
//!
//! Vérifie le comportement de la boucle ReAct depuis l'API publique de `apollia-llm` :
//! - arrêt immédiat sur `FinishReason::Stop` ;
//! - 1 appel d'outil puis réponse finale ;
//! - garde-fou `max_iterations` ;
//! - garde-fou `StepBudget` épuisé ;
//! - absorption des erreurs d'outil comme résultat texte.
//!
//! Aucune dépendance Python - utilise uniquement des mocks Rust.

use std::pin::Pin;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use futures::Stream;

use apollia_llm::{
    ChatMessage, CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError,
    StepBudgetView, StreamChunk, TokenUsage, ToolCall, ToolCallHelper, ToolInvoker, ToolSpec,
};

// ─────────────────────────────────────────────
// Mock CompletionModel : réponse Stop immédiate
// ─────────────────────────────────────────────

/// Mock LLM qui retourne toujours `FinishReason::Stop` avec le contenu fourni.
struct MockStopModel {
    response: String,
    call_count: Arc<AtomicU32>,
}

impl MockStopModel {
    fn new(response: impl Into<String>) -> (Arc<Self>, Arc<AtomicU32>) {
        let count = Arc::new(AtomicU32::new(0));
        (
            Arc::new(Self {
                response: response.into(),
                call_count: count.clone(),
            }),
            count,
        )
    }
}

#[async_trait::async_trait]
impl CompletionModel for MockStopModel {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(CompletionResponse {
            content: self.response.clone(),
            tool_calls: vec![],
            usage: TokenUsage {
                prompt_tokens: 5,
                completion_tokens: 5,
                cost_usd: None,
                cache_read_input_tokens: 0,
                cache_write_input_tokens: 0,
            },
            finish_reason: FinishReason::Stop,
            latency_ms: 0,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        let content = self.response.clone();
        Ok(Box::pin(futures::stream::once(async move {
            Ok(StreamChunk::Text(content))
        })))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "mock-stop"
    }
    fn model_id(&self) -> &str {
        "mock-v1"
    }
}

// ─────────────────────────────────────────────
// Mock CompletionModel : ReAct (1 ToolCalls → Stop)
// ─────────────────────────────────────────────

/// Mock LLM qui émet 1 appel d'outil au 1er appel, puis `Stop` au 2ème.
struct MockReActModel {
    tool_name: String,
    final_content: String,
    call_count: Arc<AtomicU32>,
}

impl MockReActModel {
    fn new(
        tool_name: impl Into<String>,
        final_content: impl Into<String>,
    ) -> (Arc<Self>, Arc<AtomicU32>) {
        let count = Arc::new(AtomicU32::new(0));
        (
            Arc::new(Self {
                tool_name: tool_name.into(),
                final_content: final_content.into(),
                call_count: count.clone(),
            }),
            count,
        )
    }
}

#[async_trait::async_trait]
impl CompletionModel for MockReActModel {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let current = self.call_count.fetch_add(1, Ordering::SeqCst);
        if current == 0 {
            Ok(CompletionResponse {
                content: String::new(),
                tool_calls: vec![ToolCall {
                    id: "call_01".into(),
                    name: self.tool_name.clone(),
                    arguments: serde_json::json!({}),
                }],
                usage: TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 2,
                    cost_usd: None,
                    cache_read_input_tokens: 0,
                    cache_write_input_tokens: 0,
                },
                finish_reason: FinishReason::ToolCalls,
                latency_ms: 0,
                ttft_ms: None,
            })
        } else {
            Ok(CompletionResponse {
                content: self.final_content.clone(),
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 5,
                    completion_tokens: 5,
                    cost_usd: None,
                    cache_read_input_tokens: 0,
                    cache_write_input_tokens: 0,
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 0,
                ttft_ms: None,
            })
        }
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        unimplemented!("non utilisé dans ce test")
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "mock-react"
    }
    fn model_id(&self) -> &str {
        "mock-v1"
    }
}

// ─────────────────────────────────────────────
// Mock CompletionModel : toujours ToolCalls (infini)
// ─────────────────────────────────────────────

/// Mock LLM qui retourne toujours `FinishReason::ToolCalls` - pour tester le garde-fou.
struct MockInfiniteToolCallModel {
    call_count: Arc<AtomicU32>,
}

impl MockInfiniteToolCallModel {
    fn new() -> (Arc<Self>, Arc<AtomicU32>) {
        let count = Arc::new(AtomicU32::new(0));
        (
            Arc::new(Self {
                call_count: count.clone(),
            }),
            count,
        )
    }
}

#[async_trait::async_trait]
impl CompletionModel for MockInfiniteToolCallModel {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(CompletionResponse {
            content: String::new(),
            tool_calls: vec![ToolCall {
                id: "c1".into(),
                name: "echo".into(),
                arguments: serde_json::json!({}),
            }],
            usage: TokenUsage {
                prompt_tokens: 5,
                completion_tokens: 2,
                cost_usd: None,
                cache_read_input_tokens: 0,
                cache_write_input_tokens: 0,
            },
            finish_reason: FinishReason::ToolCalls,
            latency_ms: 0,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        unimplemented!("non utilisé dans ce test")
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        "mock-infinite"
    }
    fn model_id(&self) -> &str {
        "mock-v1"
    }
}

// ─────────────────────────────────────────────
// Mock ToolInvoker
// ─────────────────────────────────────────────

/// Mock `ToolInvoker` qui retourne un résultat configurable.
struct MockToolInvoker {
    result: String,
    call_count: Arc<AtomicU32>,
}

impl MockToolInvoker {
    fn new(result: impl Into<String>) -> (Arc<Self>, Arc<AtomicU32>) {
        let count = Arc::new(AtomicU32::new(0));
        (
            Arc::new(Self {
                result: result.into(),
                call_count: count.clone(),
            }),
            count,
        )
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
        Ok(self.result.clone())
    }
}

/// Mock `ToolInvoker` qui échoue toujours.
struct FailingToolInvoker;

#[async_trait::async_trait]
impl ToolInvoker for FailingToolInvoker {
    async fn invoke(
        &self,
        tool_name: &str,
        _arguments: &serde_json::Value,
    ) -> Result<String, String> {
        Err(format!("outil '{tool_name}' non disponible"))
    }
}

// ─────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────

fn make_tool_spec(name: &str) -> ToolSpec {
    ToolSpec {
        name: name.to_string(),
        description: "outil de test".to_string(),
        parameters: serde_json::json!({}),
    }
}

fn user_message(text: &str) -> ChatMessage {
    ChatMessage::user(text)
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

/// La boucle s'arrête immédiatement quand le LLM retourne `Stop`.
#[tokio::test]
async fn test_react_loop_stops_on_finish_stop() {
    // GIVEN un mock qui répond directement avec Stop
    let (model, model_count) = MockStopModel::new("réponse directe");
    let (invoker, invoker_count) = MockToolInvoker::new("ok");
    let helper = ToolCallHelper::new(model, invoker);
    let budget = StepBudgetView::unlimited();

    // WHEN la boucle ReAct est exécutée
    let result = helper
        .run_tools(
            vec![user_message("question")],
            vec![make_tool_spec("echo")],
            5,
            &budget,
        )
        .await;

    // THEN le résultat est Ok avec le contenu du mock
    assert_eq!(result.unwrap(), "réponse directe");
    // ET le LLM a été appelé exactement 1 fois
    assert_eq!(
        model_count.load(Ordering::Relaxed),
        1,
        "le LLM doit être appelé exactement 1 fois"
    );
    // ET aucun outil n'a été invoqué
    assert_eq!(
        invoker_count.load(Ordering::Relaxed),
        0,
        "aucun outil ne doit être invoqué"
    );
}

/// Boucle ReAct : 1 appel d'outil puis réponse finale (2 appels LLM, 1 appel outil).
#[tokio::test]
async fn test_react_loop_calls_tool_once() {
    // GIVEN un mock ReAct : ToolCalls au 1er appel, Stop au 2ème
    let (model, model_count) = MockReActModel::new("echo", "réponse finale");
    let (invoker, invoker_count) = MockToolInvoker::new("résultat_outil");
    let helper = ToolCallHelper::new(model, invoker);
    let budget = StepBudgetView::unlimited();

    // WHEN la boucle ReAct est exécutée
    let result = helper
        .run_tools(
            vec![user_message("question")],
            vec![make_tool_spec("echo")],
            5,
            &budget,
        )
        .await;

    // THEN le résultat est Ok avec la réponse finale
    assert_eq!(result.unwrap(), "réponse finale");
    // ET le LLM a été appelé exactement 2 fois (ToolCalls + Stop)
    assert_eq!(
        model_count.load(Ordering::Relaxed),
        2,
        "le LLM doit être appelé exactement 2 fois"
    );
    // ET l'outil a été invoqué exactement 1 fois
    assert_eq!(
        invoker_count.load(Ordering::Relaxed),
        1,
        "l'outil doit être invoqué exactement 1 fois"
    );
}

/// Garde-fou `max_iterations` : la boucle infinie est arrêtée à N itérations.
#[tokio::test]
async fn test_react_loop_max_iterations_guard() {
    // GIVEN un mock qui retourne toujours ToolCalls (boucle infinie potentielle)
    let (model, model_count) = MockInfiniteToolCallModel::new();
    let (invoker, _) = MockToolInvoker::new("ok");
    let helper = ToolCallHelper::new(model, invoker);
    let budget = StepBudgetView::unlimited();

    // WHEN la boucle est exécutée avec max_iterations = 3
    let result = helper
        .run_tools(vec![user_message("q")], vec![], 3, &budget)
        .await;

    // THEN MaxIterationsReached(3) est retourné
    assert!(
        matches!(
            result,
            Err(LlmError::MaxIterationsReached { iterations: 3 })
        ),
        "expected MaxIterationsReached(3), got: {result:?}"
    );
    // ET exactement 3 appels LLM ont eu lieu
    assert_eq!(
        model_count.load(Ordering::Relaxed),
        3,
        "le LLM doit être appelé exactement 3 fois"
    );
}

/// Garde-fou `StepBudget` : aucun appel LLM si le budget est épuisé.
#[tokio::test]
async fn test_react_loop_budget_exhausted_guard() {
    // GIVEN un budget déjà épuisé (100/100 steps)
    let counter = Arc::new(AtomicU32::new(100));
    let budget = StepBudgetView::new(counter, 100);
    let (model, model_count) = MockStopModel::new("ne doit pas être atteint");
    let (invoker, _) = MockToolInvoker::new("ok");
    let helper = ToolCallHelper::new(model, invoker);

    // WHEN la boucle est exécutée avec un budget épuisé
    let result = helper
        .run_tools(vec![user_message("q")], vec![], 5, &budget)
        .await;

    // THEN BudgetExceeded est retourné immédiatement
    assert!(
        matches!(result, Err(LlmError::BudgetExceeded)),
        "expected BudgetExceeded, got: {result:?}"
    );
    // ET aucun appel LLM n'a eu lieu
    assert_eq!(
        model_count.load(Ordering::Relaxed),
        0,
        "aucun appel LLM ne doit avoir lieu avec un budget épuisé"
    );
}

/// Les erreurs d'outil sont absorbées comme texte, la boucle continue.
///
/// Vérifie que `ToolInvoker::invoke` retournant `Err` ne fait pas planter la boucle :
/// l'erreur est transmise au LLM comme résultat textuel.
#[tokio::test]
async fn test_react_loop_tool_error_absorbed() {
    // GIVEN un mock ReAct + un ToolInvoker qui échoue toujours
    let (model, _) = MockReActModel::new("fail_tool", "réponse malgré erreur");
    let invoker = Arc::new(FailingToolInvoker);
    let helper = ToolCallHelper::new(model, invoker);
    let budget = StepBudgetView::unlimited();

    // WHEN la boucle est exécutée
    let result = helper
        .run_tools(vec![user_message("q")], vec![], 5, &budget)
        .await;

    // THEN la boucle ne panic pas et retourne Ok (erreur absorbée comme texte)
    assert!(
        result.is_ok(),
        "erreur d'outil ne doit pas être fatale : {result:?}"
    );
    assert_eq!(result.unwrap(), "réponse malgré erreur");
}
