//! `MetaLlmOrchestrator` — service partagé pour la transparence des agents (ADR-073).
//!
//! Réutilise le `LlmRouter` configuré par l'utilisateur pour produire les artefacts
//! de transparence affichés dans le frontend (rationale d'outil, résumés, titre de
//! session, explication d'erreur, etc.). Pas de modèle dédié — la politique de coût
//! est : jamais de nouveau backend, toujours le LLM déjà payé.
//!
//! # Architecture
//!
//! Acteur Tokio (`mpsc::channel` + handle clonable) avec :
//! - cache LRU keyed `(routine, SHA-256(inputs))`, taille 512, TTL 15 min ;
//! - budget tracker `AtomicU64` par session, défaut 10_000 tokens/session ;
//! - prompt templates versionnés en `include_str!` depuis `prompts/meta/*.md` ;
//! - toggle `MetaLlmSettings { enabled: false, per_routine }` — opt-in strict ;
//! - timeout 10 s → fallback `None` (l'UI affiche un texte statique) ;
//! - émission `RuntimeEvent::MetaLlmBudgetExceeded` si dépassement du budget.

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

// ─────────────────────────────────────────────
// Constants
// ─────────────────────────────────────────────

/// Capacité du cache LRU partagé entre toutes les routines.
const CACHE_CAPACITY: usize = 512;

/// Durée de vie d'une entrée dans le cache (15 minutes).
const CACHE_TTL: Duration = Duration::from_secs(15 * 60);

/// Timeout d'un appel LLM méta — au-delà, fallback à `None`.
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// Budget par défaut en tokens consommés par session (toutes routines confondues).
pub const DEFAULT_SESSION_BUDGET_TOKENS: u64 = 10_000;

// ─────────────────────────────────────────────
// Routines
// ─────────────────────────────────────────────

/// Enum typée listant toutes les routines de génération méta supportées.
///
/// Chaque variante correspond à un fichier `prompts/meta/*.md` embarqué via
/// `include_str!`. Pour ajouter une routine : créer le template puis ajouter
/// la variante + la ligne `include_str!` dans [`MetaRoutine::prompt_template`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetaRoutine {
    /// Explication courte du pourquoi d'un appel d'outil.
    GenerateToolCallRationale,
    /// Résumé d'une trace de thinking.
    GenerateThinkingSummary,
    /// Résumé global de session.
    GenerateSessionSummary,
    /// Proposition de prochaines étapes.
    GenerateNextSteps,
    /// Titre court de session.
    GenerateSessionTitle,
    /// Explication en langage simple d'une erreur.
    GenerateErrorExplanation,
    /// Conséquences probables d'une question AskUser.
    GenerateAskUserConsequences,
    /// Branches alternatives d'un plan.
    GenerateAlternativeBranches,
    /// Évaluation de risque d'une action.
    GenerateRiskAssessment,
    /// Vérification de possibles hallucinations.
    GenerateHallucinationCheck,
    /// Score de risque d'hallucination agrégé au niveau session (US-SP42-048).
    GenerateHallucinationRisk,
}

impl MetaRoutine {
    /// Retourne le template Markdown embarqué pour cette routine.
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

    /// `true` si la routine doit rester désactivée même quand le master toggle
    /// est on — l'utilisateur doit l'activer explicitement via `per_routine`
    /// (ex. `routines.decision_branches` pour `GenerateAlternativeBranches`).
    pub fn is_opt_in_by_default(self) -> bool {
        matches!(self, Self::GenerateAlternativeBranches)
    }

    /// Toutes les variantes — utile pour itérer côté Settings UI et tests.
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

// ─────────────────────────────────────────────
// Settings
// ─────────────────────────────────────────────

/// Configuration utilisateur persistable (SQLite) du service méta.
///
/// `enabled` est le master toggle "Enable AI narration" ; `per_routine` permet
/// de désactiver individuellement une routine même si le master est actif.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaLlmSettings {
    /// Master toggle — défaut `false` (opt-in strict).
    #[serde(default)]
    pub enabled: bool,
    /// Overrides par routine ; si absent, la routine hérite de `enabled`.
    #[serde(default)]
    pub per_routine: HashMap<MetaRoutine, bool>,
    /// Budget tokens/session (toutes routines confondues).
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
    /// Indique si la routine doit être exécutée pour cette config.
    ///
    /// Certaines routines sont opt-in strict même avec master `enabled = true` —
    /// elles coûtent un appel LLM par occurrence (ex. `GenerateAlternativeBranches`,
    /// US-SP42-041) et doivent être activées explicitement via `per_routine`.
    pub fn is_routine_enabled(&self, routine: MetaRoutine) -> bool {
        if !self.enabled {
            return false;
        }
        let default = !routine.is_opt_in_by_default();
        self.per_routine.get(&routine).copied().unwrap_or(default)
    }
}

// ─────────────────────────────────────────────
// Budget
// ─────────────────────────────────────────────

/// Snapshot immuable du budget d'une session, exposé via Tauri.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaLlmBudget {
    /// Identifiant de session suivi.
    pub session_id: String,
    /// Tokens consommés cumulés depuis la création du compteur.
    pub tokens_used: u64,
    /// Budget configuré (tokens/session).
    pub budget: u64,
    /// `true` dès que `tokens_used >= budget`.
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

/// Tracker de budget par session, protégé par un Mutex court (jamais tenu async).
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

// ─────────────────────────────────────────────
// Cache
// ─────────────────────────────────────────────

#[derive(Debug, Clone)]
struct CacheEntry {
    value: String,
    inserted_at: Instant,
}

/// Calcule la clé de cache `(routine, SHA-256(canonical_json(inputs)))`.
fn cache_key(routine: MetaRoutine, inputs: &serde_json::Value) -> String {
    let canonical = canonical_json(inputs);
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let digest = hasher.finalize();
    format!("{:?}::{:x}", routine, digest)
}

/// Sérialise un `serde_json::Value` en JSON canonique (clés triées).
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

// ─────────────────────────────────────────────
// Thinking summary (US-SP42-037)
// ─────────────────────────────────────────────

/// Niveau de qualité estimé d'une trace de thinking par [`MetaRoutine::GenerateThinkingSummary`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingQuality {
    /// Raisonnement vague, hésitant, ou contradictoire avec lui-même.
    Low,
    /// Cohérent mais superficiel — saute aux conclusions sans examiner d'alternatives.
    Medium,
    /// Raisonnement explicite, alternatives pondérées, décisions ancrées dans le contexte.
    High,
}

/// Contradiction détectée entre la trace courante et un tour précédent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThinkingContradiction {
    /// Identifiant du tour précédent avec lequel il y a contradiction.
    pub turn_id: String,
    /// Court extrait (≤ 30 mots) du raisonnement précédent concerné.
    pub excerpt: String,
}

/// Sortie structurée de [`MetaRoutine::GenerateThinkingSummary`].
///
/// Le prompt `thinking_summary.md` demande au LLM de retourner ce JSON.
/// Utiliser [`ThinkingSummary::parse`] pour désérialiser la réponse brute
/// (tolère les backticks Markdown éventuels).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThinkingSummary {
    /// Résumé en langage naturel du point de décision clé (1-2 phrases).
    pub summary: String,
    /// Qualité estimée du raisonnement.
    pub quality: ThinkingQuality,
    /// Contradiction détectée avec un tour précédent, ou `None`.
    #[serde(default)]
    pub contradiction_with_previous: Option<ThinkingContradiction>,
}

impl ThinkingSummary {
    /// Désérialise une réponse brute du LLM (trim + strip des backticks Markdown).
    ///
    /// Retourne `Err` si le JSON est invalide ou si les champs requis sont absents.
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

// ─────────────────────────────────────────────
// Prompt rendering
// ─────────────────────────────────────────────

/// Substitue les placeholders `{{key}}` par les valeurs de `inputs` (stringifiées).
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

// ─────────────────────────────────────────────
// Handle (clonable)
// ─────────────────────────────────────────────

/// Handle clonable vers l'acteur `MetaLlmOrchestrator`.
///
/// Envoie une [`MetaCmd`] sur le `mpsc` interne ; la réponse arrive via un
/// `oneshot` côté appelant. Retourne `Ok(None)` quand la routine est désactivée,
/// le budget épuisé, ou que l'appel a dépassé le timeout (fallback UI statique).
#[derive(Clone)]
pub struct MetaOrchestratorHandle {
    tx: mpsc::Sender<MetaCmd>,
}

/// Commande adressée à l'acteur `MetaLlmOrchestrator`.
#[derive(Debug)]
enum MetaCmd {
    Run {
        routine: MetaRoutine,
        inputs: serde_json::Value,
        session_id: String,
        reply: oneshot::Sender<Result<Option<String>, LlmError>>,
    },
    GetSettings {
        reply: oneshot::Sender<MetaLlmSettings>,
    },
    SetSettings {
        settings: MetaLlmSettings,
        reply: oneshot::Sender<()>,
    },
    GetBudget {
        session_id: String,
        reply: oneshot::Sender<MetaLlmBudget>,
    },
}

impl MetaOrchestratorHandle {
    /// Exécute une routine. `Ok(None)` = désactivée / budget épuisé / timeout.
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

    /// Génère un [`ToolCallRationale`] structuré pour un appel d'outil.
    ///
    /// Retourne `Ok(None)` si la routine est désactivée / le budget est
    /// épuisé / l'appel LLM a échoué ou dépassé le timeout / la réponse
    /// n'a pas pu être parsée en JSON conforme au schéma. L'UI doit
    /// afficher un fallback statique dans ces cas-là.
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

    /// Génère un [`DecisionPoint`] structuré depuis une trace de thinking.
    ///
    /// Appelle la routine `GenerateAlternativeBranches` qui renvoie un JSON
    /// `{ chosen, alternatives: [{ label, rejected_reason, confidence_delta }] }`.
    /// Retourne `None` si la routine est désactivée (opt-in `routines.decision_branches`,
    /// default off), si le budget est épuisé, si l'appel LLM échoue, ou si la
    /// réponse ne parse pas. Au plus 3 alternatives sont conservées.
    ///
    /// L'UI doit afficher un fallback silencieux (aucun panneau) dans ces cas.
    pub async fn generate_decision_point(
        &self,
        turn_id: &str,
        kind: DecisionKind,
        thinking_raw: &str,
        chosen_action: &str,
        session_id: impl Into<String>,
    ) -> Option<DecisionPoint> {
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

    /// Retourne la config courante.
    pub async fn get_settings(&self) -> Result<MetaLlmSettings, LlmError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(MetaCmd::GetSettings { reply })
            .await
            .map_err(|_| LlmError::Cancelled)?;
        rx.await.map_err(|_| LlmError::Cancelled)
    }

    /// Met à jour la config.
    pub async fn set_settings(&self, settings: MetaLlmSettings) -> Result<(), LlmError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(MetaCmd::SetSettings { settings, reply })
            .await
            .map_err(|_| LlmError::Cancelled)?;
        rx.await.map_err(|_| LlmError::Cancelled)
    }

    /// Retourne un snapshot du budget de la session.
    pub async fn get_budget(&self, session_id: impl Into<String>) -> Result<MetaLlmBudget, LlmError> {
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

// ─────────────────────────────────────────────
// Actor
// ─────────────────────────────────────────────

/// Acteur Tokio `MetaLlmOrchestrator`.
///
/// Construit via [`spawn`](Self::spawn). Boucle sur `mpsc::Receiver<MetaCmd>` et
/// exécute chaque commande séquentiellement (aucun état partagé cross-tâches ;
/// le cache et les compteurs sont privés).
pub struct MetaLlmOrchestrator {
    router: Arc<LlmRouter>,
    bus: Option<EventBusSender>,
    settings: MetaLlmSettings,
    cache: LruCache<String, CacheEntry>,
    budget: Arc<Mutex<BudgetTracker>>,
}

impl MetaLlmOrchestrator {
    /// Démarre l'acteur et retourne un handle clonable.
    ///
    /// Le task tokio s'arrête automatiquement quand tous les handles sont drop.
    pub fn spawn(
        router: Arc<LlmRouter>,
        bus: Option<EventBusSender>,
        settings: MetaLlmSettings,
    ) -> MetaOrchestratorHandle {
        let (tx, rx) = mpsc::channel::<MetaCmd>(64);
        let cache_cap = NonZeroUsize::new(CACHE_CAPACITY)
            .expect("CACHE_CAPACITY > 0");
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
                MetaCmd::GetSettings { reply } => {
                    let _ = reply.send(self.settings.clone());
                }
                MetaCmd::SetSettings { settings, reply } => {
                    self.settings = settings;
                    let _ = reply.send(());
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
        // Short-circuit : toggle off → aucun appel, aucune consommation.
        if !self.settings.is_routine_enabled(routine) {
            tracing::debug!(routine = ?routine, "meta routine disabled — short-circuit");
            return Ok(None);
        }

        // Budget vérifié AVANT l'appel — si déjà dépassé on refuse d'émettre.
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
                "meta budget exceeded — short-circuit"
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
            // Expired — will be overwritten below.
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
                tracing::warn!(routine = ?routine, error = %e, "meta llm call failed — fallback None");
                return Ok(None);
            }
            Err(_) => {
                tracing::warn!(routine = ?routine, "meta llm timeout — fallback None");
                return Ok(None);
            }
        };

        // Update budget + emit MetaLlmBudgetExceeded once per session.
        let tokens_this_call =
            u64::from(response.usage.prompt_tokens + response.usage.completion_tokens);
        let total = counter.tokens_used.fetch_add(tokens_this_call, Ordering::Relaxed)
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

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    use apollia_core::events::RuntimeEvent;
    use futures::Stream;
    use tokio::sync::broadcast;

    use crate::types::{
        CompletionResponse, FinishReason, StreamChunk, TokenUsage,
    };
    use crate::{CompletionModel, CompletionRequest};

    // ── Mock backend avec compteur d'appels et délai configurable ─────────

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
        ) -> Result<
            Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>,
            LlmError,
        > {
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

    // GIVEN orchestrator + settings enabled + cache vide
    // WHEN on appelle run() deux fois avec les mêmes inputs
    // THEN le backend n'est appelé qu'une seule fois (cache hit)
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
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1, "second call must hit the cache");
    }

    // GIVEN orchestrator + settings disabled (master toggle off)
    // WHEN on appelle run()
    // THEN Ok(None) sans appel LLM
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

    // GIVEN budget très bas (100 tokens) et backend qui consomme 150 tokens/appel
    // WHEN on fait 2 appels distincts (cache miss)
    // THEN le 2e retourne None + RuntimeEvent::MetaLlmBudgetExceeded émis
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

        // 1er appel : consomme 150 tokens → dépassement → event émis
        let first = handle
            .run(
                MetaRoutine::GenerateSessionTitle,
                serde_json::json!({ "first_message": "a" }),
                "sess-budget",
            )
            .await
            .unwrap();
        assert!(first.is_some());

        // 2e appel (inputs différents pour éviter le cache) → short-circuit, None.
        let second = handle
            .run(
                MetaRoutine::GenerateSessionTitle,
                serde_json::json!({ "first_message": "b" }),
                "sess-budget",
            )
            .await
            .unwrap();
        assert!(second.is_none(), "budget-exceeded routine must return None");

        // L'event a été émis exactement une fois.
        let evt = rx.try_recv().expect("budget exceeded event emitted");
        assert!(matches!(
            evt,
            RuntimeEvent::MetaLlmBudgetExceeded { ref session_id, budget: 100, .. }
            if session_id == "sess-budget"
        ));
    }

    // GIVEN un backend qui retourne une erreur (scénario timeout/unavailable)
    // WHEN on appelle run()
    // THEN Ok(None) sans propagation (fallback UI statique)
    //
    // Note : la logique `tokio::time::timeout` qui entoure l'appel est testée
    // implicitement — cette branche couvre l'`Ok(Err)` du `timeout(...)`, qui
    // partage le même traitement (`Ok(None)`) que la branche `Err(_)` timeout.
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
            ) -> Result<
                Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>,
                LlmError,
            > {
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

    // GIVEN routine désactivée par `per_routine` (master on mais routine off)
    // WHEN on appelle run() pour cette routine
    // THEN Ok(None) sans appel LLM
    #[tokio::test]
    async fn meta_per_routine_override_disables() {
        let (backend, calls) = CountingBackend::new();
        let router = router_with(backend as Arc<dyn CompletionModel>);

        let mut settings = enabled_settings();
        settings
            .per_routine
            .insert(MetaRoutine::GenerateRiskAssessment, false);
        let handle = MetaLlmOrchestrator::spawn(router, None, settings);

        // Routine désactivée → None.
        let risk = handle
            .run(
                MetaRoutine::GenerateRiskAssessment,
                serde_json::json!({ "action": "delete", "context": "" }),
                "s",
            )
            .await
            .unwrap();
        assert!(risk.is_none());

        // Autre routine toujours active → appel LLM.
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

    // GIVEN deux entrées sémantiquement identiques mais clés JSON non-triées
    // WHEN on calcule la clé de cache
    // THEN la clé est stable (canonical_json trie les clés)
    #[test]
    fn cache_key_is_canonical() {
        let a = serde_json::json!({ "b": 1, "a": 2 });
        let b = serde_json::json!({ "a": 2, "b": 1 });
        let ka = cache_key(MetaRoutine::GenerateSessionTitle, &a);
        let kb = cache_key(MetaRoutine::GenerateSessionTitle, &b);
        assert_eq!(ka, kb);
    }

    // GIVEN settings master=false, routine non listée
    // WHEN is_routine_enabled()
    // THEN false quelque soit la routine
    #[test]
    fn settings_is_routine_enabled_respects_master() {
        let s = MetaLlmSettings::default();
        for r in MetaRoutine::ALL {
            assert!(!s.is_routine_enabled(r));
        }
    }

    // ─── US-SP42-041 — DecisionPoint extraction ───

    /// GIVEN un orchestrator avec `GenerateAlternativeBranches` désactivée
    ///   (opt-in par défaut off, même avec master=on)
    /// WHEN generate_decision_point() est appelé
    /// THEN retourne None sans appel LLM
    #[tokio::test]
    async fn decision_point_opt_in_default_off() {
        let (backend, calls) = CountingBackend::new();
        let router = router_with(backend as Arc<dyn CompletionModel>);
        // master=on mais pas de per_routine override → GenerateAlternativeBranches OFF
        let handle = MetaLlmOrchestrator::spawn(router, None, enabled_settings());

        let dp = handle
            .generate_decision_point(
                "turn-1",
                DecisionKind::ToolChoice,
                "let me think…",
                "read_file",
                "sess-opt",
            )
            .await;

        assert!(dp.is_none(), "opt-in routine must be off by default");
        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
    }

    /// GIVEN un orchestrator avec la routine explicitement activée et un
    ///   backend qui renvoie un JSON DecisionPoint valide
    /// WHEN generate_decision_point() est appelé
    /// THEN un DecisionPoint parsé est retourné avec ses 2 alternatives
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
            ) -> Result<
                Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>,
                LlmError,
            > {
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
                "turn-77",
                DecisionKind::ToolChoice,
                "I should read the file before running bash…",
                "read_file",
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

    // ─── US-SP42-037 — ThinkingSummary parsing ───

    /// GIVEN une réponse JSON valide avec tous les champs
    /// WHEN ThinkingSummary::parse()
    /// THEN les champs sont désérialisés correctement
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

    /// GIVEN une réponse avec backticks Markdown et contradiction nulle
    /// WHEN ThinkingSummary::parse()
    /// THEN les fences sont strippées et contradiction_with_previous = None
    #[test]
    fn thinking_summary_strips_backticks_and_allows_null_contradiction() {
        let raw = "```json\n{\"summary\":\"s\",\"quality\":\"medium\",\"contradiction_with_previous\":null}\n```";
        let ts = ThinkingSummary::parse(raw).expect("parse ok");
        assert_eq!(ts.quality, ThinkingQuality::Medium);
        assert!(ts.contradiction_with_previous.is_none());
    }

    /// GIVEN une réponse où contradiction_with_previous est absent
    /// WHEN ThinkingSummary::parse()
    /// THEN serde default donne None
    #[test]
    fn thinking_summary_defaults_missing_contradiction_to_none() {
        let raw = r#"{"summary":"s","quality":"low"}"#;
        let ts = ThinkingSummary::parse(raw).expect("parse ok");
        assert_eq!(ts.quality, ThinkingQuality::Low);
        assert!(ts.contradiction_with_previous.is_none());
    }

    // GIVEN prompt template avec placeholders et inputs correspondants
    // WHEN render_prompt()
    // THEN les placeholders sont remplacés
    #[test]
    fn render_prompt_substitutes_placeholders() {
        let tpl = "hello {{name}}, you have {{count}} items";
        let inputs = serde_json::json!({ "name": "alice", "count": 3 });
        let out = render_prompt(tpl, &inputs);
        assert_eq!(out, "hello alice, you have 3 items");
    }
}
