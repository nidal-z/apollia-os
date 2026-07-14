# ADR-039 - Verification and critic on the orchestrated ORIA path

- Status: Accepted
- Date: 2026-07-08

## Context

`apollia-oria` has defined a post-run verification loop for several iterations: `VerificationLoop` (deterministic shell checks) and `CriticPass` (optional, degradable LLM critic), in `crates/apollia-oria/src/verification.rs`. Verified in the code: these types were wired **only on the chat side** (`apollia-runtime/src/chat/manager.rs` and `builtin_agent.rs`). On the orchestrated path (`ORIAEngine::execute_orchestrated_plan`), a run therefore ended **without** verification or critique. Capability 2.8 was in a "scaffolding" state on the orchestrated side.

The chat path serves as the reference: verification there is gated by the autonomy tier (`AutonomyLevelConfig.run_verification`, false for `assisted`, true above), the LLM critic is **off-budget** (it routes directly, never touches the `StepBudget`), and on a fail verdict the chat injects a correction and **restarts** its ReAct loop, bounded and guarded by the budget.

The orchestrated path does not run a message-buffer ReAct loop: it executes a plan via the `ActorLoop`. The question "what does the critic do with a fail verdict in orchestration" (annotate, gate, or replan) had no obvious answer modeled on chat. This is a real architecture decision, raised for arbitration.

Value constraint: Apollia's accountability rests on audit + verify + rollback (cap 4.3). A verification verdict must be **traceable in the signed journal**, not merely logged (chat, for its part, drops its verdict without persisting it: a gap not to be reproduced).

## Decision

On the orchestrated path we adopt a post-run verification **gated by the autonomy tier** which, on a fail verdict, **replans and re-executes** in a bounded way under a shared budget:

- **Activation**: at the end of a completed orchestrated run, if the tier resolves `run_verification = true` (chat parity, via `AutonomyLevelConfig::default_for(tier)`), the engine runs `VerificationLoop` (fed by `manifest.check_commands`) plus `CriticPass` on the final result.
- **Verdict semantics = replan-on-fail**: on a fail verdict, the engine produces structured feedback, calls `Reasoner::plan_with_feedback`, re-executes the `ActorLoop`, and repeats up to `oria_config.verification_max_replans` (default 2, `0` disables replan). The `StepBudget` is created **once** and shared across all iterations: it remains the non-bypassable ceiling of the whole run (principle #7). The LLM critic is **off-budget** (chat parity); plan re-execution is on-budget (the `ActorLoop` increments), and the loop stops on budget exhaustion.
- **Traceability**: each verdict is emitted as `RuntimeEvent::VerificationCompleted` on the EventBus, mapped by the `audit_journal` subscriber under the run's `task_id` (like plan-gate events). The verdict therefore lands in the signed journal.
- **Shell checks**: `VerificationLoop` is built from `manifest.check_commands` but with a no-op invoker (chat parity, which does not run a command). Real guarded shell execution remains a later workstream.

## Alternatives considered

### Annotate and trace only (rejected for this workstream)
**For:** the simplest and safest; no loop risk; strict "observability" parity without changing the execution flow.
**Against:** the engine observes a defect without acting on it. The value "the agent corrects itself" (the differentiator of autonomous ReAct agents vs deterministic pipelines) is not delivered in orchestration. This was the fallback option of the brief, set aside in favor of replan.

### Plan-gate on a fail verdict (rejected)
**For:** puts a human in the loop before re-delivering a doubtful result.
**Against:** the plan-gate already exists before execution; adding another after verification burdens the headless flow (A2A, triggers) and adds value only in supervised interactive mode. Out of scope.

### Chosen: bounded replan-on-fail under a shared budget
**For:** delivers self-correction in orchestration; reuses `plan_with_feedback`, already proven at plan-gate reject; the shared budget guarantees no replan bypasses the ceiling.
**Trade-offs:** one more loop in the engine (complexity, tests); a run may cost several plans before converging (bounded by `verification_max_replans` and the budget).

## Consequences

**Positives:**
- Cap 2.8 goes from scaffolding to wired and proven in orchestration (a verdict produced and emitted on a real run).
- The verdict is traceable in the signed journal, reinforcing the accountability primitive (cap 4.3).
- The orchestrated agent self-corrects in a bounded way, never exceeding the `StepBudget`.

**Negatives / Trade-offs:**
- New public variant `RuntimeEvent::VerificationCompleted` in `apollia-core` (additive, catch-all consumers not broken).
- New field `ORIAConfig.verification_max_replans` (additive, default 2).
- The critic is off-budget: consistent with chat, but a costly critic is not counted against the run budget (the re-execution is).

**Neutral / Watch:**
- The replan rate triggered by verification: if it is high, initial planning is weak.
- Real shell execution of the `check_commands` (no-op invoker today): to be wired under guard in a later workstream.

## Architectural principles

- **Principle #7 - Non-bypassable safeguards**: `StepBudget` created once and shared across all replans; the loop stops on budget exhaustion; the critic does not bypass it (it executes nothing governed, it routes an LLM call like chat).
- **Principle #4 - Fail fast, degradable**: without a critic backend, the pass is skipped (verdict `skipped`), the run does not fail.
- **Audit / accountability moat**: the verdict enters the signed journal via the EventBus.

## Related

- ADR-038 (argument contract for orchestrated steps): the immediately preceding workstream and the STOP -> ADR procedure model.
- ADR-031 (unified plan model): replan reuses `Reasoner::plan_with_feedback`.
- Cartography: `docs/internal/cartography/capability-registry.md` (cap 2.8, cap 4.3).
- Origin: the orchestrated verification / critic workstream.
