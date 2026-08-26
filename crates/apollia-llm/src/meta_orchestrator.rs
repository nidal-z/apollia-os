//! `MetaLlmOrchestrator`, shared service for agent transparency.
//!
//! Reuses the user-configured `LlmRouter` to produce the transparency
//! artifacts shown in the frontend (tool rationale, summaries, session title,
//! error explanation, etc.). No dedicated model: the cost policy is never a new
//! backend, always the LLM already paid for.
//!
//! # Architecture
//!
//! Tokio actor (`mpsc::channel` plus a cloneable handle) with:
//! - an LRU cache keyed by `(routine, SHA-256(inputs))`, size 512, TTL 15 min;
//! - a per-session `AtomicU64` budget tracker, default 10_000 tokens/session;
//! - prompt templates versioned via `include_str!` from `prompts/meta/*.md`;
//! - a `MetaLlmSettings { enabled: false, per_routine }` toggle (strict opt-in);
//! - a 10 s timeout that falls back to `None` (the UI shows static text);
//! - emission of `RuntimeEvent::MetaLlmBudgetExceeded` when the budget is exceeded.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use apollia_core::decision_point::{DecisionKind, DecisionPoint};
use apollia_core::events::{EventBusSender, RuntimeEvent, ToolCallRationale};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, oneshot};

use crate::router::LlmRouter;
use crate::types::{ChatMessage, CompletionRequest, LlmError};

// ── Constants ─────────────────────────────────────────────────────────────

/// LRU cache capacity shared across all routines.
const CACHE_CAPACITY: usize = 512;

/// Lifetime of a cache entry (15 minutes).
const CACHE_TTL: Duration = Duration::from_secs(15 * 60);

/// Meta LLM call timeout; beyond it, fall back to `None`.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// Default budget in tokens consumed per session (all routines combined).
pub const DEFAULT_SESSION_BUDGET_TOKENS: u64 = 10_000;

// ── Routines ──────────────────────────────────────────────────────────────

/// Typed enum listing every supported meta generation routine.
///
/// Each variant maps to a `prompts/meta/*.md` file embedded via `include_str!`.
/// To add a routine: create the template, then add the variant plus the
/// `include_str!` line in [`MetaRoutine::prompt_template`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetaRoutine {
    /// Short explanation of why a tool is being called.
    GenerateToolCallRationale,
    /// Summary of a thinking trace.
    GenerateThinkingSummary,
    /// Overall session summary.
    GenerateSessionSummary,
    /// Suggestion of next steps.
    GenerateNextSteps,
    /// Short session title.
    GenerateSessionTitle,
    /// Plain-language explanation of an error.
    GenerateErrorExplanation,
    /// Likely consequences of an AskUser question.
    GenerateAskUserConsequences,
    /// Alternative branches of a plan.
    GenerateAlternativeBranches,
    /// Risk assessment of an action.
    GenerateRiskAssessment,
    /// Check for possible hallucinations.
    GenerateHallucinationCheck,
    /// Session-level aggregated hallucination risk score.
    GenerateHallucinationRisk,
}

impl MetaRoutine {
    /// Return the embedded Markdown template for this routine.
    pub fn prompt_template(self) -> &'static str {
        match self {
            Self::GenerateToolCallRationale => {
                include_str!("../prompts/meta/tool_call_rationale.md")
            }
            Self::GenerateThinkingSummary => include_str!("../prompts/meta/thinking_summary.md"),
            Self::GenerateSessionSummary => include_str!("../prompts/meta/session_summary.md"),
            Self::GenerateNextSteps => include_str!("../prompts/meta/next_steps.md"),
            Self::GenerateSessionTitle => include_str!("../prompts/meta/session_title.md"),
            Self::GenerateErrorExplanation => include_str!("../prompts/meta/error_explanation.md"),
            Self::GenerateAskUserConsequences => {
                include_str!("../prompts/meta/ask_user_consequences.md")
            }
            Self::GenerateAlternativeBranches => {
                include_str!("../prompts/meta/alternative_branches.md")
            }
            Self::GenerateRiskAssessment => include_str!("../prompts/meta/risk_assessment.md"),
            Self::GenerateHallucinationCheck => {
                include_str!("../prompts/meta/hallucination_check.md")
            }
            Self::GenerateHallucinationRisk => {
                include_str!("../prompts/meta/hallucination_risk.md")
            }
        }
    }

    /// `true` if the routine must stay off even when the main toggle is on;
    /// the user must enable it explicitly via `per_routine` (e.g.
    /// `routines.decision_branches` for `GenerateAlternativeBranches`).
    pub fn is_opt_in_by_default(self) -> bool {
        matches!(self, Self::GenerateAlternativeBranches)
    }

    /// All variants, useful to iterate in the Settings UI and in tests.
    pub const ALL: [MetaRoutine; 11] = [
        Self::GenerateToolCallRationale,
        Self::GenerateThinkingSummary,
        Self::GenerateSessionSummary,
        Self::GenerateNextSteps,
        Self::GenerateSessionTitle,
        Self::GenerateErrorExplanation,
        Self::GenerateAskUserConsequences,
        Self::GenerateAlternativeBranches,
        Self::GenerateRiskAssessment,
        Self::GenerateHallucinationCheck,
        Self::GenerateHallucinationRisk,
    ];
}

// ── Settings ──────────────────────────────────────────────────────────────

/// User-persistable (SQLite) configuration of the meta service.
///
/// `enabled` is the "Enable AI narration" main toggle; `per_routine` lets the
/// user disable an individual routine even when the main toggle is on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaLlmSettings {
    /// Main toggle, default `false` (strict opt-in).
    #[serde(default)]
    pub enabled: bool,
    /// Per-routine overrides; if absent, the routine inherits `enabled`.
    #[serde(default)]
    pub per_routine: HashMap<MetaRoutine, bool>,
    /// Tokens/session budget (all routines combined).
    #[serde(default = "default_session_budget")]
    pub session_budget_tokens: u64,
}

fn default_session_budget() -> u64 {
    DEFAULT_SESSION_BUDGET_TOKENS
}

impl Default for MetaLlmSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            per_routine: HashMap::new(),
            session_budget_tokens: DEFAULT_SESSION_BUDGET_TOKENS,
        }
    }
}

impl MetaLlmSettings {
    /// Whether the routine should run for this config.
    ///
    /// Some routines are strict opt-in even with master `enabled = true`: they
    /// cost one LLM call per occurrence (e.g. `GenerateAlternativeBranches`) and
    /// must be enabled explicitly via `per_routine`.
    pub fn is_routine_enabled(&self, routine: MetaRoutine) -> bool {
        if !self.enabled {
            return false;
        }
        let default = !routine.is_opt_in_by_default();
        self.per_routine.get(&routine).copied().unwrap_or(default)
    }
}

// ── Budget ────────────────────────────────────────────────────────────────

/// Immutable snapshot of a session's budget, exposed via Tauri.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaLlmBudget {
    /// Tracked session identifier.
    pub session_id: String,
    /// Tokens consumed cumulatively since the counter was created.
    pub tokens_used: u64,
    /// Configured budget (tokens/session).
    pub budget: u64,
    /// `true` once `tokens_used >= budget`.
    pub exceeded: bool,
}

#[derive(Debug)]
struct SessionCounter {
    tokens_used: AtomicU64,
    budget: u64,
    exceeded_emitted: AtomicU64,
}

impl SessionCounter {
    fn new(budget: u64) -> Self {
        Self {
            tokens_used: AtomicU64::new(0),
            budget,
            exceeded_emitted: AtomicU64::new(0),
        }
    }
}

/// Per-session budget tracker, guarded by a short Mutex (never held across async).
#[derive(Debug, Default)]
struct BudgetTracker {
    sessions: HashMap<String, Arc<SessionCounter>>,
}

impl BudgetTracker {
    fn counter(&mut self, session_id: &str, budget: u64) -> Arc<SessionCounter> {
        self.sessions
            .entry(session_id.to_owned())
            .or_insert_with(|| Arc::new(SessionCounter::new(budget)))
            .clone()
    }

    fn snapshot(&self, session_id: &str) -> Option<MetaLlmBudget> {
        self.sessions.get(session_id).map(|c| {
            let used = c.tokens_used.load(Ordering::Relaxed);
            MetaLlmBudget {
                session_id: session_id.to_owned(),
                tokens_used: used,
                budget: c.budget,
                exceeded: used >= c.budget,
            }
        })
    }
}

// ── Cache ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct CacheEntry {
    value: String,
    inserted_at: Instant,
}

/// Compute the cache key `(routine, SHA-256(canonical_json(inputs)))`.
fn cache_key(routine: MetaRoutine, inputs: &serde_json::Value) -> String {
    let canonical = canonical_json(inputs);
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let digest = hasher.finalize();
    format!("{:?}::{:x}", routine, digest)
}

/// Serialize a `serde_json::Value` into canonical JSON (sorted keys).
fn canonical_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|k| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap_or_default(),
                        canonical_json(&map[k])
                    )
                })
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        serde_json::Value::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(canonical_json).collect();
            format!("[{}]", parts.join(","))
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

// ── Thinking summary ──────────────────────────────────────────────────────

/// Estimated quality level of a thinking trace by [`MetaRoutine::GenerateThinkingSummary`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingQuality {
    /// Vague, hesitant, or self-contradictory reasoning.
    Low,
    /// Coherent but shallow: jumps to conclusions without weighing alternatives.
    Medium,
    /// Explicit reasoning, weighed alternatives, decisions grounded in context.
    High,
}

/// Contradiction detected between the current trace and a previous turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThinkingContradiction {
    /// Identifier of the previous turn the contradiction is with.
    pub turn_id: String,
    /// Short excerpt (<= 30 words) of the relevant previous reasoning.
    pub excerpt: String,
}

/// Structured output of [`MetaRoutine::GenerateThinkingSummary`].
///
/// The `thinking_summary.md` prompt asks the LLM to return this JSON. Use
/// [`ThinkingSummary::parse`] to deserialize the raw response (tolerates
/// possible Markdown backticks).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThinkingSummary {
    /// Natural-language summary of the key decision point (1-2 sentences).
    pub summary: String,
    /// Estimated quality of the reasoning.
    pub quality: ThinkingQuality,
    /// Contradiction detected with a previous turn, or `None`.
    #[serde(default)]
    pub contradiction_with_previous: Option<ThinkingContradiction>,
}

impl ThinkingSummary {
    /// Deserialize a raw LLM response (trim + strip Markdown backticks).
    ///
    /// Returns `Err` if the JSON is invalid or required fields are absent.
    pub fn parse(raw: &str) -> Result<Self, serde_json::Error> {
        let clean = raw
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();
        serde_json::from_str(clean)
    }
}

// ── Prompt rendering ──────────────────────────────────────────────────────

/// Substitute `{{key}}` placeholders with the (stringified) `inputs` values.
pub fn render_prompt(template: &str, inputs: &serde_json::Value) -> String {
    let mut out = template.to_owned();
    if let Some(obj) = inputs.as_object() {
        for (k, v) in obj {
            let needle = format!("{{{{{}}}}}", k);
            let replacement = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            out = out.replace(&needle, &replacement);
        }
    }
    out
}

// ── Handle (cloneable) ────────────────────────────────────────────────────

/// A decision trace to turn into a structured [`DecisionPoint`] via
/// [`MetaOrchestratorHandle::generate_decision_point`].
///
/// Groups the turn identifier, the decision kind, the raw thinking trace and
/// the chosen action. The `session_id` stays passed separately because it
/// addresses the budget rather than the decision itself.
pub struct DecisionPointRequest<'a> {
    /// Identifier of the ReAct turn the decision attaches to.
    pub turn_id: &'a str,
    /// Decision category (tool choice, replanning, etc.).
    pub kind: DecisionKind,
    /// Raw thinking trace produced by the reasoner.
    pub thinking_raw: &'a str,
    /// Action actually chosen at the end of reasoning.
    pub chosen_action: &'a str,
}

/// Cloneable handle to the `MetaLlmOrchestrator` actor.
///
/// Sends a [`MetaCmd`] on the internal `mpsc`; the response arrives via a
/// caller-side `oneshot`. Returns `Ok(None)` when the routine is disabled, the
/// budget is exhausted, or the call timed out (static UI fallback).
#[derive(Clone)]
pub struct MetaOrchestratorHandle {
    tx: mpsc::Sender<MetaCmd>,
}

/// Command addressed to the `MetaLlmOrchestrator` actor.
#[derive(Debug)]
enum MetaCmd {
    Run {
        routine: MetaRoutine,
        inputs: serde_json::Value,
        session_id: String,
        reply: oneshot::Sender<Result<Option<String>, LlmError>>,
    },
    GetBudget {
        session_id: String,
        reply: oneshot::Sender<MetaLlmBudget>,
    },
}

impl MetaOrchestratorHandle {
    /// Run a routine. `Ok(None)` = disabled / budget exhausted / timeout.
    pub async fn run(
        &self,
        routine: MetaRoutine,
        inputs: serde_json::Value,
        session_id: impl Into<String>,
    ) -> Result<Option<String>, LlmError> {
        let (reply, rx) = oneshot::channel();
        let cmd = MetaCmd::Run {
            routine,
            inputs,
            session_id: session_id.into(),
            reply,
        };
        self.tx.send(cmd).await.map_err(|_| LlmError::Cancelled)?;
        rx.await.map_err(|_| LlmError::Cancelled)?
    }

    /// Generate a structured [`ToolCallRationale`] for a tool call.
    ///
    /// Returns `Ok(None)` if the routine is disabled / the budget is exhausted
    /// / the LLM call failed or timed out / the response could not be parsed as
    /// schema-conformant JSON. The UI must show a static fallback in these
    /// cases.
    pub async fn generate_tool_call_rationale(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        context: &str,
        session_id: impl Into<String>,
    ) -> Option<ToolCallRationale> {
        let performance_hint_default =
            crate::tool_performance_hints::format_hint(tool_name).unwrap_or_default();
        let inputs = serde_json::json!({
            "tool_name": tool_name,
            "arguments": serde_json::to_string(arguments).unwrap_or_default(),
            "context": context,
            "performance_hint_default": performance_hint_default,
        });
        let raw = self
            .run(MetaRoutine::GenerateToolCallRationale, inputs, session_id)
            .await
            .ok()??;
        ToolCallRationale::parse(&raw).ok()
    }

    /// Generate a structured [`DecisionPoint`] from a thinking trace.
    ///
    /// Calls the `GenerateAlternativeBranches` routine, which returns JSON
    /// `{ chosen, alternatives: [{ label, rejected_reason, confidence_delta }] }`.
    /// Returns `None` if the routine is disabled (opt-in
    /// `routines.decision_branches`, default off), the budget is exhausted, the
    /// LLM call fails, or the response does not parse. At most 3 alternatives
    /// are kept.
    ///
    /// The UI must show a silent fallback (no panel) in these cases.
    pub async fn generate_decision_point(
        &self,
        req: DecisionPointRequest<'_>,
        session_id: impl Into<String>,
    ) -> Option<DecisionPoint> {
        let DecisionPointRequest {
            turn_id,
            kind,
            thinking_raw,
            chosen_action,
        } = req;
        let inputs = serde_json::json!({
            "turn_id": turn_id,
            "thinking": thinking_raw,
            "chosen_action": chosen_action,
        });
        let raw = self
            .run(MetaRoutine::GenerateAlternativeBranches, inputs, session_id)
            .await
            .ok()??;
        DecisionPoint::parse(&raw, turn_id, kind).ok()
    }

    /// Return a snapshot of the session's budget.
    pub async fn get_budget(
        &self,
        session_id: impl Into<String>,
    ) -> Result<MetaLlmBudget, LlmError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(MetaCmd::GetBudget {
                session_id: session_id.into(),
                reply,
            })
            .await
            .map_err(|_| LlmError::Cancelled)?;
        rx.await.map_err(|_| LlmError::Cancelled)
    }
}

// ── Actor ─────────────────────────────────────────────────────────────────

/// `MetaLlmOrchestrator` Tokio actor.
///
/// Built via [`spawn`](Self::spawn). Loops on `mpsc::Receiver<MetaCmd>` and runs
/// each command sequentially (no cross-task shared state; the cache and counters
/// are private).
pub struct MetaLlmOrchestrator {
    router: Arc<LlmRouter>,
    bus: Option<EventBusSender>,
    settings: MetaLlmSettings,
    cache: LruCache<String, CacheEntry>,
    budget: Arc<Mutex<BudgetTracker>>,
}

impl MetaLlmOrchestrator {
    /// Start the actor and return a cloneable handle.
    ///
    /// The tokio task stops automatically once all handles are dropped.
    pub fn spawn(
        router: Arc<LlmRouter>,
        bus: Option<EventBusSender>,
        settings: MetaLlmSettings,
    ) -> MetaOrchestratorHandle {
        let (tx, rx) = mpsc::channel::<MetaCmd>(64);
        // SAFETY: `CACHE_CAPACITY` is a non-zero literal in this module, so
        // the conversion is decided at the point the constant is written.
        let cache_cap = NonZeroUsize::new(CACHE_CAPACITY).expect("CACHE_CAPACITY is zero");
        let actor = MetaLlmOrchestrator {
            router,
            bus,
            settings,
            cache: LruCache::new(cache_cap),
            budget: Arc::new(Mutex::new(BudgetTracker::default())),
        };
        tokio::spawn(actor.run(rx));
        MetaOrchestratorHandle { tx }
    }

    async fn run(mut self, mut rx: mpsc::Receiver<MetaCmd>) {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                MetaCmd::Run {
                    routine,
                    inputs,
                    session_id,
                    reply,
                } => {
                    let result = self.handle_run(routine, inputs, &session_id).await;
                    let _ = reply.send(result);
                }
                MetaCmd::GetBudget { session_id, reply } => {
                    let snapshot = self
                        .budget
                        .lock()
                        .ok()
                        .and_then(|b| b.snapshot(&session_id))
                        .unwrap_or(MetaLlmBudget {
                            session_id: session_id.clone(),
                            tokens_used: 0,
                            budget: self.settings.session_budget_tokens,
                            exceeded: false,
                        });
                    let _ = reply.send(snapshot);
                }
            }
        }
    }

    async fn handle_run(
        &mut self,
        routine: MetaRoutine,
        inputs: serde_json::Value,
        session_id: &str,
    ) -> Result<Option<String>, LlmError> {
        // Short-circuit: toggle off means no call and no consumption.
        if !self.settings.is_routine_enabled(routine) {
            tracing::debug!(routine = ?routine, "meta routine disabled - short-circuit");
            return Ok(None);
        }

        // Budget checked BEFORE the call: if already exceeded, refuse to emit.
        let counter = {
            let mut guard = self
                .budget
                .lock()
                .map_err(|_| LlmError::InferenceError("budget mutex poisoned".into()))?;
            guard.counter(session_id, self.settings.session_budget_tokens)
        };
        let used = counter.tokens_used.load(Ordering::Relaxed);
        if used >= counter.budget {
            tracing::info!(
                routine = ?routine,
                session_id = %session_id,
                used,
                budget = counter.budget,
                "meta budget exceeded - short-circuit"
            );
            return Ok(None);
        }

        // Cache lookup.
        let key = cache_key(routine, &inputs);
        if let Some(entry) = self.cache.get(&key) {
            if entry.inserted_at.elapsed() < CACHE_TTL {
                let value = entry.value.clone();
                tracing::info!(hit = true, routine = ?routine, "meta cache");
                return Ok(Some(value));
            }
            // Expired, will be overwritten below.
        }
        tracing::info!(hit = false, routine = ?routine, "meta cache");

        // Render prompt, call LLM with timeout.
        let prompt = render_prompt(routine.prompt_template(), &inputs);
        let req = CompletionRequest {
            messages: vec![ChatMessage::user(prompt)],
            temperature: Some(0.2),
            max_tokens: Some(256),
            ..Default::default()
        };
        let backend = self
            .router
            .get(None)
            .ok_or_else(|| LlmError::BackendUnavailable {
                backend: "meta".into(),
                reason: "no default backend in router".into(),
            })?;

        let call = backend.complete(req);
        let response = match tokio::time::timeout(CALL_TIMEOUT, call).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                tracing::warn!(routine = ?routine, error = %e, "meta llm call failed - fallback None");
                return Ok(None);
            }
            Err(_) => {
                tracing::warn!(routine = ?routine, "meta llm timeout - fallback None");
                return Ok(None);
            }
        };

        // Update budget + emit MetaLlmBudgetExceeded once per session.
        let tokens_this_call = response.usage.total_tokens();
        let total = counter
            .tokens_used
            .fetch_add(tokens_this_call, Ordering::Relaxed)
            + tokens_this_call;
        if total >= counter.budget
            && counter
                .exceeded_emitted
                .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        {
            if let Some(bus) = &self.bus {
                let _ = bus.send(RuntimeEvent::MetaLlmBudgetExceeded {
                    session_id: session_id.to_owned(),
                    tokens_used: total,
                    budget: counter.budget,
                });
            }
        }

        let value = response.content.trim().to_owned();
        self.cache.put(
            key,
            CacheEntry {
                value: value.clone(),
                inserted_at: Instant::now(),
            },
        );
        Ok(Some(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    use apollia_core::events::RuntimeEvent;
    use futures::Stream;
    use tokio::sync::broadcast;

    use crate::types::{CompletionResponse, FinishReason, StreamChunk, TokenUsage};
    use crate::{CompletionModel, CompletionRequest};

    // ── Mock backend with a call counter and configurable delay ───────────

    struct CountingBackend {
        calls: Arc<AtomicUsize>,
        delay: Duration,
        prompt_tokens: u32,
        completion_tokens: u32,
    }

    impl CountingBackend {
        fn new() -> (Arc<Self>, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            let backend = Arc::new(Self {
                calls: calls.clone(),
                delay: Duration::from_millis(0),
                prompt_tokens: 100,
                completion_tokens: 50,
            });
            (backend, calls)
        }

        fn with_tokens(prompt: u32, completion: u32) -> (Arc<Self>, Arc<AtomicUsize>) {
            let calls = Arc::new(AtomicUsize::new(0));
            let backend = Arc::new(Self {
                calls: calls.clone(),
                delay: Duration::from_millis(0),
                prompt_tokens: prompt,
                completion_tokens: completion,
            });
            (backend, calls)
        }
    }

    #[async_trait::async_trait]
    impl CompletionModel for CountingBackend {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            self.calls.fetch_add(1, AtomicOrdering::SeqCst);
            if !self.delay.is_zero() {
                tokio::time::sleep(self.delay).await;
            }
            Ok(CompletionResponse {
                engine_timings: None,
                content: "meta answer".to_owned(),
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: self.prompt_tokens,
                    completion_tokens: self.completion_tokens,
                    cost_usd: None,
                    ..Default::default()
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 1,
                ttft_ms: None,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            Ok(Box::pin(futures::stream::empty()))
        }

        fn is_available(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &str {
            "mock-meta"
        }
        fn model_id(&self) -> &str {
            "mock-meta"
        }
    }

    fn router_with(backend: Arc<dyn CompletionModel>) -> Arc<LlmRouter> {
        let mut map = HashMap::new();
        map.insert("mock".to_owned(), backend);
        Arc::new(LlmRouter::with_backends(map, "mock"))
    }

    fn enabled_settings() -> MetaLlmSettings {
        MetaLlmSettings {
            enabled: true,
            per_routine: HashMap::new(),
            session_budget_tokens: DEFAULT_SESSION_BUDGET_TOKENS,
        }
    }

    // GIVEN an enabled orchestrator with an empty cache
    // WHEN run() is called twice with the same inputs
    // THEN the backend is called only once (cache hit)
    #[tokio::test]
    async fn meta_cache_hit_avoids_second_call() {
        let (backend, calls) = CountingBackend::new();
        let router = router_with(backend as Arc<dyn CompletionModel>);
        let handle = MetaLlmOrchestrator::spawn(router, None, enabled_settings());

        let inputs = serde_json::json!({ "tool_name": "bash", "arguments": "ls", "context": "" });
        let r1 = handle
            .run(MetaRoutine::GenerateToolCallRationale, inputs.clone(), "s1")
            .await
            .unwrap();
        let r2 = handle
            .run(MetaRoutine::GenerateToolCallRationale, inputs, "s1")
            .await
            .unwrap();

        assert_eq!(r1, r2);
        assert_eq!(
            calls.load(AtomicOrdering::SeqCst),
            1,
            "second call must hit the cache"
        );
    }

    // GIVEN a disabled orchestrator (main toggle off)
    // WHEN run() is called
    // THEN Ok(None) with no LLM call
    #[tokio::test]
    async fn meta_toggle_off_short_circuits() {
        let (backend, calls) = CountingBackend::new();
        let router = router_with(backend as Arc<dyn CompletionModel>);
        let handle = MetaLlmOrchestrator::spawn(router, None, MetaLlmSettings::default());

        let result = handle
            .run(
                MetaRoutine::GenerateSessionTitle,
                serde_json::json!({ "first_message": "hello" }),
                "sess",
            )
            .await
            .unwrap();

        assert!(result.is_none(), "disabled routine must return None");
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
    }

    // GIVEN a very low budget (100 tokens) and a backend that consumes 150 tokens/call
    // WHEN we make 2 distinct calls (cache miss)
    // THEN the 2nd returns None + RuntimeEvent::MetaLlmBudgetExceeded emitted
    #[tokio::test]
    async fn meta_budget_exceeded_emits_event_and_short_circuits() {
        let (backend, _calls) = CountingBackend::with_tokens(100, 50);
        let router = router_with(backend as Arc<dyn CompletionModel>);

        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);
        let settings = MetaLlmSettings {
            enabled: true,
            per_routine: HashMap::new(),
            session_budget_tokens: 100,
        };
        let handle = MetaLlmOrchestrator::spawn(router, Some(tx), settings);

        // 1st call: consumes 150 tokens, exceeds budget, event emitted
        let first = handle
            .run(
                MetaRoutine::GenerateSessionTitle,
                serde_json::json!({ "first_message": "a" }),
                "sess-budget",
            )
            .await
            .unwrap();
        assert!(first.is_some());

        // 2nd call (different inputs to avoid the cache): short-circuit, None.
        let second = handle
            .run(
                MetaRoutine::GenerateSessionTitle,
                serde_json::json!({ "first_message": "b" }),
                "sess-budget",
            )
            .await
            .unwrap();
        assert!(second.is_none(), "budget-exceeded routine must return None");

        // The event was emitted exactly once.
        let evt = rx.try_recv().expect("budget exceeded event emitted");
        assert!(matches!(
            evt,
            RuntimeEvent::MetaLlmBudgetExceeded { ref session_id, budget: 100, .. }
            if session_id == "sess-budget"
        ));
    }

    // GIVEN a backend that returns an error (timeout/unavailable scenario)
    // WHEN run() is called
    // THEN Ok(None) without propagation (static UI fallback)
    //
    // Note: the `tokio::time::timeout` logic wrapping the call is tested
    // implicitly; this branch covers the `Ok(Err)` of `timeout(...)`, which
    // shares the same handling (`Ok(None)`) as the `Err(_)` timeout branch.
    #[tokio::test]
    async fn meta_llm_error_returns_none() {
        struct FailingBackend;
        #[async_trait::async_trait]
        impl CompletionModel for FailingBackend {
            async fn complete(
                &self,
                _req: CompletionRequest,
            ) -> Result<CompletionResponse, LlmError> {
                Err(LlmError::InferenceError("boom".into()))
            }
            async fn stream(
                &self,
                _req: CompletionRequest,
            ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
            {
                Ok(Box::pin(futures::stream::empty()))
            }
            fn is_available(&self) -> bool {
                true
            }
            fn backend_name(&self) -> &str {
                "fail"
            }
            fn model_id(&self) -> &str {
                "fail"
            }
        }

        let router = router_with(Arc::new(FailingBackend) as Arc<dyn CompletionModel>);
        let handle = MetaLlmOrchestrator::spawn(router, None, enabled_settings());

        let result = handle
            .run(
                MetaRoutine::GenerateThinkingSummary,
                serde_json::json!({ "thinking": "x" }),
                "s",
            )
            .await
            .unwrap();
        assert!(result.is_none(), "backend error must fall back to None");
    }

    // GIVEN a routine disabled via `per_routine` (main on but routine off)
    // WHEN run() is called for this routine
    // THEN Ok(None) without an LLM call
    #[tokio::test]
    async fn meta_per_routine_override_disables() {
        let (backend, calls) = CountingBackend::new();
        let router = router_with(backend as Arc<dyn CompletionModel>);

        let mut settings = enabled_settings();
        settings
            .per_routine
            .insert(MetaRoutine::GenerateRiskAssessment, false);
        let handle = MetaLlmOrchestrator::spawn(router, None, settings);

        // Disabled routine: None.
        let risk = handle
            .run(
                MetaRoutine::GenerateRiskAssessment,
                serde_json::json!({ "action": "delete", "context": "" }),
                "s",
            )
            .await
            .unwrap();
        assert!(risk.is_none());

        // Another routine still active: LLM call.
        let title = handle
            .run(
                MetaRoutine::GenerateSessionTitle,
                serde_json::json!({ "first_message": "hi" }),
                "s",
            )
            .await
            .unwrap();
        assert!(title.is_some());
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
    }

    // GIVEN two semantically identical inputs but unsorted JSON keys
    // WHEN the cache key is computed
    // THEN the key is stable (canonical_json sorts the keys)
    #[test]
    fn cache_key_is_canonical() {
        let a = serde_json::json!({ "b": 1, "a": 2 });
        let b = serde_json::json!({ "a": 2, "b": 1 });
        let ka = cache_key(MetaRoutine::GenerateSessionTitle, &a);
        let kb = cache_key(MetaRoutine::GenerateSessionTitle, &b);
        assert_eq!(ka, kb);
    }

    // GIVEN settings master=false, routine not listed
    // WHEN is_routine_enabled()
    // THEN false regardless of the routine
    #[test]
    fn settings_is_routine_enabled_respects_master() {
        let s = MetaLlmSettings::default();
        for r in MetaRoutine::ALL {
            assert!(!s.is_routine_enabled(r));
        }
    }

    // ─── DecisionPoint extraction ───

    /// GIVEN an orchestrator with `GenerateAlternativeBranches` disabled
    ///   (opt-in, off by default, even with master=on)
    /// WHEN generate_decision_point() is called
    /// THEN returns None without an LLM call
    #[tokio::test]
    async fn decision_point_opt_in_default_off() {
        let (backend, calls) = CountingBackend::new();
        let router = router_with(backend as Arc<dyn CompletionModel>);
        // master=on but no per_routine override, so GenerateAlternativeBranches OFF
        let handle = MetaLlmOrchestrator::spawn(router, None, enabled_settings());

        let dp = handle
            .generate_decision_point(
                DecisionPointRequest {
                    turn_id: "turn-1",
                    kind: DecisionKind::ToolChoice,
                    thinking_raw: "let me think…",
                    chosen_action: "read_file",
                },
                "sess-opt",
            )
            .await;

        assert!(dp.is_none(), "opt-in routine must be off by default");
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
    }

    /// GIVEN an orchestrator with the routine explicitly enabled and a
    ///   backend that returns a valid DecisionPoint JSON
    /// WHEN generate_decision_point() is called
    /// THEN a parsed DecisionPoint is returned with its 2 alternatives
    #[tokio::test]
    async fn decision_point_parses_structured_payload() {
        struct FixedBackend {
            calls: Arc<AtomicUsize>,
        }
        #[async_trait::async_trait]
        impl CompletionModel for FixedBackend {
            async fn complete(
                &self,
                _req: CompletionRequest,
            ) -> Result<CompletionResponse, LlmError> {
                self.calls.fetch_add(1, AtomicOrdering::SeqCst);
                Ok(CompletionResponse {
                    engine_timings: None,
                    content: r#"{"chosen":"read_file","alternatives":[
                        {"label":"bash","rejected_reason":"overkill","confidence_delta":-0.3},
                        {"label":"grep","rejected_reason":"path known","confidence_delta":-0.5}
                    ]}"#
                    .into(),
                    tool_calls: vec![],
                    usage: TokenUsage {
                        prompt_tokens: 50,
                        completion_tokens: 50,
                        cost_usd: None,
                        ..Default::default()
                    },
                    finish_reason: FinishReason::Stop,
                    latency_ms: 1,
                    ttft_ms: None,
                })
            }
            async fn stream(
                &self,
                _req: CompletionRequest,
            ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
            {
                Ok(Box::pin(futures::stream::empty()))
            }
            fn is_available(&self) -> bool {
                true
            }
            fn backend_name(&self) -> &str {
                "fixed"
            }
            fn model_id(&self) -> &str {
                "fixed"
            }
        }

        let calls = Arc::new(AtomicUsize::new(0));
        let router = router_with(Arc::new(FixedBackend {
            calls: calls.clone(),
        }) as Arc<dyn CompletionModel>);

        let mut settings = enabled_settings();
        settings
            .per_routine
            .insert(MetaRoutine::GenerateAlternativeBranches, true);
        let handle = MetaLlmOrchestrator::spawn(router, None, settings);

        let dp = handle
            .generate_decision_point(
                DecisionPointRequest {
                    turn_id: "turn-77",
                    kind: DecisionKind::ToolChoice,
                    thinking_raw: "I should read the file before running bash…",
                    chosen_action: "read_file",
                },
                "sess-dp",
            )
            .await
            .expect("decision point expected");

        assert_eq!(dp.turn_id, "turn-77");
        assert_eq!(dp.kind, DecisionKind::ToolChoice);
        assert_eq!(dp.chosen, "read_file");
        assert_eq!(dp.alternatives.len(), 2);
        assert_eq!(dp.alternatives[0].label, "bash");
        assert!(dp.alternatives[0].confidence_delta < 0.0);
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
    }

    // ─── ThinkingSummary parsing ───

    /// GIVEN a valid JSON response with all fields
    /// WHEN ThinkingSummary::parse()
    /// THEN the fields are deserialized correctly
    #[test]
    fn thinking_summary_parses_full_payload() {
        let raw = r#"{
            "summary": "Chose to read the config before editing.",
            "quality": "high",
            "contradiction_with_previous": {
                "turn_id": "turn-3",
                "excerpt": "I already read the config."
            }
        }"#;
        let ts = ThinkingSummary::parse(raw).expect("parse ok");
        assert_eq!(ts.quality, ThinkingQuality::High);
        let c = ts.contradiction_with_previous.expect("some contradiction");
        assert_eq!(c.turn_id, "turn-3");
    }

    /// GIVEN a response with Markdown backticks and null contradiction
    /// WHEN ThinkingSummary::parse()
    /// THEN the fences are stripped and contradiction_with_previous = None
    #[test]
    fn thinking_summary_strips_backticks_and_allows_null_contradiction() {
        let raw = "```json\n{\"summary\":\"s\",\"quality\":\"medium\",\"contradiction_with_previous\":null}\n```";
        let ts = ThinkingSummary::parse(raw).expect("parse ok");
        assert_eq!(ts.quality, ThinkingQuality::Medium);
        assert!(ts.contradiction_with_previous.is_none());
    }

    /// GIVEN a response where contradiction_with_previous is absent
    /// WHEN ThinkingSummary::parse()
    /// THEN serde default yields None
    #[test]
    fn thinking_summary_defaults_missing_contradiction_to_none() {
        let raw = r#"{"summary":"s","quality":"low"}"#;
        let ts = ThinkingSummary::parse(raw).expect("parse ok");
        assert_eq!(ts.quality, ThinkingQuality::Low);
        assert!(ts.contradiction_with_previous.is_none());
    }

    // GIVEN a prompt template with placeholders and matching inputs
    // WHEN render_prompt()
    // THEN the placeholders are substituted
    #[test]
    fn render_prompt_substitutes_placeholders() {
        let tpl = "hello {{name}}, you have {{count}} items";
        let inputs = serde_json::json!({ "name": "alice", "count": 3 });
        let out = render_prompt(tpl, &inputs);
        assert_eq!(out, "hello alice, you have 3 items");
    }
}
