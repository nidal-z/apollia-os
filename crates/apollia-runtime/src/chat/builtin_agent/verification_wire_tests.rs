use super::*;
use apollia_core::StepBudgetConfig;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc as StdArc, Mutex as StdMutex};

/// Mock invoker driven by a fixed response sequence; counts every call.
struct CountingInvoker {
    responses: StdArc<StdMutex<Vec<Result<CheckOutcome, String>>>>,
    call_count: StdArc<AtomicU32>,
}

impl CountingInvoker {
    fn with_sequence(seq: Vec<Result<CheckOutcome, String>>) -> Self {
        Self {
            responses: StdArc::new(StdMutex::new(seq)),
            call_count: StdArc::new(AtomicU32::new(0)),
        }
    }

    fn call_count(&self) -> u32 {
        self.call_count.load(Ordering::SeqCst)
    }
}

impl CheckInvoker for CountingInvoker {
    async fn invoke_check(&self, _command: &str) -> Result<CheckOutcome, String> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let mut seq = self.responses.lock().unwrap_or_else(|e| e.into_inner());
        if seq.is_empty() {
            Ok(CheckOutcome {
                exit_code: 0,
                stderr: String::new(),
            })
        } else {
            seq.remove(0)
        }
    }
}

fn ok_check() -> Result<CheckOutcome, String> {
    Ok(CheckOutcome {
        exit_code: 0,
        stderr: String::new(),
    })
}

fn failed_check() -> Result<CheckOutcome, String> {
    Ok(CheckOutcome {
        exit_code: 1,
        stderr: "echec".into(),
    })
}

// supervised tier, checks pass, no retry.
#[tokio::test]
async fn test_supervised_checks_pass_no_retry() {
    // GIVEN a passing check and a disabled critic at the supervised tier
    let invoker = CountingInvoker::with_sequence(vec![ok_check()]);
    let loop_ = VerificationLoop::new(vec!["cargo test".into()], vec![]);
    let critic = CriticPass::disabled();
    let budget = StepBudget::new(&StepBudgetConfig {
        max_steps: 10,
        max_tool_calls: 20,
        wall_clock_secs: 300,
    });
    let autonomy = AutonomyLevel::Supervised;

    // WHEN running verification with retry
    let (report, ()) = run_verification_with_retry(
        &autonomy,
        Some(&loop_),
        Some(&critic),
        &invoker,
        "objectif",
        "sortie",
        &budget,
        VERIFICATION_MAX_RETRIES,
        (),
        |(), _correction: String| async { (Ok("sortie corrigee".to_string()), ()) },
    )
    .await;

    // THEN it passes on the first run, no retry, one invocation
    let report = report.expect("report attendu pour palier supervised");
    assert!(report.passed);
    assert_eq!(report.retry_iterations, 0);
    assert_eq!(invoker.call_count(), 1);
}

// budget exhausted before retry, report returned cleanly.
#[tokio::test]
async fn test_budget_exhausted_no_retry() {
    // GIVEN a failing check and a budget with no steps left
    let budget = StepBudget::new(&StepBudgetConfig {
        max_steps: 0,
        max_tool_calls: 20,
        wall_clock_secs: 300,
    });
    let invoker = CountingInvoker::with_sequence(vec![failed_check()]);
    let loop_ = VerificationLoop::new(vec!["cargo test".into()], vec![]);
    let critic = CriticPass::disabled();
    let autonomy = AutonomyLevel::Supervised;

    // WHEN running verification with retry
    let (report, ()) = run_verification_with_retry(
        &autonomy,
        Some(&loop_),
        Some(&critic),
        &invoker,
        "objectif",
        "sortie",
        &budget,
        VERIFICATION_MAX_RETRIES,
        (),
        |(), _correction: String| async { (Ok("sortie".to_string()), ()) },
    )
    .await;

    // THEN no retry is attempted and a failing report is returned, not an error
    let report = report.expect("report attendu meme quand budget epuise");
    assert!(!report.passed);
    assert_eq!(report.retry_iterations, 0);
}

// At the assisted tier, declared checks run once, without critic or retries.
#[tokio::test]
async fn test_assisted_runs_declared_checks() {
    // GIVEN the assisted tier with a declared check command
    let invoker = CountingInvoker::with_sequence(vec![ok_check()]);
    let loop_ = VerificationLoop::new(vec!["cargo test".into()], vec![]);
    let critic = CriticPass::disabled();
    let budget = StepBudget::new(&StepBudgetConfig {
        max_steps: 10,
        max_tool_calls: 20,
        wall_clock_secs: 300,
    });
    let autonomy = AutonomyLevel::Assisted;

    // WHEN running verification with retry
    let (report, ()) = run_verification_with_retry(
        &autonomy,
        Some(&loop_),
        Some(&critic),
        &invoker,
        "objectif",
        "sortie",
        &budget,
        VERIFICATION_MAX_RETRIES,
        (),
        |(), _correction: String| async { (Ok("sortie".to_string()), ()) },
    )
    .await;

    // THEN the declared check runs once, with no retries
    let report = report.expect("declared checks run at the assisted tier");
    assert!(report.passed);
    assert_eq!(report.retry_iterations, 0);
    assert_eq!(invoker.call_count(), 1);
}

// At the assisted tier with no declared checks, verification is skipped.
#[tokio::test]
async fn test_assisted_without_checks_skips_verification() {
    // GIVEN the assisted tier and a verification loop with no commands
    let invoker = CountingInvoker::with_sequence(vec![]);
    let loop_ = VerificationLoop::new(vec![], vec![]);
    let critic = CriticPass::disabled();
    let budget = StepBudget::new(&StepBudgetConfig {
        max_steps: 10,
        max_tool_calls: 20,
        wall_clock_secs: 300,
    });
    let autonomy = AutonomyLevel::Assisted;

    // WHEN running verification with retry
    let (report, ()) = run_verification_with_retry(
        &autonomy,
        Some(&loop_),
        Some(&critic),
        &invoker,
        "objectif",
        "sortie",
        &budget,
        VERIFICATION_MAX_RETRIES,
        (),
        |(), _correction: String| async { (Ok("sortie".to_string()), ()) },
    )
    .await;

    // THEN nothing runs and no report is produced
    assert!(report.is_none(), "no declared checks means no verification");
    assert_eq!(invoker.call_count(), 0);
}

// persistent failures stop at the retry bound.
#[tokio::test]
async fn test_max_retries_bounded() {
    // GIVEN checks that always fail and ample budget
    let invoker = CountingInvoker::with_sequence(vec![
        failed_check(),
        failed_check(),
        failed_check(),
        failed_check(),
    ]);
    let loop_ = VerificationLoop::new(vec!["cargo test".into()], vec![]);
    let critic = CriticPass::disabled();
    let budget = StepBudget::new(&StepBudgetConfig {
        max_steps: 50,
        max_tool_calls: 100,
        wall_clock_secs: 300,
    });
    let autonomy = AutonomyLevel::Supervised;

    // WHEN running verification with retry
    let (report, ()) = run_verification_with_retry(
        &autonomy,
        Some(&loop_),
        Some(&critic),
        &invoker,
        "objectif",
        "sortie",
        &budget,
        VERIFICATION_MAX_RETRIES,
        (),
        |(), _correction: String| async { (Ok("sortie".to_string()), ()) },
    )
    .await;

    // THEN exactly max_retries iterations ran (initial + 2 = 3 invocations)
    let report = report.expect("report attendu");
    assert!(!report.passed);
    assert_eq!(report.retry_iterations, VERIFICATION_MAX_RETRIES);
    assert_eq!(invoker.call_count(), VERIFICATION_MAX_RETRIES + 1);
}

// a failure on the first run that the retry resolves.
#[tokio::test]
async fn test_retry_resolves_failure() {
    // GIVEN a check that fails once then passes
    let invoker = CountingInvoker::with_sequence(vec![failed_check(), ok_check()]);
    let loop_ = VerificationLoop::new(vec!["cargo test".into()], vec![]);
    let critic = CriticPass::disabled();
    let budget = StepBudget::new(&StepBudgetConfig {
        max_steps: 50,
        max_tool_calls: 100,
        wall_clock_secs: 300,
    });
    let autonomy = AutonomyLevel::Supervised;
    let retry_calls = StdArc::new(AtomicU32::new(0));
    let retry_calls_inner = StdArc::clone(&retry_calls);

    // WHEN running verification with retry
    let (report, ()) = run_verification_with_retry(
        &autonomy,
        Some(&loop_),
        Some(&critic),
        &invoker,
        "objectif",
        "sortie initiale",
        &budget,
        VERIFICATION_MAX_RETRIES,
        (),
        move |(), _correction: String| {
            let counter = StdArc::clone(&retry_calls_inner);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                (Ok("sortie corrigee".to_string()), ())
            }
        },
    )
    .await;

    // THEN the retry ran once and the final report passes
    let report = report.expect("report attendu");
    assert!(report.passed);
    assert_eq!(report.retry_iterations, 1);
    assert_eq!(retry_calls.load(Ordering::SeqCst), 1);
    assert_eq!(invoker.call_count(), 2);
}

// correction_message embeds both failed checks and critic corrections.
#[test]
fn test_correction_message_contains_failures_and_corrections() {
    // GIVEN one check failure and one critic correction
    let failures = vec![CheckFailure {
        command: "cargo test".into(),
        exit_code: 1,
        stderr: "boom".into(),
    }];
    let corrections = vec![Correction {
        kind: "missing_file".into(),
        description: "fichier absent".into(),
        suggestion: "creer le fichier".into(),
    }];

    // WHEN building the correction message
    let message = correction_message(&failures, &corrections);

    // THEN it carries both pieces and the instruction wrapper
    assert!(message.contains("cargo test"));
    assert!(message.contains("boom"));
    assert!(message.contains("missing_file"));
    assert!(message.contains("creer le fichier"));
    assert!(message.contains("<verification_feedback>"));
    assert!(message.contains("Please address the issues"));
}
