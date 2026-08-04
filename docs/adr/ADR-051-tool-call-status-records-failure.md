# ADR-051: A tool call's persisted status records failure

- Status: Accepted
- Date: 2026-08-04

## Context

`ToolCallStatus` had four values: `Pending`, `Authorized`, `Executed`,
`Refused`. Nothing in it could say that a call ran and failed.

The runtime computed the verdict correctly. `execute_tool_call` flattens a
dispatcher error into the text `tool error: {code}: {message}` while keeping a
`success` boolean, emits that boolean on `ChatToolCallCompleted`, and emits it
again as the status of `ToolOutputCaptured`. Then it wrote
`ToolCallStatus::Executed` into the `ToolCallRecord` unconditionally, two lines
below the boolean.

The persisted record is the only thing the interface reads once a turn
finalizes. So a `python_executor` call that could not spawn its interpreter was
rendered with a green check and "finished without errors", directly above the
text `tool error: spawn_failed: No such file or directory`. During the turn the
same row had been correct, because the live path reads the event: it flipped
from failed to succeeded at the moment the turn ended. This was found by a
manual test campaign on a macOS build, not by any test or gate.

Every tool shared the path, so every tool shared the false report. Two other
sites wrote the same hardcoded `Executed` while holding their own `ok` flag:
the `todo_write` handler and the plan-tool handler.

## Decision

We adopt a fifth value, `ToolCallStatus::Failed`, written through a single
constructor `ToolCallStatus::from_success(bool)` at every site that records a
call that ran.

`Failed` means the call ran and did not succeed: the executor returned an
error, or the tool reported a non-zero exit code. It stays distinct from
`Refused`, which is a human or policy decision taken before anything ran. The
distinction matters to a reader: one is the agent hitting a wall, the other is
the operator holding a line.

Serialization keeps the existing shape (`rename_all = "lowercase"` plus a
title-case alias), so the value is `"failed"` on the wire and a session
persisted before this decision still deserializes unchanged. The desktop maps
`failed` to its error status, which makes the destructive-tinted rows, the
cross, and the `bash_failed` wording reachable for the first time; all of it
already existed and nothing rendered it.

## Alternatives considered

### A parallel boolean on the record (rejected)
- Pros: no new enum value, no mapping to update in the frontend.
- Cons: two fields that can disagree, and a reader has to know which one wins.
  The frontend already switches on `status` alone; a second source of truth is
  how the live path and the persisted path came to disagree in the first place.

### Infer the failure from the output text (rejected)
- Pros: no schema change at all, works on sessions recorded before this ADR.
- Cons: it makes `tool error:` a wire format. A tool whose legitimate output
  happens to contain that prefix would be reported as failed, and a failure
  whose message shape changes would silently go back to reading as success.
  Guessing from prose is what the structured status exists to avoid.

### Chosen: a fifth status value written through one constructor
- Pros: one field, one meaning, one place where the verdict becomes a stored
  value. The three sites that hardcoded `Executed` now cannot disagree.
- Trade-offs: an exhaustive `match` on `ToolCallStatus` in a future consumer
  has one more arm to handle, and a session recorded before this change still
  shows its old failures as executed. Neither is worth a migration.

## Consequences

- Positive: a failed call is reported as failed, live and on reopening.
- Positive: the failure rendering the desktop already carried becomes
  reachable, without new strings or new components.
- Negative / trade-off: sessions recorded before this change keep their
  inaccurate `executed` status. Rewriting history to guess at it would be
  worse than leaving it.
- Watch: any new site building a `ToolCallRecord` for a call that ran must go
  through `from_success`. The claim `failed-tool-call-is-marked-failed` guards
  the constructor having callers, not every site using it.

## Architectural principles

- Principle #8 (human CLI, machine API): the status is the machine-readable
  half of the outcome. A machine-readable field that always says success is
  worse than no field, because it is trusted.

## Related

- [ADR-023](ADR-023-agent-minimal-contract.md) the dispatch contract whose
  errors reach this record.
