//! Injection tracker: tracks memory entries injected into agent turns.
//!
//! Supports memory injection visibility. Each time an
//! agent calls `recall_entry()` / `recall_all()` and surfaces the result into a
//! turn, the wrapper records an [`InjectedEntry`] under the corresponding
//! `TurnId`. The desktop frontend reads the tracker via the Tauri command
//! `get_injected_memory_entries(turn_id)` to render the
//! `<InjectedMemorySheet />` panel.
//!
//! The tracker is process-local (runtime in-memory) and purges turns beyond a
//! fixed window (default 50) to keep memory bounded.
//!
//! Memory is never injected automatically: this module records
//! *agent-initiated* injections so that the user can audit what the agent
//! pulled into context.

use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

/// Default number of turns retained by the tracker before purge.
pub const DEFAULT_MAX_TURNS: usize = 50;

/// A memory entry that the agent injected into a specific turn.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InjectedEntry {
    /// Source memory entry id (semantic/episodic/procedural UUID).
    pub id: String,
    /// Short preview of the entry content (truncated to ~160 chars).
    pub content_preview: String,
    /// Namespace of origin (primary or shared).
    pub namespace: String,
    /// Agent-supplied rationale (why this entry was injected).
    ///
    /// Fallback = `"Matched query: <query>"` when the agent does not supply
    /// an explicit reason.
    pub injection_reason: String,
    /// Relevance score, normalized to `[0.0, 1.0]` when available.
    pub relevance_score: f32,
}

/// Process-local tracker keyed by turn id.
#[derive(Debug, Default)]
pub struct InjectionTracker {
    /// Max number of turns retained (LRU eviction beyond this window).
    max_turns: usize,
    /// Entries per turn.
    by_turn: HashMap<String, Vec<InjectedEntry>>,
    /// Insertion order (used to purge the oldest turn on overflow).
    order: VecDeque<String>,
}

impl InjectionTracker {
    /// Creates a tracker retaining at most `max_turns` turns.
    pub fn new(max_turns: usize) -> Self {
        Self {
            max_turns: max_turns.max(1),
            by_turn: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// Records an injected entry for `turn_id`.
    ///
    /// When the number of distinct turns exceeds `max_turns`, the oldest turn
    /// is purged (FIFO).
    pub fn record(&mut self, turn_id: impl Into<String>, entry: InjectedEntry) {
        let turn_id = turn_id.into();
        let is_new = !self.by_turn.contains_key(&turn_id);
        self.by_turn.entry(turn_id.clone()).or_default().push(entry);

        if is_new {
            self.order.push_back(turn_id);
            while self.order.len() > self.max_turns {
                if let Some(evicted) = self.order.pop_front() {
                    self.by_turn.remove(&evicted);
                }
            }
        }
    }

    /// Returns a clone of the entries injected for `turn_id` (empty when none).
    pub fn entries_for(&self, turn_id: &str) -> Vec<InjectedEntry> {
        self.by_turn.get(turn_id).cloned().unwrap_or_default()
    }

    /// Total number of turns currently tracked.
    pub fn tracked_turns(&self) -> usize {
        self.order.len()
    }

    /// Clears all recorded turns.
    pub fn clear(&mut self) {
        self.by_turn.clear();
        self.order.clear();
    }
}

/// Truncates `content` to at most `max` chars with a trailing ellipsis.
pub fn preview(content: &str, max: usize) -> String {
    if content.chars().count() <= max {
        return content.to_string();
    }
    let mut out: String = content.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Global singleton used by the desktop `get_injected_memory_entries` command
/// and by the PyO3 `recall_entry()` wrapper.
///
/// The tracker is process-local; unit tests that depend on a clean state
/// should call [`global_tracker_clear`] at the start.
pub fn global_tracker() -> &'static Mutex<InjectionTracker> {
    static TRACKER: OnceLock<Mutex<InjectionTracker>> = OnceLock::new();
    TRACKER.get_or_init(|| Mutex::new(InjectionTracker::new(DEFAULT_MAX_TURNS)))
}

/// Records an injection in the global tracker.
pub fn global_record(turn_id: impl Into<String>, entry: InjectedEntry) {
    if let Ok(mut guard) = global_tracker().lock() {
        guard.record(turn_id, entry);
    }
}

/// Reads entries for a turn from the global tracker.
pub fn global_entries_for(turn_id: &str) -> Vec<InjectedEntry> {
    global_tracker()
        .lock()
        .map(|g| g.entries_for(turn_id))
        .unwrap_or_default()
}

/// Clears the global tracker, test-only helper.
#[doc(hidden)]
pub fn global_tracker_clear() {
    if let Ok(mut guard) = global_tracker().lock() {
        guard.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(id: &str) -> InjectedEntry {
        InjectedEntry {
            id: id.to_string(),
            content_preview: "hello".to_string(),
            namespace: "ns".to_string(),
            injection_reason: "Matched query: hello".to_string(),
            relevance_score: 0.8,
        }
    }

    #[test]
    fn record_and_fetch_returns_entries() {
        // GIVEN a fresh tracker
        let mut t = InjectionTracker::new(10);

        // WHEN two entries are recorded for turn-1
        t.record("turn-1", sample_entry("e-1"));
        t.record("turn-1", sample_entry("e-2"));

        // THEN both are returned in insertion order
        let got = t.entries_for("turn-1");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].id, "e-1");
        assert_eq!(got[1].id, "e-2");
    }

    #[test]
    fn unknown_turn_returns_empty() {
        // GIVEN an empty tracker
        let t = InjectionTracker::new(10);

        // WHEN querying an unknown turn
        // THEN the result is empty
        assert!(t.entries_for("nope").is_empty());
    }

    #[test]
    fn fifo_eviction_beyond_max_turns() {
        // GIVEN a tracker with a 2-turn window
        let mut t = InjectionTracker::new(2);

        // WHEN three turns are recorded
        t.record("t1", sample_entry("a"));
        t.record("t2", sample_entry("b"));
        t.record("t3", sample_entry("c"));

        // THEN the oldest turn is evicted
        assert_eq!(t.tracked_turns(), 2);
        assert!(t.entries_for("t1").is_empty());
        assert_eq!(t.entries_for("t2").len(), 1);
        assert_eq!(t.entries_for("t3").len(), 1);
    }

    #[test]
    fn preview_truncates_long_strings() {
        // GIVEN a long input and limit 5
        // WHEN previewed
        // THEN result has 5 chars ending with ellipsis
        let out = preview("abcdefghij", 5);
        assert_eq!(out.chars().count(), 5);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn preview_short_input_is_unchanged() {
        // GIVEN an input shorter than the preview limit
        // WHEN a preview is taken of it
        // THEN it comes back whole, with nothing appended
        assert_eq!(preview("hi", 10), "hi");
    }
}
