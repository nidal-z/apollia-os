# ADR-044: Agent isolation hardening and posture visibility

- Status: Accepted
- Date: 2026-07-10

> Status moved from Proposed to Accepted on 2026-07-31.
> Delivered: `SecurityPosture` and `ToolSandbox` live in
> `crates/apollia-core/src/security_posture.rs` and are surfaced by the CLI
> and the desktop app; `rlimits.rs` applies the limits on Unix and is an
> explicit no-op elsewhere.

## Context

> **Amendment (2026-08-04).** The "Windows is out of scope" restatement below
> reflects ADR-003 as it stood at the time of writing. The platform-scope
> section of ADR-003 has since been superseded by
> [ADR-049](ADR-049-windows-in-scope-for-v0-1-0.md) (2026-07-30): Windows
> x86_64 is a supported platform for v0.1.0, with no OS-level tool sandbox and
> a POSIX shell required for `bash_executor`. The isolation and posture
> decisions of this ADR stand unchanged.

[ADR-003](ADR-003-sandbox-trust-platform-scope.md) settled the trust model:
agent Python is trusted in-process code, native tools get an OS-native sandbox
with no Docker, macOS runs tools without a sandbox behind an explicit warning,
and Windows is out of scope. That decision stands. This ADR does not reopen it.

Two gaps remained after ADR-003, both of which matter for a regulated adopter who
must understand exactly what they deploy.

First, the posture was under-hardened where hardening was cheap and additive.
Native tool child processes were bounded only by a wall-clock timeout: no CPU,
memory, or file-descriptor limit. The Python executor had no namespace isolation
even on Linux (only the shell did) and, unlike the shell, emitted no warning on
macOS. So the two arbitrary-code tools were the least confined.

Second, the posture was under-communicated and, in places, over-stated. The
`SandboxProfile` enum documented RAM caps, network namespaces, and `tmpfs` mounts
that no code enforces. `docs/agents/SECURITY.md` presented a filesystem sandbox
as the mitigation against a malicious agent, which an in-process agent bypasses
with raw Python, and asserted a sovereignty-profile enforcement path that is not
wired. There was no way for an operator to see, at a glance, what isolation is
active on their platform.

## Decision

We harden the reasonably achievable surfaces in v0.1.0, make the active posture
visible in the CLI and the desktop app, and correct every over-stated claim. We
do not change the trust model or introduce agent tiers.

### Per-process resource limits on tool child processes

Native tool executors (`bash_executor`, `python_executor`) apply POSIX
`setrlimit` limits to every child via a `pre_exec` hook: CPU seconds, address
space (Linux only, where it is honored), and open file descriptors. `RLIMIT_NPROC`
ships disabled because it is enforced per real UID, not per sandbox, and a low
value would interfere with unrelated user processes. Limits survive `execve` and
are inherited across `unshare --fork`, so they reach the shell. Defaults are
fixed for v0.1.0 (operator configuration is a later cycle). This adds `libc` as a
direct dependency of `apollia-tools`, already transitive in the lockfile and the
leanest binding for the syscall.

### Python executor brought to parity with the shell

On Linux the Python interpreter now runs inside PID and mount namespaces via
`unshare`, matching `bash_executor`, and fails closed when `unshare` is
unavailable. On other platforms it emits the same per-invocation development-mode
warning the shell already emits. Python tool code was previously the least
confined arbitrary-code path; this closes that asymmetry.

### Posture made visible

A `SecurityPosture` value in `apollia-core` computes, per platform and runtime
state, the active tool sandbox, whether rlimits are set, whether `unshare` is
usable, and the agent-execution model. It is surfaced in `apollia doctor` as a
check (a warning, never an error, on platforms without an OS sandbox), in
`apollia status --json`, and in the desktop Settings under a security-posture
card. The command-line agent install prints a trust banner restating that agent
code runs with full user rights and no sandbox, the "dedicated install banner"
ADR-003 noted as not yet implemented.

### Documentation corrected to match the code

The `SandboxProfile` doc-comments now state only what is enforced and mark RAM,
network, and `tmpfs` guarantees as not yet enforced. `docs/agents/SECURITY.md`
gains an explicit agent-trust-model section, corrects the malicious-agent threat
row, and stops claiming an unwired sovereignty gate. A public explanation page
states the posture and its limits for adopters.

## Alternatives considered

### Out-of-process, OS-sandboxed agent execution now (rejected)
- Pros: real isolation of agent code, credentials unreadable from the agent.
- Cons: the heavy re-architecture ADR-003 already deferred (IPC, tool-call
  serialization, child lifecycle, per-call latency). Out of scope here; kept on
  the roadmap.

### macOS `sandbox-exec` for tools (rejected)
- Pros: a native macOS sandbox surface.
- Cons: deprecated and undocumented, already rejected in ADR-003. Rlimits give a
  portable, honest partial control instead.

### Enforce per-profile sandboxing (network namespace, `tmpfs`) in this cycle (rejected)
- Pros: would make `SandboxProfile` a real constraint.
- Cons: larger than the additive hardening this ADR targets. Deferred and
  documented as not-yet-enforced rather than pretended.

### Introduce trusted/untrusted agent tiers (rejected)
- Pros: a familiar model.
- Cons: without real OS isolation behind the "untrusted" tier it would be
  security theater. A single honest execution model is safer than a false
  distinction.

### Chosen: additive hardening plus honest, visible posture
- Pros: real, portable resource limits and Linux namespace parity with no trust-
  model change; the active posture is impossible to miss; every over-stated claim
  is corrected; one new dependency that is the OS interface itself.
- Trade-offs: rlimits are per-process, not tree-total, and macOS enforces the
  address-space limit weakly, so they are a partial control, stated as such;
  agent code remains unsandboxed by design.

## Consequences

- Positive: tool child processes are bounded in CPU, memory, and descriptors on
  Unix; the Python executor is no longer the weakest arbitrary-code path; an
  operator can read the active isolation level from the CLI and the app; the
  documentation no longer promises isolation it does not deliver.
- Negative / trade-off: the hardening is partial and platform-dependent, and it
  must keep being described honestly; agent code is still trusted in-process,
  so the install-chain audit and the approval gate remain the real controls.
- Watch: the fixed rlimit defaults may need tuning once real workloads run; the
  Direct-mode tool-call budget bypass (below) remains open.

## Architectural principles

- Principle #1 (Local-first): unchanged; all hardening is local and adds no
  external service.
- Principle #2 (Zero external dependency): the one new dependency is `libc`, the
  platform's own syscall interface, not a third-party runtime.
- Principle #3 (Minimal contract): unchanged; no new constraint on agent Python.
- Principle #4 (Fail fast): the Python executor now fails closed on Linux without
  `unshare`, and warns on every invocation where no sandbox is active.
- Principle #7 (Non-negotiable safeguards): rlimits and the tool sandbox are
  applied by the runtime and are not agent-configurable. One acknowledged gap:
  in Direct mode the per-agent tool-call counter is not reconciled with the
  shared step budget, so `max_tool_calls` is not enforced for Python-driven tool
  calls. Closing it is a focused follow-up (a small budget trait shared between
  `apollia-core` and the tool proxy) tracked in the hardening roadmap.

## Related

- [ADR-002](ADR-002-pyo3-bridge-decoupling.md) loads the agent Python that this
  trust model governs.
- [ADR-003](ADR-003-sandbox-trust-platform-scope.md) sets the trust model this
  ADR hardens and communicates without changing.
