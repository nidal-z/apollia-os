//! The detail of a journal entry, read on demand.
//!
//! The entry of a tool call holds neither its arguments nor its output. Both
//! are already in the journal, written for replay: the arguments in the model
//! response that asked for the call, the output in the capture that followed
//! it. The detail joins them to the call by the run and the call identifier.

use serde::Serialize;
use serde_json::Value;

use crate::audit_journal::entry::{JournalEntry, JournalEntryKind};
use crate::audit_journal::summary::JournalLine;

/// The arguments and the output of one tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, utoipa::ToSchema)]
pub struct ToolCallDetail {
    /// Identifier of the call within its run.
    pub tool_call_id: String,
    /// Full arguments of the call. `None` when no model response of the run
    /// carries this call, as for a tool an agent's own code called.
    #[schema(value_type = Option<Object>)]
    pub arguments: Option<Value>,
    /// Full output of the call. `None` when the run captured none.
    #[schema(value_type = Option<Object>)]
    pub output: Option<Value>,
    /// How the call ended, as the capture wrote it: `success`, `error` or
    /// `rejected`.
    pub status: Option<String>,
}

/// A journal entry with everything it records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JournalEntryDetail {
    /// The entry and its summary.
    #[serde(flatten)]
    pub line: JournalLine,
    /// Arguments and output, when the entry is about a tool call.
    pub tool_call: Option<ToolCallDetail>,
}

/// The detail of the entry `seq` of a run, whose entries are `run`, ordered by
/// ascending `seq`. `None` when the run has no such entry.
pub fn entry_detail(run: &[JournalEntry], seq: u64) -> Option<JournalEntryDetail> {
    let position = run.iter().position(|entry| entry.seq == seq)?;
    let entry = &run[position];
    let tool_call = call_id_of(entry).map(|id| tool_call_detail(run, position, id));
    Some(JournalEntryDetail {
        line: JournalLine::of(entry.clone()),
        tool_call,
    })
}

/// The call an entry is about, for the three kinds that are about one.
fn call_id_of(entry: &JournalEntry) -> Option<&str> {
    match entry.kind {
        JournalEntryKind::ToolCallStarted
        | JournalEntryKind::ToolCallCompleted
        | JournalEntryKind::ToolOutput => entry.payload.get("tool_call_id")?.as_str(),
        _ => None,
    }
}

/// Joins the call `id`, seen at `position`, to its arguments and its output.
///
/// A model may reuse an identifier later in the same run, so the join stays
/// within one call: from the last model response at or before `position` that
/// asks for `id`, up to the next response that asks for it again.
pub(crate) fn tool_call_detail(run: &[JournalEntry], position: usize, id: &str) -> ToolCallDetail {
    let asked = run[..=position]
        .iter()
        .rposition(|entry| requested_arguments(entry, id).is_some());
    let start = asked.unwrap_or(0);
    let end = run
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, entry)| requested_arguments(entry, id).is_some())
        .map_or(run.len(), |(index, _)| index);
    let output = run[start..end].iter().find(|entry| {
        entry.kind == JournalEntryKind::ToolOutput
            && entry.payload.get("tool_call_id").and_then(Value::as_str) == Some(id)
    });
    ToolCallDetail {
        tool_call_id: id.to_owned(),
        arguments: asked.and_then(|index| requested_arguments(&run[index], id)),
        output: output.and_then(|entry| entry.payload.get("output").cloned()),
        status: output
            .and_then(|entry| entry.payload.get("status"))
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

/// The arguments a model response asks the call `id` with, when `entry` is a
/// model response that asks for it.
fn requested_arguments(entry: &JournalEntry, id: &str) -> Option<Value> {
    if entry.kind != JournalEntryKind::LlmCompletion {
        return None;
    }
    entry
        .payload
        .get("tool_calls")?
        .as_array()?
        .iter()
        .find(|call| call.get("id").and_then(Value::as_str) == Some(id))?
        .get("arguments")
        .cloned()
}

#[cfg(test)]
mod tests;
