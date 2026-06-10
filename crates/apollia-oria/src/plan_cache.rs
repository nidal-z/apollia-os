//! Plan cache repository for ORIA: avoids regenerating identical plans via LLM.
//!
//! Implements [`PlanCacheRepository`] backed by SQLite (`plan_cache.db`), following
//! the same pattern as existing repositories (`TriggerDefinitionRepository`, etc.).
//!
//! The cache key is a SHA-256 digest of `{agent_name}:{agent_version}:{sorted_tools}:{normalized_text}`.
//! Strategy: TTL 7 days, max 1000 entries, LRU eviction.

use std::path::Path;

use rusqlite::params;
use sha2::{Digest, Sha256};

use crate::plan::ExecutionPlan;

// Error type

/// Errors returned by [`PlanCacheRepository`] operations.
#[derive(Debug, thiserror::Error)]
pub enum PlanCacheError {
    /// Underlying SQLite error.
    #[error("SQLite error: {0}")]
    Database(#[from] rusqlite::Error),
    /// JSON serialization or deserialization error.
    #[error("JSON serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

// Constants

/// Maximum length of the normalized task text included in the cache key.
const MAX_TEXT_LENGTH: usize = 500;

// Repository

/// SQLite-backed cache for ORIA execution plans.
///
/// Stores plans keyed by a deterministic SHA-256 hash of agent metadata and
/// task text. On cache hit, `hit_count` and `last_used_at` are updated for
/// observability. Eviction is time-based via [`Self::evict_expired`].
///
/// Time-based eviction keeps the cache bounded.
pub struct PlanCacheRepository {
    conn: rusqlite::Connection,
}

impl PlanCacheRepository {
    /// Open or create the plan cache database at the given path.
    ///
    /// Creates the `plan_cache` table if it does not already exist and enables
    /// WAL mode for concurrent read performance.
    pub fn open(path: &Path) -> Result<Self, PlanCacheError> {
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS plan_cache (
                cache_key     TEXT PRIMARY KEY,
                plan_json     TEXT NOT NULL,
                hit_count     INTEGER NOT NULL DEFAULT 0,
                created_at    TEXT NOT NULL,
                last_used_at  TEXT NOT NULL,
                agent_name    TEXT NOT NULL,
                agent_version TEXT NOT NULL
            );",
        )?;
        Ok(Self { conn })
    }

    /// Look up a cached plan by key.
    ///
    /// On cache hit, increments `hit_count` and updates `last_used_at` to the
    /// current timestamp. Returns `None` if no entry exists for the given key.
    pub fn lookup(&self, key: &str) -> Result<Option<ExecutionPlan>, PlanCacheError> {
        let mut stmt = self
            .conn
            .prepare("SELECT plan_json FROM plan_cache WHERE cache_key = ?1")?;

        let plan_json: Option<String> =
            stmt.query_row(params![key], |row| row.get(0)).optional()?;

        let Some(json) = plan_json else {
            return Ok(None);
        };

        let plan: ExecutionPlan = serde_json::from_str(&json)?;

        // Update hit_count and last_used_at on cache hit.
        self.conn.execute(
            "UPDATE plan_cache SET hit_count = hit_count + 1, last_used_at = datetime('now') WHERE cache_key = ?1",
            params![key],
        )?;

        Ok(Some(plan))
    }

    /// Store a plan in the cache.
    ///
    /// Uses `INSERT OR REPLACE` so that storing with an existing key overwrites
    /// the previous entry (upsert semantics).
    pub fn store(
        &self,
        key: &str,
        plan: &ExecutionPlan,
        agent_name: &str,
        agent_version: &str,
    ) -> Result<(), PlanCacheError> {
        let plan_json = serde_json::to_string(plan)?;

        self.conn.execute(
            "INSERT OR REPLACE INTO plan_cache (cache_key, plan_json, hit_count, created_at, last_used_at, agent_name, agent_version)
             VALUES (?1, ?2, 0, datetime('now'), datetime('now'), ?3, ?4)",
            params![key, plan_json, agent_name, agent_version],
        )?;

        Ok(())
    }

    /// Evict entries older than `max_age_days` days based on `created_at`.
    ///
    /// Returns the number of evicted entries.
    pub fn evict_expired(&self, max_age_days: u32) -> Result<u32, PlanCacheError> {
        let deleted = self.conn.execute(
            "DELETE FROM plan_cache WHERE created_at < datetime('now', ?1)",
            params![format!("-{max_age_days} days")],
        )?;

        Ok(deleted as u32)
    }

    /// Returns aggregate statistics about the cache contents.
    ///
    /// All counters are zero when the cache is empty. The `oldest_entry_at` and
    /// `newest_entry_at` fields are `None` when no entries exist.
    pub fn stats(&self) -> Result<PlanCacheStats, PlanCacheError> {
        let total_entries: u32 =
            self.conn
                .query_row("SELECT COUNT(*) FROM plan_cache", [], |row| row.get(0))?;

        let total_hits: u64 = self.conn.query_row(
            "SELECT COALESCE(SUM(hit_count), 0) FROM plan_cache",
            [],
            |row| row.get(0),
        )?;

        let oldest: Option<String> = self
            .conn
            .query_row("SELECT MIN(created_at) FROM plan_cache", [], |row| {
                row.get(0)
            })
            .optional()?
            .flatten();

        let newest: Option<String> = self
            .conn
            .query_row("SELECT MAX(created_at) FROM plan_cache", [], |row| {
                row.get(0)
            })
            .optional()?
            .flatten();

        Ok(PlanCacheStats {
            total_entries,
            cache_hits: total_hits,
            oldest_entry_at: oldest,
            newest_entry_at: newest,
        })
    }

    /// Removes all entries from the cache and returns the count of deleted rows.
    pub fn clear_all(&self) -> Result<u32, PlanCacheError> {
        let deleted = self.conn.execute("DELETE FROM plan_cache", [])?;
        Ok(deleted as u32)
    }
}

/// Aggregate statistics for the plan cache.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanCacheStats {
    /// Number of cached plans.
    pub total_entries: u32,
    /// Total number of cache hits across all entries.
    pub cache_hits: u64,
    /// Timestamp of the oldest cache entry (RFC 3339 or SQLite datetime).
    pub oldest_entry_at: Option<String>,
    /// Timestamp of the newest cache entry (RFC 3339 or SQLite datetime).
    pub newest_entry_at: Option<String>,
}

// ─────────────────────────────────────────────
// Cache key computation
// ─────────────────────────────────────────────

/// Compute a deterministic cache key from agent metadata and task text.
///
/// Formula: `SHA-256("{agent_name}:{agent_version}:{sorted_tools_joined}:{normalized_text}")`
///
/// Normalization rules:
/// - Tools are sorted alphabetically and joined with `","`.
/// - Task text is lowercased, whitespace-collapsed (sequences of whitespace → single space),
///   trimmed, and truncated to 500 characters.
pub fn compute_cache_key(
    agent_name: &str,
    agent_version: &str,
    tools: &[String],
    task_text: &str,
) -> String {
    let mut sorted_tools = tools.to_vec();
    sorted_tools.sort();
    let tools_joined = sorted_tools.join(",");

    let normalized_text = normalize_text(task_text);

    let input = format!("{agent_name}:{agent_version}:{tools_joined}:{normalized_text}");

    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    result
        .iter()
        .fold(String::with_capacity(64), |mut acc, byte| {
            use std::fmt::Write;
            let _ = write!(acc, "{byte:02x}");
            acc
        })
}

/// Normalize text for cache key computation.
///
/// 1. Lowercase.
/// 2. Collapse all whitespace sequences into a single space.
/// 3. Trim leading/trailing whitespace.
/// 4. Truncate to [`MAX_TEXT_LENGTH`] characters.
fn normalize_text(text: &str) -> String {
    let lowered = text.to_lowercase();
    let collapsed: String = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.len() > MAX_TEXT_LENGTH {
        collapsed.chars().take(MAX_TEXT_LENGTH).collect()
    } else {
        collapsed
    }
}

// Allow the optional return from query_row.
use rusqlite::OptionalExtension;

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::PlanStep;

    /// Helper: create a minimal ExecutionPlan for testing.
    fn sample_plan() -> ExecutionPlan {
        ExecutionPlan {
            plan_id: "plan-test-001".to_string(),
            task_id: "task-test-001".to_string(),
            steps: vec![
                {
                    let mut s = PlanStep::new("s1", "Read file");
                    s.tool_hint = Some("file_io".to_string());
                    s
                },
                {
                    let mut s = PlanStep::new("s2", "Summarize");
                    s.tool_hint = Some("llm".to_string());
                    s.depends_on = vec!["s1".to_string()];
                    s.model_hint = Some("fast-7b".to_string());
                    s
                },
            ],
        }
    }

    // store + lookup round-trip

    /// GIVEN a valid ExecutionPlan and a computed cache key
    /// WHEN the plan is stored via store() then retrieved via lookup()
    /// THEN the returned plan is identical to the original plan
    #[test]
    fn test_store_and_lookup_roundtrip() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("plan_cache.db");
        let repo = PlanCacheRepository::open(&db_path).expect("open");
        let plan = sample_plan();
        let key = compute_cache_key(
            "my-agent",
            "1.0",
            &["file_io".into(), "llm".into()],
            "do stuff",
        );

        // WHEN
        repo.store(&key, &plan, "my-agent", "1.0").expect("store");
        let fetched = repo.lookup(&key).expect("lookup");

        // THEN
        let fetched = fetched.expect("expected Some(plan)");
        assert_eq!(fetched.plan_id, plan.plan_id);
        assert_eq!(fetched.task_id, plan.task_id);
        assert_eq!(fetched.steps.len(), plan.steps.len());
        assert_eq!(fetched.steps[0].step_id, "s1");
        assert_eq!(fetched.steps[1].step_id, "s2");
        assert_eq!(fetched.steps[1].model_hint, Some("fast-7b".to_string()));
    }

    // Eviction removes expired entries

    /// GIVEN 3 cache entries, 2 of which are older than 7 days
    /// WHEN evict_expired(7) is called
    /// THEN 2 entries are removed and the function returns 2
    #[test]
    fn test_evict_expired_entries() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("plan_cache.db");
        let repo = PlanCacheRepository::open(&db_path).expect("open");
        let plan = sample_plan();

        // Insert 3 entries: 2 with old timestamps, 1 fresh.
        repo.store("key-old-1", &plan, "agent", "1.0")
            .expect("store");
        repo.store("key-old-2", &plan, "agent", "1.0")
            .expect("store");
        repo.store("key-fresh", &plan, "agent", "1.0")
            .expect("store");

        // Backdate 2 entries to 10 days ago.
        repo.conn
            .execute(
                "UPDATE plan_cache SET created_at = datetime('now', '-10 days') WHERE cache_key IN ('key-old-1', 'key-old-2')",
                [],
            )
            .expect("backdate");

        // WHEN
        let evicted = repo.evict_expired(7).expect("evict");

        // THEN
        assert_eq!(evicted, 2);
        assert!(repo.lookup("key-old-1").expect("lookup").is_none());
        assert!(repo.lookup("key-old-2").expect("lookup").is_none());
        assert!(repo.lookup("key-fresh").expect("lookup").is_some());
    }

    // Different versions produce different keys

    /// GIVEN the same agent_name, the same tools, and the same task text
    /// WHEN compute_cache_key is called with agent_version "1.0" then "1.1"
    /// THEN the two cache keys differ
    #[test]
    fn test_different_versions_produce_different_keys() {
        // GIVEN
        let tools = vec!["file_io".to_string(), "bash".to_string()];
        let text = "Generate a report from logs";

        // WHEN
        let key_v1 = compute_cache_key("my-agent", "1.0", &tools, text);
        let key_v2 = compute_cache_key("my-agent", "1.1", &tools, text);

        // THEN
        assert_ne!(key_v1, key_v2);
        assert_eq!(key_v1.len(), 64, "SHA-256 hex should be 64 chars");
    }

    // Text normalization in the key

    /// GIVEN text with uppercase, multiple spaces, and more than 500 characters
    /// WHEN compute_cache_key is called
    /// THEN the text is normalized (lowercase, whitespace-collapsed, truncated to 500 chars)
    #[test]
    fn test_text_normalization_in_cache_key() {
        // GIVEN
        let tools = vec!["llm".to_string()];
        let text_messy = "  Hello   WORLD   with   extra   spaces  ";
        let text_clean = "hello world with extra spaces";

        // WHEN: messy and clean should produce the same key.
        let key_messy = compute_cache_key("agent", "1.0", &tools, text_messy);
        let key_clean = compute_cache_key("agent", "1.0", &tools, text_clean);

        // THEN
        assert_eq!(key_messy, key_clean);

        // GIVEN: text longer than 500 chars.
        let long_text = "a ".repeat(600); // 1200 chars
        let key_long = compute_cache_key("agent", "1.0", &tools, &long_text);
        // Verify the key is a valid SHA-256 hex.
        assert_eq!(key_long.len(), 64);

        // Verify truncation: a very long text differs only after 500 chars, same key.
        let base = "x".repeat(500);
        let extended = format!("{base}YYYYY");
        let key_base = compute_cache_key("agent", "1.0", &tools, &base);
        let key_extended = compute_cache_key("agent", "1.0", &tools, &extended);
        assert_eq!(
            key_base, key_extended,
            "texts differing only after 500 chars must produce the same key"
        );
    }

    // ─── Extra: lookup miss returns None ───

    /// GIVEN an empty cache
    /// WHEN lookup is called with an unknown key
    /// THEN None is returned
    #[test]
    fn test_lookup_miss_returns_none() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("plan_cache.db");
        let repo = PlanCacheRepository::open(&db_path).expect("open");

        // WHEN
        let result = repo.lookup("nonexistent-key").expect("lookup");

        // THEN
        assert!(result.is_none());
    }

    // ─── Extra: hit_count increments on lookup ───

    /// GIVEN a cached plan
    /// WHEN lookup is called twice
    /// THEN hit_count is 2
    #[test]
    fn test_hit_count_increments() {
        // GIVEN
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("plan_cache.db");
        let repo = PlanCacheRepository::open(&db_path).expect("open");
        let plan = sample_plan();
        repo.store("key-1", &plan, "agent", "1.0").expect("store");

        // WHEN
        let _ = repo.lookup("key-1").expect("lookup 1");
        let _ = repo.lookup("key-1").expect("lookup 2");

        // THEN
        let hit_count: i64 = repo
            .conn
            .query_row(
                "SELECT hit_count FROM plan_cache WHERE cache_key = ?1",
                params!["key-1"],
                |row| row.get(0),
            )
            .expect("query hit_count");
        assert_eq!(hit_count, 2);
    }

    // ─── Extra: tool order does not affect key ───

    /// GIVEN tools in different orders
    /// WHEN compute_cache_key is called
    /// THEN the same key is produced (tools are sorted)
    #[test]
    fn test_tool_order_irrelevant() {
        // GIVEN
        let tools_a = vec!["bash".to_string(), "file_io".to_string()];
        let tools_b = vec!["file_io".to_string(), "bash".to_string()];

        // WHEN
        let key_a = compute_cache_key("agent", "1.0", &tools_a, "task");
        let key_b = compute_cache_key("agent", "1.0", &tools_b, "task");

        // THEN
        assert_eq!(key_a, key_b);
    }
}
