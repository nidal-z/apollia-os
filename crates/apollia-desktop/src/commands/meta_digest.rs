//! Tauri IPC command: daily digest narration for the operator Dashboard.
//!
//! Generates a 2-4 sentence narration summarising the user's last N hours of
//! activity ("Aujourd'hui chez Apollia"). Facts are computed frontend-side
//! from the existing stores (tasks, approvals, failures) and passed in; the
//! backend's job is narration + a 6h SQLite TTL cache keyed by
//! `(user_id, user_date)`.
//!
//! Today the narration is a deterministic template. When the Meta-LLM
//! `GenerateDailyDigest` routine lands the same
//! `DailyDigest { narration, source_facts }` shape will be produced by a
//! Meta-LLM prompt with an output cap of 200 tokens, so the frontend will not
//! need to change. `from_llm` exposes which path produced the current digest.

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Concrete facts derived from the user's last `window_hours`. Everything is
/// precomputed by the frontend from stores already live on the Dashboard;
/// the backend never invents numbers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DigestFacts {
    /// Tasks completed successfully in the window.
    #[serde(default)]
    pub tasks_completed: u32,
    /// Tasks that failed in the window.
    #[serde(default)]
    pub tasks_failed: u32,
    /// Approvals currently pending user action.
    #[serde(default)]
    pub approvals_pending: u32,
    /// Memory insights created in the window.
    #[serde(default)]
    pub insights_created: u32,
    /// Failed automations / triggers in the window.
    #[serde(default)]
    pub automations_failed: u32,
    /// Optional, named failure (first one), surfaced in the narration.
    #[serde(default)]
    pub first_failure_label: Option<String>,
    /// Optional, named top-performing assistant surfaced in the narration.
    #[serde(default)]
    pub top_assistant: Option<String>,
}

/// Request body for [`meta_generate_daily_digest`].
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateDigestRequest {
    /// User-local date in `YYYY-MM-DD`. The frontend computes this from the
    /// browser tz, keeping the backend free of tz dependencies.
    pub user_date: String,
    /// Rolling window used to aggregate the facts (hours).
    #[serde(default = "default_window")]
    pub window_hours: u32,
    /// Precomputed facts for the window.
    pub facts: DigestFacts,
    /// Bypass the 6h cache and regenerate immediately.
    #[serde(default)]
    pub force_refresh: bool,
}

fn default_window() -> u32 {
    24
}

/// Digest returned to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyDigest {
    /// 2-4 sentences rendered in the `DigestNarration` block.
    pub narration: String,
    /// Facts used to generate the narration, displayed as a "Why this?"
    /// disclosure under the hero.
    pub source_facts: DigestFacts,
    /// RFC-3339 timestamp of the generation that produced this narration.
    pub generated_at: String,
    /// `true` when the digest was served from cache (no regeneration).
    pub cached: bool,
    /// `true` when the narration came from the Meta-LLM path.
    /// Currently always `false`; placeholder for the upcoming LLM wiring.
    pub from_llm: bool,
}

/// SQLite cache (6h TTL). Keyed by `(user_date, window_hours)`; the next
/// iteration will also include the authenticated user id once user_memory
/// gets multi-profile support.
static DIGEST_DB: Mutex<Option<Connection>> = Mutex::new(None);

fn db_path() -> Result<PathBuf, String> {
    let home = std::env::var("HOME")
        .map_err(|_| "cannot determine home directory: $HOME not set".to_string())?;
    Ok(PathBuf::from(home).join(".apollia").join("daily_digest.db"))
}

fn with_conn<R>(f: impl FnOnce(&Connection) -> Result<R, String>) -> Result<R, String> {
    let mut guard = DIGEST_DB
        .lock()
        .map_err(|e| format!("digest DB mutex poisoned: {e}"))?;

    if guard.is_none() {
        let path = db_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create digest dir: {e}"))?;
        }
        let conn =
            Connection::open(&path).map_err(|e| format!("failed to open daily_digest.db: {e}"))?;
        conn.execute_batch("PRAGMA journal_mode = WAL;")
            .map_err(|e| format!("WAL pragma failed: {e}"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS daily_digest_cache (
                user_date     TEXT NOT NULL,
                window_hours  INTEGER NOT NULL,
                narration     TEXT NOT NULL,
                source_facts  TEXT NOT NULL,
                from_llm      INTEGER NOT NULL,
                generated_at  TEXT NOT NULL,
                PRIMARY KEY (user_date, window_hours)
            );",
        )
        .map_err(|e| format!("daily_digest schema failed: {e}"))?;
        *guard = Some(conn);
    }

    f(guard.as_ref().expect("just initialized"))
}

/// Cache TTL: 6h. Matches the "refresh summary" budget: at most one LLM call
/// per 6h window unless the user explicitly refreshes.
const CACHE_TTL_SECONDS: i64 = 6 * 3600;

fn read_cache(user_date: &str, window_hours: u32) -> Result<Option<DailyDigest>, String> {
    with_conn(|conn| {
        let row = conn.query_row(
            "SELECT narration, source_facts, from_llm, generated_at
               FROM daily_digest_cache WHERE user_date = ?1 AND window_hours = ?2",
            params![user_date, window_hours],
            |row| {
                let narration: String = row.get(0)?;
                let facts_json: String = row.get(1)?;
                let from_llm: i64 = row.get(2)?;
                let generated_at: String = row.get(3)?;
                Ok((narration, facts_json, from_llm != 0, generated_at))
            },
        );

        match row {
            Ok((narration, facts_json, from_llm, generated_at)) => {
                let parsed = chrono::DateTime::parse_from_rfc3339(&generated_at)
                    .map_err(|e| format!("stored generated_at invalid: {e}"))?;
                let age =
                    chrono::Utc::now().signed_duration_since(parsed.with_timezone(&chrono::Utc));
                if age.num_seconds() > CACHE_TTL_SECONDS {
                    return Ok(None);
                }
                let source_facts: DigestFacts = serde_json::from_str(&facts_json)
                    .map_err(|e| format!("stored source_facts invalid: {e}"))?;
                Ok(Some(DailyDigest {
                    narration,
                    source_facts,
                    generated_at,
                    cached: true,
                    from_llm,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("digest cache read failed: {e}")),
        }
    })
}

fn write_cache(user_date: &str, window_hours: u32, digest: &DailyDigest) -> Result<(), String> {
    let facts_json = serde_json::to_string(&digest.source_facts)
        .map_err(|e| format!("source_facts serialize failed: {e}"))?;
    with_conn(|conn| {
        conn.execute(
            "INSERT OR REPLACE INTO daily_digest_cache
                (user_date, window_hours, narration, source_facts, from_llm, generated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                user_date,
                window_hours,
                digest.narration,
                facts_json,
                digest.from_llm as i64,
                digest.generated_at,
            ],
        )
        .map_err(|e| format!("digest cache write failed: {e}"))?;
        Ok(())
    })
}

/// Deterministic narration used until the Meta-LLM path is wired. Always in
/// French (operator persona is fr-first); the LLM prompt will produce the
/// localised equivalents. The sentences are composed strictly from the
/// provided facts: if there is nothing to say, we say so.
fn render_narration(facts: &DigestFacts) -> String {
    let mut sentences: Vec<String> = Vec::new();

    if facts.tasks_completed > 0 {
        if let Some(assistant) = &facts.top_assistant {
            sentences.push(format!(
                "Vos assistants ont terminé {n} tâches, {a} en tête.",
                n = facts.tasks_completed,
                a = assistant
            ));
        } else {
            sentences.push(format!(
                "Vos assistants ont terminé {n} tâches.",
                n = facts.tasks_completed
            ));
        }
    }

    if facts.approvals_pending > 0 {
        sentences.push(format!(
            "Il y a {n} approbation{s} en attente depuis ce matin.",
            n = facts.approvals_pending,
            s = if facts.approvals_pending > 1 { "s" } else { "" }
        ));
    }

    if facts.automations_failed > 0 {
        if let Some(label) = &facts.first_failure_label {
            sentences.push(format!("Une automatisation a échoué : « {label} »."));
        } else {
            sentences.push(format!(
                "{n} automatisation{s} ont échoué — à vérifier.",
                n = facts.automations_failed,
                s = if facts.automations_failed > 1 {
                    "s"
                } else {
                    ""
                }
            ));
        }
    }

    if facts.tasks_failed > 0 && facts.automations_failed == 0 {
        sentences.push(format!(
            "{n} tâche{s} ont échoué sur la période.",
            n = facts.tasks_failed,
            s = if facts.tasks_failed > 1 { "s" } else { "" }
        ));
    }

    if facts.insights_created > 0 {
        sentences.push(format!(
            "{n} nouveau{x} insight{s} ont été ajoutés à votre mémoire.",
            n = facts.insights_created,
            x = if facts.insights_created > 1 { "x" } else { "" },
            s = if facts.insights_created > 1 { "s" } else { "" }
        ));
    }

    if sentences.is_empty() {
        return "Rien à signaler aujourd'hui. Lancez une tâche quand vous êtes prêt !".into();
    }

    sentences.join(" ")
}

/// Generate (or return a cached) daily digest for the operator Dashboard.
///
/// Never fails on missing facts; a zero-filled `DigestFacts` yields the
/// "Rien à signaler aujourd'hui" narration. Cache misses always produce a
/// fresh entry; `force_refresh = true` bypasses the 6h TTL.
#[tauri::command]
pub async fn meta_generate_daily_digest(
    request: GenerateDigestRequest,
) -> Result<DailyDigest, String> {
    if !request.force_refresh {
        if let Some(cached) = read_cache(&request.user_date, request.window_hours)? {
            return Ok(cached);
        }
    }

    let narration = render_narration(&request.facts);
    let digest = DailyDigest {
        narration,
        source_facts: request.facts,
        generated_at: chrono::Utc::now().to_rfc3339(),
        cached: false,
        from_llm: false,
    };
    write_cache(&request.user_date, request.window_hours, &digest)?;
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_facts_yields_idle_narration() {
        let n = render_narration(&DigestFacts::default());
        assert!(n.contains("Rien à signaler"));
    }

    #[test]
    fn narration_mentions_completed_and_pending() {
        let facts = DigestFacts {
            tasks_completed: 3,
            approvals_pending: 2,
            ..Default::default()
        };
        let n = render_narration(&facts);
        assert!(n.contains("3 tâches"));
        assert!(n.contains("2 approbations"));
    }

    #[test]
    fn failure_label_is_quoted() {
        let facts = DigestFacts {
            automations_failed: 1,
            first_failure_label: Some("Résumé hebdo".into()),
            ..Default::default()
        };
        let n = render_narration(&facts);
        assert!(n.contains("Résumé hebdo"));
    }
}
