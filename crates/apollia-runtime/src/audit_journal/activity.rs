//! The activity of a connection over a period: when its tools were last used,
//! and how many actions each agent took with them.
//!
//! An action is a tool call that ended, successfully or not: the journal
//! writes one `tool_call_completed` entry per call, whereas its start and the
//! output kept for replay are bookkeeping of the same call and are not counted.
//! The activity is told from the lines the journal read for the filter of the
//! connection, so it holds no content: an agent, a count and a date.

use serde::Serialize;

use crate::audit_journal::summary::JournalLine;
use crate::audit_journal::JournalEntryKind;

/// What one agent did with the tools of a connection over the period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentActivity {
    /// The agent, `None` for calls whose entry names none.
    pub agent: Option<String>,
    /// Tool calls of the connection that ended over the period.
    pub actions: u64,
    /// When the last of them ended, RFC 3339 UTC.
    pub last_used_at: String,
}

/// What every agent did with the tools of a connection over the period.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivityTally {
    /// Tool calls of the connection that ended over the period, every agent
    /// counted.
    pub actions: u64,
    /// When the last of them ended, RFC 3339 UTC; `None` when none did.
    pub last_used_at: Option<String>,
    /// One line per agent, the most active first, then by name.
    pub agents: Vec<AgentActivity>,
}

/// The activity told by `lines`. Every line is expected to belong to the
/// connection and the period already; only the ends of tool calls count.
pub fn tally(lines: &[JournalLine]) -> ActivityTally {
    let mut agents: Vec<AgentActivity> = Vec::new();
    let mut tally = ActivityTally::default();
    for line in lines {
        if line.entry.kind != JournalEntryKind::ToolCallCompleted {
            continue;
        }
        let at = &line.summary.ts;
        tally.actions += 1;
        if tally.last_used_at.as_ref().is_none_or(|last| at > last) {
            tally.last_used_at = Some(at.clone());
        }
        match agents
            .iter_mut()
            .find(|activity| activity.agent == line.summary.agent)
        {
            Some(activity) => {
                activity.actions += 1;
                if *at > activity.last_used_at {
                    activity.last_used_at = at.clone();
                }
            }
            None => agents.push(AgentActivity {
                agent: line.summary.agent.clone(),
                actions: 1,
                last_used_at: at.clone(),
            }),
        }
    }
    agents.sort_by(|a, b| {
        b.actions
            .cmp(&a.actions)
            .then_with(|| a.agent.cmp(&b.agent))
    });
    tally.agents = agents;
    tally
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit_journal::entry::JournalEntry;
    use serde_json::json;

    fn line(kind: JournalEntryKind, agent: Option<&str>, ts: &str) -> JournalLine {
        JournalLine::of(JournalEntry {
            seq: 0,
            run_id: "run-1".into(),
            ts: ts.into(),
            kind,
            payload: json!({ "tool_name": "mcp:notion/search", "agent_id": agent, "success": true }),
            prev_hash: String::new(),
            hash: String::new(),
            signature: None,
            signing_key_id: None,
        })
    }

    #[test]
    fn only_the_ends_of_calls_count_and_each_agent_keeps_its_last_use() {
        // GIVEN two calls by `quotes`, one by `invoices`, a start and a call
        // that names no agent
        let lines = [
            line(
                JournalEntryKind::ToolCallCompleted,
                Some("invoices"),
                "2026-10-05T09:00:00Z",
            ),
            line(
                JournalEntryKind::ToolCallCompleted,
                Some("quotes"),
                "2026-10-04T09:00:00Z",
            ),
            line(
                JournalEntryKind::ToolCallStarted,
                Some("quotes"),
                "2026-10-06T09:00:00Z",
            ),
            line(
                JournalEntryKind::ToolCallCompleted,
                Some("quotes"),
                "2026-10-02T09:00:00Z",
            ),
            line(
                JournalEntryKind::ToolCallCompleted,
                None,
                "2026-10-01T09:00:00Z",
            ),
        ];
        // WHEN they are tallied
        let tally = tally(&lines);
        // THEN four actions count, the last on the fifth, `quotes` first
        assert_eq!(tally.actions, 4);
        assert_eq!(tally.last_used_at.as_deref(), Some("2026-10-05T09:00:00Z"));
        let agents: Vec<_> = tally
            .agents
            .iter()
            .map(|a| (a.agent.as_deref(), a.actions, a.last_used_at.as_str()))
            .collect();
        assert_eq!(
            agents,
            [
                (Some("quotes"), 2, "2026-10-04T09:00:00Z"),
                (None, 1, "2026-10-01T09:00:00Z"),
                (Some("invoices"), 1, "2026-10-05T09:00:00Z"),
            ]
        );
    }

    #[test]
    fn no_call_tells_no_last_use() {
        // GIVEN no line
        // WHEN they are tallied
        let tally = tally(&[]);
        // THEN nothing counts and no last use is told
        assert_eq!(tally, ActivityTally::default());
    }
}
