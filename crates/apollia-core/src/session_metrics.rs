//! Métriques de session agrégées (US-SP42-047 Pattern P11).
//!
//! [`SessionMetrics`] agrège tokens, contexte, timings d'outils et événements
//! de summarization sur la durée d'une session. Calculé par
//! `apollia_runtime::session_metrics::SessionMetricsActor` et diffusé via
//! [`crate::events::RuntimeEvent::SessionMetricsUpdated`].

use serde::{Deserialize, Serialize};

/// Timing d'un appel outil avec delta par rapport au hint statique.
///
/// `expected_ms` provient de `tool_performance_hints.toml` (P2) et `actual_ms`
/// est mesuré par `ChatToolCallCompleted`. `delta_pct` = 100 × (actual - expected) / expected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolTiming {
    /// Nom logique de l'outil invoqué.
    pub tool_name: String,
    /// Durée attendue d'après `tool_performance_hints.toml` (ms). `None` si absent de la table.
    pub expected_ms: Option<u64>,
    /// Durée réelle observée (ms).
    pub actual_ms: u64,
    /// Écart en pourcentage ; positif = plus lent qu'attendu. `None` si `expected_ms` est `None`.
    pub delta_pct: Option<f32>,
}

impl ToolTiming {
    /// Construit un `ToolTiming` en calculant le delta si `expected_ms` est fourni.
    pub fn new(tool_name: impl Into<String>, expected_ms: Option<u64>, actual_ms: u64) -> Self {
        let delta_pct = expected_ms.and_then(|exp| {
            if exp == 0 {
                None
            } else {
                Some((actual_ms as f32 - exp as f32) / exp as f32 * 100.0)
            }
        });
        Self {
            tool_name: tool_name.into(),
            expected_ms,
            actual_ms,
            delta_pct,
        }
    }
}

/// Événement de summarization / compaction du contexte.
///
/// Émis par le ContextManager quand l'historique a été compacté. Permet au
/// frontend d'afficher un banner "N messages résumés" + preview du résumé.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SummarizationEvent {
    /// Nombre de messages originaux remplacés par le résumé.
    pub messages_summarized_count: usize,
    /// Tokens économisés (estimation : tokens_avant − tokens_apres).
    pub tokens_saved: u64,
    /// Extrait du résumé tronqué pour affichage (≤ 280 chars).
    pub summary_excerpt: String,
}

/// Niveau d'alerte budget en fonction des seuils configurés.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BudgetAlertLevel {
    /// Sous le seuil de warning — RAS.
    Ok,
    /// Au-dessus de `token_warn_pct`, affichage toast warning.
    Warning,
    /// Au-dessus de `token_block_pct`, approach bloquante.
    Block,
}

impl Default for BudgetAlertLevel {
    fn default() -> Self {
        Self::Ok
    }
}

/// Seuils configurables depuis `apollia.toml [session]`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SessionThresholds {
    /// Seuil de warning en pourcentage du budget (par défaut 80).
    pub token_warn_pct: u8,
    /// Seuil bloquant en pourcentage du budget (par défaut 100).
    pub token_block_pct: u8,
}

impl Default for SessionThresholds {
    fn default() -> Self {
        Self {
            token_warn_pct: 80,
            token_block_pct: 100,
        }
    }
}

impl SessionThresholds {
    /// Évalue le niveau d'alerte pour un usage donné par rapport au budget total.
    ///
    /// `budget` représente le nombre total de tokens autorisés. Retourne
    /// [`BudgetAlertLevel::Ok`] si `budget == 0` (budget non configuré).
    pub fn evaluate(&self, used: u64, budget: u64) -> BudgetAlertLevel {
        if budget == 0 {
            return BudgetAlertLevel::Ok;
        }
        let pct = (used as f64) * 100.0 / (budget as f64);
        if pct >= self.token_block_pct as f64 {
            BudgetAlertLevel::Block
        } else if pct >= self.token_warn_pct as f64 {
            BudgetAlertLevel::Warning
        } else {
            BudgetAlertLevel::Ok
        }
    }
}

/// Snapshot des métriques d'une session — tokens, contexte, timings, summarization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionMetrics {
    /// Tokens envoyés en entrée (prompt) cumulés.
    pub tokens_in: u64,
    /// Tokens générés en sortie cumulés.
    pub tokens_out: u64,
    /// Tokens lus depuis le cache (cache hits).
    pub tokens_cached: u64,
    /// Tokens consommés par les appels meta-LLM (narration, résumé).
    pub tokens_meta: u64,
    /// Tokens occupant actuellement la context window (estimation live).
    pub context_window_used: u64,
    /// Taille maximale de la context window du backend courant.
    pub context_window_max: u64,
    /// Budget total configuré pour la session (0 si non configuré).
    pub token_budget: u64,
    /// Historique des timings d'outils — capé à 100 dernières entrées.
    pub tool_timings: Vec<ToolTiming>,
    /// Historique des événements de summarization.
    pub summarization_events: Vec<SummarizationEvent>,
}

/// Nombre maximal de timings conservés dans [`SessionMetrics::tool_timings`].
///
/// Au-delà, les plus anciens sont évincés en FIFO pour borner la mémoire.
pub const TOOL_TIMINGS_MAX: usize = 100;

impl SessionMetrics {
    /// Fusionne les compteurs d'un appel LLM — incrémente `tokens_in`, `tokens_out`, `tokens_cached`.
    ///
    /// `is_meta = true` route les tokens vers `tokens_meta` au lieu des compteurs principaux.
    pub fn record_llm_call(
        &mut self,
        prompt_tokens: u32,
        completion_tokens: u32,
        cached_tokens: u32,
        is_meta: bool,
    ) {
        let total = u64::from(prompt_tokens) + u64::from(completion_tokens);
        if is_meta {
            self.tokens_meta = self.tokens_meta.saturating_add(total);
        } else {
            self.tokens_in = self.tokens_in.saturating_add(u64::from(prompt_tokens));
            self.tokens_out = self.tokens_out.saturating_add(u64::from(completion_tokens));
            self.tokens_cached = self.tokens_cached.saturating_add(u64::from(cached_tokens));
        }
        // Estimation rapide : context_window_used ≈ tokens_in + tokens_out
        // non-meta. Le ContextManager peut corriger via `set_context_window_used`.
        self.context_window_used = self.tokens_in.saturating_add(self.tokens_out);
    }

    /// Met à jour `context_window_used` à partir d'une mesure autoritaire.
    pub fn set_context_window_used(&mut self, used: u64) {
        self.context_window_used = used;
    }

    /// Ajoute un timing d'outil en respectant la borne [`TOOL_TIMINGS_MAX`].
    pub fn push_tool_timing(&mut self, timing: ToolTiming) {
        if self.tool_timings.len() >= TOOL_TIMINGS_MAX {
            self.tool_timings.remove(0);
        }
        self.tool_timings.push(timing);
    }

    /// Enregistre un événement de summarization.
    pub fn push_summarization(&mut self, event: SummarizationEvent) {
        self.summarization_events.push(event);
    }

    /// Somme des tokens comptés pour le budget (in + out + meta, cache exclu).
    pub fn tokens_used_for_budget(&self) -> u64 {
        self.tokens_in
            .saturating_add(self.tokens_out)
            .saturating_add(self.tokens_meta)
    }

    /// Pourcentage d'occupation de la context window (0.0 − 100.0).
    ///
    /// Retourne `0.0` si `context_window_max == 0` pour éviter la division par zéro.
    pub fn context_window_pct(&self) -> f32 {
        if self.context_window_max == 0 {
            return 0.0;
        }
        (self.context_window_used as f32 / self.context_window_max as f32) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_timing_delta_computation() {
        // GIVEN un outil plus lent que prévu
        let t = ToolTiming::new("file_read", Some(100), 150);
        // THEN delta = +50%
        assert_eq!(t.expected_ms, Some(100));
        assert_eq!(t.actual_ms, 150);
        assert!((t.delta_pct.unwrap() - 50.0).abs() < 0.01);
    }

    #[test]
    fn tool_timing_delta_absent_when_no_expected() {
        // GIVEN pas de hint
        let t = ToolTiming::new("custom_tool", None, 42);
        // THEN delta_pct est None
        assert!(t.delta_pct.is_none());
    }

    #[test]
    fn tool_timing_delta_ignores_zero_expected() {
        // GIVEN hint avec 0 ms (invalide mais possible)
        let t = ToolTiming::new("tool", Some(0), 50);
        // THEN delta_pct est None pour éviter la division par zéro
        assert!(t.delta_pct.is_none());
    }

    #[test]
    fn thresholds_default_is_80_100() {
        let th = SessionThresholds::default();
        assert_eq!(th.token_warn_pct, 80);
        assert_eq!(th.token_block_pct, 100);
    }

    #[test]
    fn thresholds_evaluate_returns_ok_without_budget() {
        // GIVEN pas de budget configuré
        let th = SessionThresholds::default();
        // THEN évaluation retourne Ok même avec usage élevé
        assert_eq!(th.evaluate(10_000, 0), BudgetAlertLevel::Ok);
    }

    #[test]
    fn thresholds_evaluate_crosses_warning_and_block() {
        let th = SessionThresholds::default();
        // Sous warning : Ok
        assert_eq!(th.evaluate(500, 1000), BudgetAlertLevel::Ok);
        // À 80 % : Warning
        assert_eq!(th.evaluate(800, 1000), BudgetAlertLevel::Warning);
        // À 95 % : Warning
        assert_eq!(th.evaluate(950, 1000), BudgetAlertLevel::Warning);
        // À 100 % : Block
        assert_eq!(th.evaluate(1000, 1000), BudgetAlertLevel::Block);
        // Au-delà : Block
        assert_eq!(th.evaluate(1200, 1000), BudgetAlertLevel::Block);
    }

    #[test]
    fn session_metrics_aggregates_llm_calls() {
        // GIVEN deux appels LLM non-meta successifs
        let mut m = SessionMetrics::default();
        m.record_llm_call(100, 50, 30, false);
        m.record_llm_call(200, 80, 60, false);
        // THEN compteurs principaux cumulés
        assert_eq!(m.tokens_in, 300);
        assert_eq!(m.tokens_out, 130);
        assert_eq!(m.tokens_cached, 90);
        assert_eq!(m.tokens_meta, 0);
        // AND context_window_used = tokens_in + tokens_out
        assert_eq!(m.context_window_used, 430);
    }

    #[test]
    fn session_metrics_routes_meta_tokens_separately() {
        // GIVEN un appel LLM meta (narration)
        let mut m = SessionMetrics::default();
        m.record_llm_call(50, 30, 0, true);
        // THEN compteurs meta incrémentés seulement
        assert_eq!(m.tokens_in, 0);
        assert_eq!(m.tokens_out, 0);
        assert_eq!(m.tokens_meta, 80);
    }

    #[test]
    fn session_metrics_tool_timings_capped() {
        // GIVEN plus de TOOL_TIMINGS_MAX timings poussés
        let mut m = SessionMetrics::default();
        for i in 0..(TOOL_TIMINGS_MAX + 5) {
            m.push_tool_timing(ToolTiming::new(format!("tool_{i}"), Some(10), 12));
        }
        // THEN la liste est bornée et FIFO
        assert_eq!(m.tool_timings.len(), TOOL_TIMINGS_MAX);
        assert_eq!(m.tool_timings[0].tool_name, "tool_5");
        assert_eq!(
            m.tool_timings.last().unwrap().tool_name,
            format!("tool_{}", TOOL_TIMINGS_MAX + 4)
        );
    }

    #[test]
    fn session_metrics_context_window_pct() {
        // GIVEN un contexte à 70 %
        let m = SessionMetrics {
            context_window_used: 7000,
            context_window_max: 10_000,
            ..Default::default()
        };
        // THEN pct ≈ 70.0
        assert!((m.context_window_pct() - 70.0).abs() < 0.01);
    }

    #[test]
    fn session_metrics_context_window_pct_safe_when_max_zero() {
        // GIVEN un contexte sans max configuré
        let m = SessionMetrics::default();
        // THEN pct = 0 (pas de panic sur division par zéro)
        assert_eq!(m.context_window_pct(), 0.0);
    }

    #[test]
    fn session_metrics_tokens_used_for_budget_sums_all_non_cached() {
        // GIVEN un mix in/out/meta/cached
        let m = SessionMetrics {
            tokens_in: 100,
            tokens_out: 50,
            tokens_cached: 200,
            tokens_meta: 30,
            ..Default::default()
        };
        // THEN le budget compte in + out + meta, jamais cached
        assert_eq!(m.tokens_used_for_budget(), 180);
    }

    #[test]
    fn session_metrics_serde_roundtrip() {
        // GIVEN des métriques avec des événements
        let mut m = SessionMetrics {
            tokens_in: 500,
            tokens_out: 200,
            tokens_cached: 100,
            tokens_meta: 50,
            context_window_used: 700,
            context_window_max: 200_000,
            token_budget: 10_000,
            ..Default::default()
        };
        m.push_tool_timing(ToolTiming::new("file_read", Some(50), 75));
        m.push_summarization(SummarizationEvent {
            messages_summarized_count: 10,
            tokens_saved: 2500,
            summary_excerpt: "Résumé de 10 messages…".into(),
        });
        // WHEN serialize / deserialize
        let json = serde_json::to_string(&m).expect("serialize");
        let back: SessionMetrics = serde_json::from_str(&json).expect("deserialize");
        // THEN les champs sont préservés
        assert_eq!(back.tokens_in, 500);
        assert_eq!(back.tool_timings.len(), 1);
        assert_eq!(back.summarization_events.len(), 1);
        assert_eq!(back.summarization_events[0].tokens_saved, 2500);
    }

    #[test]
    fn alert_level_default_is_ok() {
        assert_eq!(BudgetAlertLevel::default(), BudgetAlertLevel::Ok);
    }
}
