#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Cross-crate integration: plan-construction capture round-trips through the
//! replay harness using only the public API.
//!
//! Builds a journal slice for a synthetic run whose plan is constructed by a
//! Propose -> AddStep -> Submit stream, then replays it. An untouched trace
//! replays Identical; a trace with one altered plan mutation replays Diverged
//! and names the diverging mutation.

use apollia_core::events::RunId;
use apollia_core::plan::{Plan, PlanMutationKind, PlanScope, PlanStatus, PlanStep};
use apollia_runtime::audit_journal::entry::{JournalEntry, JournalEntryKind, PlanMutationSnapshot};
use apollia_runtime::replay::{ReplayBundle, ReplayHarness, ReplayReport};

fn plan(revision: u32, steps: Vec<PlanStep>) -> Plan {
    Plan {
        plan_id: "plan-1".into(),
        scope: PlanScope::Session("sess-1".into()),
        revision,
        status: PlanStatus::Draft,
        steps,
    }
}

fn plan_snapshot(
    run: &RunId,
    ordinal: u32,
    kind: PlanMutationKind,
    step_id: Option<&str>,
    steps: Vec<PlanStep>,
) -> PlanMutationSnapshot {
    PlanMutationSnapshot {
        run_id: run.as_str().to_string(),
        session_id: "sess-1".into(),
        ordinal,
        kind,
        step_id: step_id.map(str::to_string),
        reason: Some("constructing the plan".into()),
        before: None,
        after: step_id.map(|id| PlanStep::new(id, "step")),
        revision: ordinal + 1,
        plan: plan(ordinal + 1, steps),
    }
}

fn journal_entry<T: serde::Serialize>(
    run: &RunId,
    seq: u64,
    kind: JournalEntryKind,
    payload: &T,
) -> JournalEntry {
    JournalEntry {
        seq,
        run_id: run.as_str().to_string(),
        ts: "2026-06-10T00:00:00Z".into(),
        kind,
        payload: serde_json::to_value(payload).expect("serialize"),
        prev_hash: "x".into(),
        hash: "y".into(),
        signature: None,
        signing_key_id: None,
    }
}

fn llm_entry(run: &RunId, seq: u64) -> JournalEntry {
    let snap = serde_json::json!({
        "run_id": run.as_str(),
        "step_ordinal": 0,
        "backend_name": "local",
        "model_id": "m",
        "content": "final",
        "tool_calls": [],
        "prompt_tokens": 0,
        "completion_tokens": 0,
        "cost_usd": null,
        "stream_truncated": false,
    });
    journal_entry(run, seq, JournalEntryKind::LlmCompletion, &snap)
}

fn construction_trace(run: &RunId) -> Vec<PlanMutationSnapshot> {
    vec![
        plan_snapshot(
            run,
            0,
            PlanMutationKind::Propose,
            None,
            vec![PlanStep::new("s1", "first")],
        ),
        plan_snapshot(
            run,
            1,
            PlanMutationKind::AddStep,
            Some("s2"),
            vec![PlanStep::new("s1", "first"), PlanStep::new("s2", "second")],
        ),
        plan_snapshot(
            run,
            2,
            PlanMutationKind::Submit,
            None,
            vec![PlanStep::new("s1", "first"), PlanStep::new("s2", "second")],
        ),
    ]
}

fn journal_for(run: &RunId, plan_trace: &[PlanMutationSnapshot]) -> Vec<JournalEntry> {
    let mut entries = vec![llm_entry(run, 0)];
    for (i, snap) in plan_trace.iter().enumerate() {
        entries.push(journal_entry(
            run,
            (i + 1) as u64,
            JournalEntryKind::PlanMutation,
            snap,
        ));
    }
    entries
}

#[tokio::test]
async fn test_plan_construction_replays_identical() {
    // GIVEN a captured run whose plan was built by Propose -> AddStep -> Submit
    let run = RunId::new();
    let trace = construction_trace(&run);
    let entries = journal_for(&run, &trace);
    let bundle = ReplayBundle::from_journal(&entries, &run).expect("bundle");
    let mut harness = ReplayHarness::from_bundle(&run, bundle);

    // WHEN the run is replayed against its own trace
    let report = harness.run().await.expect("run");

    // THEN the plan construction replays identically
    assert!(matches!(report, ReplayReport::Identical), "got {report:?}");
}

#[tokio::test]
async fn test_altered_plan_mutation_replays_diverged() {
    // GIVEN a captured trace and a produced stream whose ordinal-1 mutation
    // differs (AddStep of a different step) from the captured AddStep
    let run = RunId::new();
    let trace = construction_trace(&run);
    let entries = journal_for(&run, &trace);
    let mut produced = construction_trace(&run);
    produced[1] = plan_snapshot(
        &run,
        1,
        PlanMutationKind::AddStep,
        Some("s9"),
        vec![PlanStep::new("s1", "first"), PlanStep::new("s9", "drifted")],
    );
    let bundle = ReplayBundle::from_journal(&entries, &run).expect("bundle");
    let mut harness = ReplayHarness::from_bundle(&run, bundle).with_produced_plan(produced);

    // WHEN the run is replayed
    let report = harness.run().await.expect("run");

    // THEN a plan_mutation divergence at ordinal 1 is reported
    match report {
        ReplayReport::Diverged { divergences } => {
            let plan_divs: Vec<_> = divergences
                .iter()
                .filter(|d| d.kind == "plan_mutation")
                .collect();
            assert_eq!(plan_divs.len(), 1);
            assert_eq!(plan_divs[0].step_ordinal, 1);
        }
        other => panic!("expected Diverged, got {other:?}"),
    }
}
