//! Telemetry A2A par skill — US-SP42-044 (Pattern P8).
//!
//! Agrégation in-memory (rolling window de 100 invocations par skill) des métriques
//! d'invocations A2A : latence moyenne, taux de succès, tokens consommés. Trace
//! également la provenance des steps pour permettre le drill-down TimelineGlobal.
//!
//! Toujours-on : le coût est minimal (anneau borné de 100 éléments par skill).
//! La persistance optionnelle se fait toutes les 5 minutes via [`TelemetryStore::flush`].

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Nombre maximum d'invocations conservées dans la fenêtre glissante par skill.
pub const ROLLING_WINDOW_SIZE: usize = 100;

/// Clé d'agrégation : `(agent_name, skill_id)` identifie un skill unique
/// dans le contexte d'un Worker Agent donné.
pub type TelemetryKey = (String, String);

/// Enregistrement d'une invocation individuelle dans la fenêtre glissante.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationRecord {
    /// Durée de l'invocation en millisecondes.
    pub duration_ms: u64,
    /// `true` si l'invocation s'est terminée avec succès.
    pub success: bool,
    /// Tokens consommés par cette invocation (0 si inconnu).
    pub tokens: u64,
    /// Horodatage Unix (millisecondes) de fin d'invocation.
    pub timestamp_ms: u64,
}

/// Vue agrégée de la télémétrie d'un skill, calculée à la demande depuis
/// la fenêtre glissante.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2ASkillTelemetry {
    /// Nom du skill (ex: `"read-excel"`).
    pub skill_name: String,
    /// Version advertised du Worker Agent qui fournit le skill.
    pub version: String,
    /// Nombre total d'invocations dans la fenêtre glissante courante.
    pub invocations: u64,
    /// Latence moyenne en millisecondes sur la fenêtre.
    pub avg_latency_ms: u64,
    /// Taux de succès sur la fenêtre, entre 0.0 et 1.0.
    pub success_rate: f64,
    /// Somme des tokens consommés sur la fenêtre.
    pub tokens_consumed: u64,
}

/// Provenance d'un step dans une chaîne A2A — clé partagée avec TimelineGlobal
/// pour le drill-down (voir US-SP42-048).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AStepProvenance {
    /// Identifiant unique du step (clé partagée avec la timeline conversation).
    pub step_id: String,
    /// Extrait de l'entrée, tronqué à 240 caractères.
    pub input_excerpt: String,
    /// Extrait de la sortie, tronqué à 240 caractères. `None` si invocation
    /// toujours en cours ou échec avant production d'output.
    pub output_excerpt: Option<String>,
    /// Nom de l'agent initiateur (caller / Director).
    pub agent_from: String,
    /// Nom de l'agent ciblé (Worker).
    pub agent_to: String,
    /// Identifiant du step parent dans la chaîne A2A, `None` pour le step racine.
    pub parent_step: Option<String>,
    /// Identifiant du skill invoqué.
    pub skill_id: String,
    /// Horodatage Unix (millisecondes) de création du step.
    pub timestamp_ms: u64,
}

/// Longueur maximale d'un extrait input/output.
pub const EXCERPT_MAX_CHARS: usize = 240;

/// Tronque une chaîne à [`EXCERPT_MAX_CHARS`] caractères, avec ellipse.
pub fn make_excerpt(s: &str) -> String {
    if s.chars().count() <= EXCERPT_MAX_CHARS {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(EXCERPT_MAX_CHARS).collect();
        format!("{truncated}…")
    }
}

/// Stockage des fenêtres glissantes de télémétrie et des provenances de steps.
///
/// Partagé entre acteurs via `Arc`. Les écritures passent par des mutex courts
/// en lecture/écriture — pas de contention significative au volume attendu.
#[derive(Debug, Default)]
pub struct TelemetryStore {
    windows: RwLock<HashMap<TelemetryKey, VecDeque<InvocationRecord>>>,
    versions: RwLock<HashMap<TelemetryKey, String>>,
    steps: RwLock<Vec<A2AStepProvenance>>,
}

/// Handle clonable vers un [`TelemetryStore`].
pub type TelemetryHandle = Arc<TelemetryStore>;

impl TelemetryStore {
    /// Construit un store vide.
    pub fn new() -> TelemetryHandle {
        Arc::new(Self::default())
    }

    /// Enregistre une invocation terminée.
    pub async fn record_invocation(
        &self,
        agent_name: &str,
        skill_id: &str,
        version: &str,
        record: InvocationRecord,
    ) {
        let key = (agent_name.to_string(), skill_id.to_string());

        let mut windows = self.windows.write().await;
        let window = windows.entry(key.clone()).or_default();
        if window.len() >= ROLLING_WINDOW_SIZE {
            window.pop_front();
        }
        window.push_back(record);
        drop(windows);

        let mut versions = self.versions.write().await;
        versions.insert(key, version.to_string());
    }

    /// Enregistre un step de provenance (garde les 10 000 derniers pour borne mémoire).
    pub async fn record_step(&self, step: A2AStepProvenance) {
        const MAX_STEPS: usize = 10_000;
        let mut steps = self.steps.write().await;
        if steps.len() >= MAX_STEPS {
            let drop_count = steps.len() - MAX_STEPS + 1;
            steps.drain(0..drop_count);
        }
        steps.push(step);
    }

    /// Calcule la vue agrégée pour `(agent_name, skill_id)`.
    ///
    /// Retourne `None` si aucune invocation n'a été enregistrée pour cette clé.
    pub async fn telemetry_for(
        &self,
        agent_name: &str,
        skill_id: &str,
    ) -> Option<A2ASkillTelemetry> {
        let key = (agent_name.to_string(), skill_id.to_string());
        let windows = self.windows.read().await;
        let window = windows.get(&key)?;
        if window.is_empty() {
            return None;
        }
        let invocations = window.len() as u64;
        let total_latency: u64 = window.iter().map(|r| r.duration_ms).sum();
        let successes: u64 = window.iter().filter(|r| r.success).count() as u64;
        let tokens: u64 = window.iter().map(|r| r.tokens).sum();
        let avg_latency_ms = total_latency / invocations;
        let success_rate = successes as f64 / invocations as f64;

        let versions = self.versions.read().await;
        let version = versions.get(&key).cloned().unwrap_or_default();

        Some(A2ASkillTelemetry {
            skill_name: skill_id.to_string(),
            version,
            invocations,
            avg_latency_ms,
            success_rate,
            tokens_consumed: tokens,
        })
    }

    /// Retourne la télémétrie agrégée pour tous les skills enregistrés.
    pub async fn all_telemetry(&self) -> Vec<A2ASkillTelemetry> {
        let windows = self.windows.read().await;
        let versions = self.versions.read().await;
        let mut out = Vec::with_capacity(windows.len());
        for (key, window) in windows.iter() {
            if window.is_empty() {
                continue;
            }
            let invocations = window.len() as u64;
            let total_latency: u64 = window.iter().map(|r| r.duration_ms).sum();
            let successes: u64 = window.iter().filter(|r| r.success).count() as u64;
            let tokens: u64 = window.iter().map(|r| r.tokens).sum();
            let version = versions.get(key).cloned().unwrap_or_default();
            out.push(A2ASkillTelemetry {
                skill_name: key.1.clone(),
                version,
                invocations,
                avg_latency_ms: total_latency / invocations,
                success_rate: successes as f64 / invocations as f64,
                tokens_consumed: tokens,
            });
        }
        out.sort_by(|a, b| a.skill_name.cmp(&b.skill_name));
        out
    }

    /// Retourne les steps de provenance filtrés par `skill_id` (ou tous si `None`).
    pub async fn steps_for(&self, skill_id: Option<&str>) -> Vec<A2AStepProvenance> {
        let steps = self.steps.read().await;
        match skill_id {
            None => steps.clone(),
            Some(id) => steps.iter().filter(|s| s.skill_id == id).cloned().collect(),
        }
    }

    /// Point d'extension pour la persistance (appelé toutes les 5 min).
    ///
    /// Implémentation courante : no-op. Une persistance SQLite pourra être
    /// branchée ici sans modifier l'API publique.
    pub async fn flush(&self) {
        // no-op — le store est in-memory pour cette release.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    fn rec(duration_ms: u64, success: bool, tokens: u64) -> InvocationRecord {
        InvocationRecord {
            duration_ms,
            success,
            tokens,
            timestamp_ms: now_ms(),
        }
    }

    #[tokio::test]
    async fn test_record_invocation_aggregates() {
        // GIVEN un store vide
        let store = TelemetryStore::new();

        // WHEN on enregistre 3 invocations
        store
            .record_invocation("worker-a", "read-excel", "1.0.0", rec(100, true, 50))
            .await;
        store
            .record_invocation("worker-a", "read-excel", "1.0.0", rec(200, true, 70))
            .await;
        store
            .record_invocation("worker-a", "read-excel", "1.0.0", rec(300, false, 10))
            .await;

        // THEN la télémétrie agrège correctement
        let tel = store.telemetry_for("worker-a", "read-excel").await.unwrap();
        assert_eq!(tel.invocations, 3);
        assert_eq!(tel.avg_latency_ms, 200);
        assert!((tel.success_rate - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(tel.tokens_consumed, 130);
        assert_eq!(tel.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_rolling_window_caps_at_100() {
        // GIVEN un store et 150 invocations
        let store = TelemetryStore::new();
        for _ in 0..150 {
            store
                .record_invocation("w", "s", "1.0.0", rec(10, true, 0))
                .await;
        }

        // WHEN on lit la télémétrie
        let tel = store.telemetry_for("w", "s").await.unwrap();

        // THEN la fenêtre est capée à 100
        assert_eq!(tel.invocations, ROLLING_WINDOW_SIZE as u64);
    }

    #[tokio::test]
    async fn test_missing_key_returns_none() {
        // GIVEN un store vide
        let store = TelemetryStore::new();

        // WHEN on interroge une clé inconnue
        let tel = store.telemetry_for("nope", "missing").await;

        // THEN on obtient None
        assert!(tel.is_none());
    }

    #[tokio::test]
    async fn test_all_telemetry_sorted_by_skill_name() {
        // GIVEN un store avec deux skills
        let store = TelemetryStore::new();
        store
            .record_invocation("w", "zeta", "1.0.0", rec(10, true, 0))
            .await;
        store
            .record_invocation("w", "alpha", "1.0.0", rec(20, true, 0))
            .await;

        // WHEN on liste tout
        let all = store.all_telemetry().await;

        // THEN le tri alphabétique est respecté
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].skill_name, "alpha");
        assert_eq!(all[1].skill_name, "zeta");
    }

    #[tokio::test]
    async fn test_step_provenance_filter() {
        // GIVEN un store avec deux steps de skills différents
        let store = TelemetryStore::new();
        store
            .record_step(A2AStepProvenance {
                step_id: "s1".into(),
                input_excerpt: "in".into(),
                output_excerpt: Some("out".into()),
                agent_from: "a".into(),
                agent_to: "b".into(),
                parent_step: None,
                skill_id: "read".into(),
                timestamp_ms: now_ms(),
            })
            .await;
        store
            .record_step(A2AStepProvenance {
                step_id: "s2".into(),
                input_excerpt: "in".into(),
                output_excerpt: None,
                agent_from: "a".into(),
                agent_to: "c".into(),
                parent_step: Some("s1".into()),
                skill_id: "write".into(),
                timestamp_ms: now_ms(),
            })
            .await;

        // WHEN on filtre par skill
        let reads = store.steps_for(Some("read")).await;
        let all = store.steps_for(None).await;

        // THEN le filtrage est correct
        assert_eq!(reads.len(), 1);
        assert_eq!(reads[0].step_id, "s1");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_make_excerpt_short() {
        assert_eq!(make_excerpt("hello"), "hello");
    }

    #[test]
    fn test_make_excerpt_truncates_with_ellipsis() {
        let s = "x".repeat(300);
        let out = make_excerpt(&s);
        assert_eq!(out.chars().count(), EXCERPT_MAX_CHARS + 1);
        assert!(out.ends_with('…'));
    }
}
