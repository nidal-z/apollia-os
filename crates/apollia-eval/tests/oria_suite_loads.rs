//! Load check for the ORIA agent-loop eval suite (`evals/oria/agent-loop.toml`).
//!
//! The suite cannot be executed without a loaded model, so nothing else in the
//! tree would notice a malformed assertion until an operator ran it against a
//! live daemon. That is the wrong place to discover a stray parenthesis in a
//! pattern: the runner turns an uncompilable regex into a per-run failure
//! reason, which reads exactly like an agent that answered wrongly.
//!
//! This test loads the suite through the crate's own model and holds the
//! invariants a suite must satisfy to measure anything at all:
//!
//! - it parses, and every assertion has a known shape;
//! - every task runs three times and carries an id used once;
//! - every task is checked by more than an exit code;
//! - every regex pattern compiles;
//! - every regex marker is a marker the prompt actually asks for;
//! - every `file_exists` path stays inside the eval workspace, so no assertion
//!   can ever reach into a real profile;
//! - the four failure-mode tasks are present, by id;
//! - the seed workspace still carries the files the prompts name.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use apollia_eval::{Assertion, EvalSuite, EvalTask};
use regex::Regex;

/// Repository root, derived from this crate's manifest directory.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .to_path_buf()
}

/// Path of the suite under test.
fn suite_path() -> PathBuf {
    repo_root().join("evals/oria/agent-loop.toml")
}

/// Loads the suite, failing the test with the parse error when it does not load.
fn load_suite() -> EvalSuite {
    let path = suite_path();
    match EvalSuite::from_path(&path) {
        Ok(suite) => suite,
        Err(error) => panic!("the ORIA suite at {} must load: {error}", path.display()),
    }
}

/// The uppercase answer marker a pattern keys on, e.g. `ANCHOR:`.
///
/// Returns `None` for a pattern that keys on no marker at all.
fn marker_of(pattern: &str) -> Option<String> {
    let marker = Regex::new(r"[A-Z][A-Z0-9-]*:").expect("the marker regex is a literal");
    marker.find(pattern).map(|m| m.as_str().to_string())
}

/// Every `(task id, pattern)` pair carried by a `regex` assertion of the suite.
fn regex_patterns(suite: &EvalSuite) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for task in &suite.tasks {
        for assertion in &task.assertions {
            if let Assertion::Regex { pattern, .. } = assertion {
                pairs.push((task.id.clone(), pattern.clone()));
            }
        }
    }
    pairs
}

/// Returns the first uncompilable pattern of the suite, as `(task id, reason)`.
///
/// This is the check the negative test below sabotages, so the two run the same
/// code rather than two lookalikes.
fn first_uncompilable_pattern(suite: &EvalSuite) -> Option<(String, String)> {
    regex_patterns(suite)
        .into_iter()
        .find_map(|(task_id, pattern)| match Regex::new(&pattern) {
            Ok(_) => None,
            Err(error) => Some((task_id, format!("/{pattern}/: {error}"))),
        })
}

/// Whether a task carries at least one assertion on what it answered, rather
/// than only on how it exited.
fn is_checked_on_content(task: &EvalTask) -> bool {
    task.assertions
        .iter()
        .any(|a| matches!(a, Assertion::Regex { .. } | Assertion::LlmJudge { .. }))
}

#[test]
fn test_oria_suite_loads_with_its_ten_tasks() {
    // GIVEN the ORIA agent-loop suite on disk
    // WHEN it is loaded through the crate's own model
    let suite = load_suite();

    // THEN it names itself and carries the ten agreed tasks
    assert_eq!(suite.name, "oria-agent-loop-failure-modes");
    assert_eq!(
        suite.tasks.len(),
        10,
        "the suite is ten tasks; got {}",
        suite.tasks.len()
    );
}

#[test]
fn test_every_task_runs_three_times_under_a_unique_id() {
    // GIVEN the loaded suite
    let suite = load_suite();

    // WHEN every task's run count and id are read
    let mut ids: Vec<&str> = suite.tasks.iter().map(|t| t.id.as_str()).collect();
    ids.sort_unstable();
    let unique = {
        let mut seen = ids.clone();
        seen.dedup();
        seen.len()
    };

    // THEN each runs three times, and no id is used twice
    for task in &suite.tasks {
        assert_eq!(task.runs, 3, "task {} must run 3 times", task.id);
        assert!(
            !task.prompt.trim().is_empty(),
            "task {} has no prompt",
            task.id
        );
    }
    assert_eq!(unique, ids.len(), "task ids must be unique, got {ids:?}");
}

#[test]
fn test_every_task_is_checked_on_what_it_answered() {
    // GIVEN the loaded suite
    let suite = load_suite();

    // WHEN each task's assertions are inspected
    let unchecked: Vec<&str> = suite
        .tasks
        .iter()
        .filter(|t| !is_checked_on_content(t))
        .map(|t| t.id.as_str())
        .collect();

    // THEN none is judged by its exit code alone: a task that only asserts an
    // exit code passes on any answer, including a wrong one
    assert!(
        unchecked.is_empty(),
        "these tasks assert nothing about their answer: {unchecked:?}"
    );
}

#[test]
fn test_every_regex_pattern_compiles() {
    // GIVEN the loaded suite
    let suite = load_suite();

    // WHEN every regex assertion is compiled
    let broken = first_uncompilable_pattern(&suite);

    // THEN none is rejected by the regex engine the runner uses
    assert!(
        broken.is_none(),
        "an uncompilable pattern would read as a wrong answer at run time: {broken:?}"
    );
}

#[test]
fn test_every_regex_marker_is_a_marker_its_prompt_asks_for() {
    // GIVEN the loaded suite
    let suite = load_suite();

    // WHEN each pattern's answer marker is checked against its own prompt
    let mut orphans: Vec<String> = Vec::new();
    for task in &suite.tasks {
        for assertion in &task.assertions {
            let Assertion::Regex { pattern, .. } = assertion else {
                continue;
            };
            let Some(marker) = marker_of(pattern) else {
                orphans.push(format!(
                    "{}: pattern /{pattern}/ keys on no marker",
                    task.id
                ));
                continue;
            };
            if !task.prompt.contains(&marker) {
                orphans.push(format!(
                    "{}: pattern /{pattern}/ expects `{marker}` but the prompt never asks for it",
                    task.id
                ));
            }
        }
    }

    // THEN every assertion checks a marker the agent was actually told to emit.
    // Without this, renaming a marker in a prompt leaves the assertion checking
    // a string nothing can ever produce, and the task is red forever for a
    // reason no report explains.
    assert!(orphans.is_empty(), "{orphans:#?}");
}

#[test]
fn test_file_exists_paths_stay_inside_the_eval_workspace() {
    // GIVEN the loaded suite
    let suite = load_suite();

    // WHEN every file_exists path is inspected
    let mut escaping: Vec<String> = Vec::new();
    for task in &suite.tasks {
        for assertion in &task.assertions {
            let Assertion::FileExists { path } = assertion else {
                continue;
            };
            let inside = path.starts_with("evals/oria/run/")
                && !path.contains("..")
                && !Path::new(path).is_absolute();
            if !inside {
                escaping.push(format!("{}: {path}", task.id));
            }
        }
    }

    // THEN each one is a relative path under the throwaway run workspace. An
    // absolute path, or one climbing out with `..`, could assert on a real
    // profile, which this suite must never touch.
    assert!(
        escaping.is_empty(),
        "file_exists paths must stay under evals/oria/run/: {escaping:#?}"
    );
}

#[test]
fn test_the_four_failure_mode_tasks_are_present() {
    // GIVEN the loaded suite
    let suite = load_suite();
    let ids: Vec<&str> = suite.tasks.iter().map(|t| t.id.as_str()).collect();

    // WHEN the four tasks the suite exists for are looked up
    let required = [
        "05-recovery-after-failing-command",
        "06-hitl-asks-instead-of-refusing",
        "07-budget-stops-and-says-why",
        "08-ambiguity-asks-instead-of-inventing",
    ];

    // THEN each is there. A suite that keeps only the reading tasks measures
    // what an agent loop rarely gets wrong.
    for id in required {
        assert!(
            ids.contains(&id),
            "task {id} is missing; suite holds {ids:?}"
        );
    }
}

#[test]
fn test_the_judged_task_carries_a_non_empty_rubric() {
    // GIVEN the loaded suite
    let suite = load_suite();

    // WHEN the llm_judge assertions are collected
    let rubrics: Vec<&String> = suite
        .tasks
        .iter()
        .flat_map(|t| t.assertions.iter())
        .filter_map(|a| match a {
            Assertion::LlmJudge { rubric } => Some(rubric),
            _ => None,
        })
        .collect();

    // THEN there is one, and it states both a pass and a fail condition. A
    // rubric that only says what passes gives the judge nothing to refuse.
    assert_eq!(rubrics.len(), 1, "the suite carries exactly one rubric");
    let rubric = rubrics[0];
    assert!(
        rubric.contains("PASS"),
        "rubric must state its pass condition"
    );
    assert!(
        rubric.contains("FAIL"),
        "rubric must state its fail condition"
    );
}

#[test]
fn test_the_seed_workspace_carries_the_files_the_prompts_name() {
    // GIVEN the seed workspace the suite is played against
    let seed = repo_root().join("evals/oria/seed");

    // WHEN the paths named in the prompts are looked up
    let required = [
        "config/service.conf",
        "config/relay.conf",
        "notes/handover-2026-04.md",
        "toolbox/drift.txt",
        "inventory/depot-a.csv",
        "inventory/depot-b.csv",
        "inventory/depot-c.csv",
        "tools/collect.sh",
    ];

    // THEN every one exists. A deleted seed file turns a task red with a
    // failure reason that blames the agent for a file that was never there.
    for relative in required {
        let path = seed.join(relative);
        assert!(path.exists(), "seed file missing: {}", path.display());
    }
}

// (error case) the load check must be able to reject a suite, not only accept one
#[test]
fn test_an_unknown_assertion_type_is_refused_by_the_loader() {
    // GIVEN a suite shaped like ours but with an assertion type the model has no
    // variant for
    let toml = r#"
        name = "sabotaged"

        [[tasks]]
        id = "01-anchoring"
        runs = 3
        prompt = "answer with ANCHOR: <value>"
        assertions = [
            { type = "screenshot_matches", path = "a.png" },
        ]
    "#;

    // WHEN it is loaded
    let outcome = EvalSuite::from_toml_str(toml);

    // THEN the loader refuses it rather than dropping the assertion silently
    assert!(
        outcome.is_err(),
        "an unknown assertion type must be a load error, not a skipped check"
    );
}

// (error case) the pattern check must be able to fail on a real malformed pattern
#[test]
fn test_the_pattern_check_catches_an_uncompilable_pattern() {
    // GIVEN a suite whose regex assertion carries an unbalanced group
    let toml = r#"
        name = "sabotaged"

        [[tasks]]
        id = "01-anchoring"
        runs = 3
        prompt = "answer with ANCHOR: <value>"
        assertions = [
            { type = "regex", on = "result", pattern = "ANCHOR:(" },
        ]
    "#;
    let suite = EvalSuite::from_toml_str(toml).expect("the TOML itself is well formed");

    // WHEN the same check the green test runs is applied
    let broken = first_uncompilable_pattern(&suite);

    // THEN it names the offending task and pattern. The TOML parses, so only
    // this check stands between a stray parenthesis and a report that blames
    // the agent for it.
    let (task_id, reason) = broken.expect("an unbalanced group must be caught");
    assert_eq!(task_id, "01-anchoring");
    assert!(
        reason.contains("ANCHOR:("),
        "the reason names the pattern: {reason}"
    );
}

// (error case) the marker check must be able to fail on a renamed marker
#[test]
fn test_the_marker_check_catches_a_pattern_no_prompt_asks_for() {
    // GIVEN a task whose prompt asks for one marker and whose assertion checks
    // another, which is what a rename leaves behind
    let toml = r#"
        name = "sabotaged"

        [[tasks]]
        id = "01-anchoring"
        runs = 3
        prompt = "answer with ANCHOR: <value>"
        assertions = [
            { type = "regex", on = "result", pattern = "(?m)^VALVE:" },
        ]
    "#;
    let suite = EvalSuite::from_toml_str(toml).expect("the TOML itself is well formed");
    let task = &suite.tasks[0];

    // WHEN the marker of the pattern is compared against the prompt
    let Assertion::Regex { pattern, .. } = &task.assertions[0] else {
        panic!("the fixture carries a regex assertion");
    };
    let marker = marker_of(pattern).expect("the pattern keys on a marker");

    // THEN the marker is absent from the prompt, so the check reports it
    assert_eq!(marker, "VALVE:");
    assert!(
        !task.prompt.contains(&marker),
        "the fixture must be an orphan marker for this test to mean anything"
    );
}
