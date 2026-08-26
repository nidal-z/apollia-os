//! Per-skill A2A telemetry.
//!
//! In-memory aggregation (rolling window of 100 invocations per skill) of A2A
//! invocation metrics: average latency, success rate, tokens consumed. Also
//! records step provenance to enable global timeline drill-down.
//!
//! Always on: the cost is minimal (bounded ring of 100 elements per skill).
//! Optional persistence runs every 5 minutes via [`TelemetryStore::flush`].

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Maximum number of invocations kept in the rolling window per skill.
pub const ROLLING_WINDOW_SIZE: usize = 100;

/// Aggregation key: `(agent_name, skill_id)` identifies a unique skill
/// within the context of a given Worker Agent.
pub type TelemetryKey = (String, String);

/// Record of a single invocation in the rolling window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationRecord {
    /// Invocation duration in milliseconds.
    pub duration_ms: u64,
    /// `true` if the invocation completed successfully.
    pub success: bool,
    /// Tokens consumed by this invocation (0 if unknown).
    pub tokens: u64,
    /// Unix timestamp (milliseconds) when the invocation ended.
    pub timestamp_ms: u64,
}

/// Aggregated view of a skill's telemetry, computed on demand from
/// the rolling window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2ASkillTelemetry {
    /// Skill name (e.g. `"read-excel"`).
    pub skill_name: String,
    /// Version advertised by the Worker Agent that provides the skill.
    pub version: String,
    /// Total number of invocations in the current rolling window.
    pub invocations: u64,
    /// Average latency in milliseconds over the window.
    pub avg_latency_ms: u64,
    /// Success rate over the window, between 0.0 and 1.0.
    pub success_rate: f64,
    /// Sum of tokens consumed over the window.
    pub tokens_consumed: u64,
}

/// Provenance of a step in an A2A chain. The step id is shared with the
/// global timeline to enable drill-down.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AStepProvenance {
    /// Unique step identifier (shared with the conversation timeline).
    pub step_id: String,
    /// Input excerpt, truncated to 240 characters.
    pub input_excerpt: String,
    /// Output excerpt, truncated to 240 characters. `None` if the invocation
    /// is still running or failed before producing output.
    pub output_excerpt: Option<String>,
    /// Name of the initiating agent (caller / Director).
    pub agent_from: String,
    /// Name of the target agent (Worker).
    pub agent_to: String,
    /// Identifier of the parent step in the A2A chain, `None` for the root step.
    pub parent_step: Option<String>,
    /// Identifier of the invoked skill.
    pub skill_id: String,
    /// Unix timestamp (milliseconds) when the step was created.
    pub timestamp_ms: u64,
}

/// Maximum length of an input/output excerpt.
pub const EXCERPT_MAX_CHARS: usize = 240;

/// Truncates a string to [`EXCERPT_MAX_CHARS`] characters, with an ellipsis.
pub fn make_excerpt(s: &str) -> String {
    if s.chars().count() <= EXCERPT_MAX_CHARS {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(EXCERPT_MAX_CHARS).collect();
        format!("{truncated}…")
    }
}

/// Storage for telemetry rolling windows and step provenance.
///
/// Shared between actors via `Arc`. Writes go through short read/write locks,
/// with no significant contention at the expected volume.
#[derive(Debug, Default)]
pub struct TelemetryStore {
    windows: RwLock<HashMap<TelemetryKey, VecDeque<InvocationRecord>>>,
    versions: RwLock<HashMap<TelemetryKey, String>>,
    steps: RwLock<Vec<A2AStepProvenance>>,
}

/// Clonable handle to a [`TelemetryStore`].
pub type TelemetryHandle = Arc<TelemetryStore>;

impl TelemetryStore {
    /// Builds an empty store.
    pub fn new() -> TelemetryHandle {
        Arc::new(Self::default())
    }

    /// Records a completed invocation.
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

    /// Records a provenance step (keeps the last 10,000 to bound memory).
    pub async fn record_step(&self, step: A2AStepProvenance) {
        const MAX_STEPS: usize = 10_000;
        let mut steps = self.steps.write().await;
        if steps.len() >= MAX_STEPS {
            let drop_count = steps.len() - MAX_STEPS + 1;
            steps.drain(0..drop_count);
        }
        steps.push(step);
    }

    /// Computes the aggregated view for `(agent_name, skill_id)`.
    ///
    /// Returns `None` if no invocation has been recorded for this key.
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

    /// Returns aggregated telemetry for all recorded skills.
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

    /// Returns provenance steps filtered by `skill_id` (or all if `None`).
    pub async fn steps_for(&self, skill_id: Option<&str>) -> Vec<A2AStepProvenance> {
        let steps = self.steps.read().await;
        match skill_id {
            None => steps.clone(),
            Some(id) => steps.iter().filter(|s| s.skill_id == id).cloned().collect(),
        }
    }

    /// Extension point for persistence (called every 5 minutes).
    ///
    /// Current implementation: no-op. SQLite persistence can be wired in here
    /// without changing the public API.
    pub async fn flush(&self) {
        // no-op: the store is in-memory for this release.
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
        // GIVEN an empty store
        let store = TelemetryStore::new();

        // WHEN we record 3 invocations
        store
            .record_invocation("worker-a", "read-excel", "1.0.0", rec(100, true, 50))
            .await;
        store
            .record_invocation("worker-a", "read-excel", "1.0.0", rec(200, true, 70))
            .await;
        store
            .record_invocation("worker-a", "read-excel", "1.0.0", rec(300, false, 10))
            .await;

        // THEN the telemetry aggregates correctly
        let tel = store.telemetry_for("worker-a", "read-excel").await.unwrap();
        assert_eq!(tel.invocations, 3);
        assert_eq!(tel.avg_latency_ms, 200);
        assert!((tel.success_rate - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(tel.tokens_consumed, 130);
        assert_eq!(tel.version, "1.0.0");
    }

    #[tokio::test]
    async fn test_rolling_window_caps_at_100() {
        // GIVEN a store and 150 invocations
        let store = TelemetryStore::new();
        for _ in 0..150 {
            store
                .record_invocation("w", "s", "1.0.0", rec(10, true, 0))
                .await;
        }

        // WHEN we read the telemetry
        let tel = store.telemetry_for("w", "s").await.unwrap();

        // THEN the window is capped at 100
        assert_eq!(tel.invocations, ROLLING_WINDOW_SIZE as u64);
    }

    #[tokio::test]
    async fn test_missing_key_returns_none() {
        // GIVEN an empty store
        let store = TelemetryStore::new();

        // WHEN we query an unknown key
        let tel = store.telemetry_for("nope", "missing").await;

        // THEN we get None
        assert!(tel.is_none());
    }

    #[tokio::test]
    async fn test_all_telemetry_sorted_by_skill_name() {
        // GIVEN a store with two skills
        let store = TelemetryStore::new();
        store
            .record_invocation("w", "zeta", "1.0.0", rec(10, true, 0))
            .await;
        store
            .record_invocation("w", "alpha", "1.0.0", rec(20, true, 0))
            .await;

        // WHEN we list all of them
        let all = store.all_telemetry().await;

        // THEN the alphabetical sort is respected
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].skill_name, "alpha");
        assert_eq!(all[1].skill_name, "zeta");
    }

    #[tokio::test]
    async fn test_step_provenance_filter() {
        // GIVEN a store with two steps from different skills
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

        // WHEN we filter by skill
        let reads = store.steps_for(Some("read")).await;
        let all = store.steps_for(None).await;

        // THEN the filtering is correct
        assert_eq!(reads.len(), 1);
        assert_eq!(reads[0].step_id, "s1");
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_make_excerpt_short() {
        // GIVEN a payload shorter than the excerpt budget
        // WHEN the excerpt is made
        // THEN it is the payload itself, unchanged
        assert_eq!(make_excerpt("hello"), "hello");
    }

    #[test]
    fn test_make_excerpt_truncates_with_ellipsis() {
        // GIVEN a payload well past the excerpt budget
        let s = "x".repeat(300);
        // WHEN the excerpt is made
        let out = make_excerpt(&s);
        // THEN it is cut at the budget and ends with an ellipsis
        assert_eq!(out.chars().count(), EXCERPT_MAX_CHARS + 1);
        assert!(out.ends_with('…'));
    }
}
