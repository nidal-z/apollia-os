//! The tool calls the journal holds, read for the two timelines.
//!
//! A call is two entries of one run: its start, which names the tool, the
//! agent and the task, and its end, which carries how long it took and whether
//! it succeeded. This module pairs them, and joins a call to its arguments and
//! its output when a reader asks for them.
//!
//! The readers open the journal file read-only, beside the actor that writes
//! it, and are best-effort like the other sources of a timeline: a journal
//! that is absent or unreadable yields no call rather than failing the view.

use std::path::Path;

use rusqlite::{params, Connection, OpenFlags};
use serde_json::Value;

use crate::audit_journal::actor::row_to_entry;
use crate::audit_journal::detail::tool_call_detail;
use crate::audit_journal::entry::{JournalEntry, JournalEntryKind};

/// Columns `row_to_entry` reads, in the order it reads them.
const ENTRY_COLUMNS: &str =
    "seq, run_id, ts, kind, payload, prev_hash, hash, signature, signing_key_id";

/// One tool call, as the journal recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournaledToolCall {
    /// Run the call belongs to.
    pub run_id: String,
    /// Position of the start of the call in its run.
    pub seq: u64,
    /// Name of the tool.
    pub tool_name: String,
    /// Agent that called it.
    pub agent_id: String,
    /// Task the call ran under. A chat turn runs under no task: its run
    /// stands for it.
    pub task_id: String,
    /// When the call started, as an RFC 3339 UTC instant.
    pub started_at: String,
    /// How long the call took. `None` while the journal holds no end for it.
    pub duration_ms: Option<i64>,
    /// Whether the call succeeded. `None` while the journal holds no end for it.
    pub success: Option<bool>,
    /// Arguments of the call, when asked for and when a model response of the
    /// run carries them.
    pub arguments: Option<Value>,
    /// Output of the call, when asked for and when the run captured one.
    pub output: Option<Value>,
}

/// The calls of the task `task_id`, oldest first, each with its arguments and
/// its output.
#[must_use]
pub fn calls_of_task(journal_db: &Path, task_id: &str) -> Vec<JournaledToolCall> {
    let Some(conn) = open_read_only(journal_db) else {
        return Vec::new();
    };
    let mut calls = Vec::new();
    for run_id in runs_naming(&conn, task_id) {
        let run = entries(
            &conn,
            &format!(
                "SELECT {ENTRY_COLUMNS} FROM audit_journal_entries \
                 WHERE run_id = ?1 ORDER BY seq ASC"
            ),
            &run_id,
        );
        calls.extend(
            pair_calls(&run, true)
                .into_iter()
                .filter(|call| call.task_id == task_id),
        );
    }
    calls.sort_by(|a, b| a.started_at.cmp(&b.started_at).then(a.seq.cmp(&b.seq)));
    calls
}

/// The calls started at or after `cutoff` (an RFC 3339 UTC instant), newest
/// first, without their arguments nor their output.
#[must_use]
pub fn calls_since(journal_db: &Path, cutoff: &str) -> Vec<JournaledToolCall> {
    let Some(conn) = open_read_only(journal_db) else {
        return Vec::new();
    };
    let recent = entries(
        &conn,
        &format!(
            "SELECT {ENTRY_COLUMNS} FROM audit_journal_entries \
             WHERE kind IN ('tool_call_started', 'tool_call_completed') AND ts >= ?1 \
             ORDER BY run_id ASC, seq ASC"
        ),
        cutoff,
    );
    let mut calls = Vec::new();
    for run in recent.chunk_by(|a, b| a.run_id == b.run_id) {
        calls.extend(pair_calls(run, false));
    }
    calls.sort_by(|a, b| b.started_at.cmp(&a.started_at).then(b.seq.cmp(&a.seq)));
    calls
}

fn open_read_only(journal_db: &Path) -> Option<Connection> {
    Connection::open_with_flags(journal_db, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()
}

/// The runs that hold the start of a call naming `task_id`. The text match
/// only narrows the read: the caller checks the task of each call it keeps.
fn runs_naming(conn: &Connection, task_id: &str) -> Vec<String> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT DISTINCT run_id FROM audit_journal_entries \
         WHERE kind = 'tool_call_started' AND instr(payload, ?1) > 0 \
         ORDER BY run_id ASC",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map(params![task_id], |row| row.get::<_, String>(0)) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

fn entries(conn: &Connection, sql: &str, parameter: &str) -> Vec<JournalEntry> {
    let Ok(mut stmt) = conn.prepare(sql) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map(params![parameter], row_to_entry) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

/// Pairs each start of `run` (ordered by ascending `seq`) with the end that
/// follows it: the same call identifier when the start carries one, the same
/// tool otherwise. An end is given to one start only.
fn pair_calls(run: &[JournalEntry], with_detail: bool) -> Vec<JournaledToolCall> {
    let mut taken = vec![false; run.len()];
    let mut calls = Vec::new();
    for (position, start) in run.iter().enumerate() {
        if start.kind != JournalEntryKind::ToolCallStarted {
            continue;
        }
        let call_id = text(start, "tool_call_id");
        let tool_name = text(start, "tool_name").unwrap_or_default();
        let end = run
            .iter()
            .enumerate()
            .skip(position + 1)
            .find(|(index, entry)| {
                !taken[*index]
                    && entry.kind == JournalEntryKind::ToolCallCompleted
                    && match call_id {
                        Some(id) => text(entry, "tool_call_id") == Some(id),
                        None => text(entry, "tool_name") == Some(tool_name),
                    }
            })
            .map(|(index, entry)| {
                taken[index] = true;
                entry
            });
        let detail = match (with_detail, call_id) {
            (true, Some(id)) => Some(tool_call_detail(run, position, id)),
            _ => None,
        };
        let (arguments, output) = detail.map_or((None, None), |d| (d.arguments, d.output));
        calls.push(JournaledToolCall {
            run_id: start.run_id.clone(),
            seq: start.seq,
            tool_name: tool_name.to_owned(),
            agent_id: text(start, "agent_id").unwrap_or_default().to_owned(),
            task_id: text(start, "task_id").unwrap_or_default().to_owned(),
            started_at: start.ts.clone(),
            duration_ms: end.and_then(|entry| entry.payload.get("duration_ms")?.as_i64()),
            success: end.and_then(|entry| entry.payload.get("success")?.as_bool()),
            arguments,
            output,
        });
    }
    calls
}

fn text<'a>(entry: &'a JournalEntry, field: &str) -> Option<&'a str> {
    entry.payload.get(field)?.as_str()
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Writes tool calls to a journal file the way a run does, for the tests
    //! of the readers that sit on top of this module.

    use std::path::Path;

    use serde_json::json;

    use crate::audit_journal::{AuditJournalHandle, JournalEntryDraft, JournalEntryKind};

    /// One call to write: the run, the task, the agent, the tool, the call
    /// identifier, the instant it starts, and how it ends (`None` leaves the
    /// call without an end).
    pub(crate) struct SeededCall<'a> {
        pub(crate) run_id: &'a str,
        pub(crate) task_id: &'a str,
        pub(crate) agent_id: &'a str,
        pub(crate) tool_name: &'a str,
        pub(crate) tool_call_id: &'a str,
        pub(crate) started_at: &'a str,
        pub(crate) arguments: serde_json::Value,
        pub(crate) end: Option<(bool, i64, &'a str)>,
    }

    /// Appends `calls` to the journal at `journal_db`, each one as a run
    /// writes it: the model response that asks for the call, its start, the
    /// capture of its output and its end.
    pub(crate) async fn seed_calls(journal_db: &Path, calls: &[SeededCall<'_>]) {
        let journal = AuditJournalHandle::open(journal_db)
            .await
            .expect("journal opens");
        for call in calls {
            let draft = |kind, payload| JournalEntryDraft {
                run_id: call.run_id.to_owned(),
                ts: call.started_at.to_owned(),
                kind,
                payload,
            };
            journal.append(draft(
                JournalEntryKind::LlmCompletion,
                json!({ "tool_calls": [{ "id": call.tool_call_id, "arguments": call.arguments }] }),
            ));
            journal.append(draft(
                JournalEntryKind::ToolCallStarted,
                json!({
                    "tool_name": call.tool_name,
                    "agent_id": call.agent_id,
                    "task_id": call.task_id,
                    "tool_call_id": call.tool_call_id,
                }),
            ));
            if let Some((success, duration_ms, output)) = call.end {
                journal.append(draft(
                    JournalEntryKind::ToolOutput,
                    json!({
                        "tool_call_id": call.tool_call_id,
                        "output": output,
                        "status": if success { "success" } else { "error" },
                    }),
                ));
                journal.append(draft(
                    JournalEntryKind::ToolCallCompleted,
                    json!({
                        "tool_name": call.tool_name,
                        "agent_id": call.agent_id,
                        "success": success,
                        "duration_ms": duration_ms,
                        "tool_call_id": call.tool_call_id,
                    }),
                ));
            }
        }
        journal.shutdown().await;
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{seed_calls, SeededCall};
    use super::*;
    use serde_json::json;

    fn call<'a>(
        run_id: &'a str,
        task_id: &'a str,
        tool_call_id: &'a str,
        started_at: &'a str,
        end: Option<(bool, i64, &'a str)>,
    ) -> SeededCall<'a> {
        SeededCall {
            run_id,
            task_id,
            agent_id: "scribe",
            tool_name: "file_read",
            tool_call_id,
            started_at,
            arguments: json!({ "path": "notes.md" }),
            end,
        }
    }

    // The calls of a task come back oldest first, each joined to its end, its
    // arguments and its output, and the calls of another task stay out.
    #[tokio::test]
    async fn test_calls_of_a_task_are_paired_and_joined_to_their_detail() {
        // GIVEN a journal with two calls of one task and one call of another
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("audit_journal.db");
        seed_calls(
            &db,
            &[
                call(
                    "run-1",
                    "task-1",
                    "c1",
                    "2026-03-13T10:00:03Z",
                    Some((false, 70, "denied")),
                ),
                call(
                    "run-1",
                    "task-1",
                    "c0",
                    "2026-03-13T10:00:02Z",
                    Some((true, 40, "the notes")),
                ),
                call("run-2", "task-10", "c0", "2026-03-13T10:00:04Z", None),
            ],
        )
        .await;

        // WHEN the calls of the first task are read
        let calls = calls_of_task(&db, "task-1");

        // THEN its two calls come back oldest first
        let starts: Vec<&str> = calls.iter().map(|c| c.started_at.as_str()).collect();
        assert_eq!(starts, vec!["2026-03-13T10:00:02Z", "2026-03-13T10:00:03Z"]);
        // AND each one carries its own end, its arguments and its output
        assert_eq!(calls[0].success, Some(true));
        assert_eq!(calls[0].duration_ms, Some(40));
        assert_eq!(calls[0].output, Some(json!("the notes")));
        assert_eq!(calls[0].arguments, Some(json!({ "path": "notes.md" })));
        assert_eq!(calls[1].success, Some(false));
        assert_eq!(calls[1].duration_ms, Some(70));
        assert_eq!(calls[1].output, Some(json!("denied")));
        // AND the call of the task whose name only starts the same stays out
        assert!(calls.iter().all(|c| c.task_id == "task-1"), "{calls:?}");
    }

    // The recent calls come back newest first, across runs, and a call that
    // has not ended yet is still listed.
    #[tokio::test]
    async fn test_calls_since_a_cutoff_come_newest_first_without_detail() {
        // GIVEN a call before the cutoff, one after it, and one still running
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("audit_journal.db");
        seed_calls(
            &db,
            &[
                call(
                    "run-1",
                    "task-1",
                    "c0",
                    "2026-03-13T09:00:00Z",
                    Some((true, 5, "old")),
                ),
                call(
                    "run-1",
                    "task-1",
                    "c1",
                    "2026-03-13T10:00:02Z",
                    Some((true, 40, "new")),
                ),
                call("run-2", "task-2", "c0", "2026-03-13T10:00:04Z", None),
            ],
        )
        .await;

        // WHEN the calls since the cutoff are read
        let calls = calls_since(&db, "2026-03-13T10:00:00Z");

        // THEN the two recent calls come back newest first
        let runs: Vec<&str> = calls.iter().map(|c| c.run_id.as_str()).collect();
        assert_eq!(runs, vec!["run-2", "run-1"]);
        // AND the running one has no end yet
        assert_eq!(calls[0].success, None);
        assert_eq!(calls[0].duration_ms, None);
        assert_eq!(calls[1].success, Some(true));
        // AND no arguments nor output are read for this view
        assert!(calls
            .iter()
            .all(|c| c.arguments.is_none() && c.output.is_none()));
    }

    // A journal that does not exist yields no call instead of an error.
    #[test]
    fn test_an_absent_journal_yields_no_call() {
        // GIVEN a path where no journal was ever written
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("audit_journal.db");

        // WHEN both readers are asked
        // THEN each one answers with nothing
        assert!(calls_of_task(&db, "task-1").is_empty());
        assert!(calls_since(&db, "2026-03-13T10:00:00Z").is_empty());
    }
}
