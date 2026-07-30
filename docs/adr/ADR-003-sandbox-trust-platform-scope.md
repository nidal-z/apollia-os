# ADR-003: Sandbox, agent trust model and platform scope

- Status: Accepted
- Date: 2026-06-04

## Context

Two different bodies of code run on the user machine, and they sit at very
different trust levels.

Native tools (`bash_executor`, `python_executor`) run shell and process commands
on the agent's behalf. They must be confined so a tool invocation cannot freely
reach the host. Docker is the standard answer, but it is a heavy dependency (a
daemon and `/var/run/docker.sock`) that violates the zero-dependency principle.

Agent Python code itself is loaded in-process through the PyO3 bridge
([ADR-002](ADR-002-pyo3-bridge-decoupling.md)). It runs with the full rights of
the runtime process: filesystem, network, subprocess spawning through the native
tools, and read access to credentials in the keyring. Confining that code would
require a process-per-agent architecture with cross-process IPC for every tool
call.

The platform target also has to be settled, because primitives such as the Unix
socket API, the notification backend, the local LLM GPU backend, and the POSIX
assumptions in the tool sandboxes are not portable to Windows. The release target
is macOS first, Linux best-effort.

## Decision

We adopt an OS-native sandbox for native tool execution with no Docker, a pure
trust model for agent Python code in v0.1.0, and Windows out of scope for v0.1.0
and v1.0.

### Native tool sandbox: OS-native, no Docker

On Linux, native tool commands run inside PID and mount namespaces through
`unshare` (`unshare --pid --mount --fork /bin/sh -c "<cmd>"`). A `SandboxProfile`
(`ReadOnly`, `FileSystem`, `NetworkRestricted`, `Full`) is declared per tool, but
in v0.1.0 it is declarative metadata only: the executors hardcode
`SandboxProfile::FileSystem` and always emit the same PID plus mount namespace
command regardless of the declared profile. There is no per-profile branching, no
network namespace, no `iptables`, and no `tmpfs` yet; per-profile enforcement
(network namespace, `iptables`) is deferred to a later cycle. `unshare` ships in
`util-linux` on every modern Linux and requires user namespaces (standard since
Linux 3.8). This is the production path. The namespace tests self-skip when the
runner lacks `CAP_SYS_ADMIN`, which is the case on standard CI runners, so the
Linux path is exercised on capable hosts rather than guaranteed by CI.

On macOS, no equivalent native primitive is usable: `sandbox-exec` (SBPL) has
been deprecated since macOS 10.15 and is an undocumented private API. The runtime
therefore runs native tools in a development mode that emits a `tracing::warn!`
on every invocation, stating that no sandbox is active and that production
deployments require Linux. The platform branch is resolved at compile time with
`#[cfg(target_os)]`, so there is zero runtime overhead and the warning is
deliberately visible on each call rather than only at startup.

### Agent Python trust model: trusted user code in v0.1.0

Agent Python code is treated as trusted user code, executed with the rights of
the runtime process, which are the rights of the current user. There is no
process-per-agent isolation, no OS sandbox around the agent, and no WASM
confinement in v0.1.0. The target audience is advanced builders who write or
audit their own agents, so security is procedural: the operator audits an agent
before installing it. The trust is surfaced at two concrete points rather than a
single dedicated install banner. On macOS, the development mode warning states
that tools run without a sandbox and that only trusted agents should be run. At
connector install time, a per-connector consent step states that the connector
runs code on the machine with the same rights as the user. A dedicated,
non-skippable install banner that restates the agent-Python trust model in those
exact terms is not yet implemented. Public-facing wording never implies strong
isolation for agent code.

### Platform scope: Windows out of scope for v0.1.0 and v1.0

> **Superseded by [ADR-049](ADR-049-windows-in-scope-for-v0-1-0.md)
> (2026-07-30).** Windows x86_64 is a supported platform for v0.1.0. The
> reassessment this section anticipated took place: the portability cost measured
> far lower than estimated here, and the llama-server migration removed the GPU
> argument. The trust-model and tool-sandbox decisions in the rest of this ADR
> are unchanged. The paragraph below is kept as the record of what was decided on
> 2026-06-04 and why.

Windows is unsupported and untested; Apollia is not built or shipped for Windows.
Platform-specific code is gated with `cfg(target_os)`, and Unix-only primitives
such as the Unix socket are `cfg(unix)`-gated, while some Windows code paths
exist (for example WMI GPU detection) without being part of a supported target.
The Unix socket API, the `notify-rust` notification backend, the GPU LLM backends
(Metal on macOS, CUDA on Linux), and the POSIX assumptions in the tool sandboxes
have no portable Windows path that fits the schedule. Public documentation, the site, and
the announcement do not mention Windows or imply cross-platform support. Windows
may be reassessed in a later cycle if community demand justifies it, and that
decision would be recorded in a future ADR.

## Alternatives considered

### Docker for tool isolation (rejected)
- Pros: proven isolation.
- Cons: requires a daemon, violates zero external dependency, not viable on
  restricted hosts.

### sandbox-exec / SBPL on macOS (rejected)
- Pros: a native macOS sandbox surface.
- Cons: deprecated and undocumented, removable without notice, not transferable
  to Linux.

### macOS warning only at startup (rejected)
- Pros: less log noise.
- Cons: a developer who misses the startup log forgets there is no sandbox; a
  per-invocation warning is an intentional safety signal.

### Process-per-agent sandbox for agent Python in v0.1.0 (rejected)
- Pros: strong OS isolation of agent code, credentials unreadable from the agent.
- Cons: high implementation cost (IPC, tool-call serialization, child lifecycle,
  portability), added per-call latency, a major divergence from the Tokio actor
  architecture. Deferred to a later cycle.

### WASM runtime for agent Python (rejected)
- Pros: strong isolation by construction.
- Cons: the Python-on-WASM ecosystem is immature, with no host-side PyO3 and no
  viable ML/LLM libraries, conflicting with the minimal-contract freedom to
  import any library.

### Windows support via named pipes, or WSL2 only (rejected)
- Pros: broadens the potential audience.
- Cons: significant engineering across IPC, notifications, tool sandbox, CI, and
  signing for little demand in the v0.1.0 audience; WSL2 degrades the desktop
  experience and gives a false promise of support.

### Chosen: OS-native tool sandbox, pure agent trust model, no Windows
- Pros: zero added dependency, a simple and fast runtime aligned with the actor
  architecture, full focus on macOS and Linux quality, and honest public
  communication.
- Trade-offs: no real tool isolation in macOS development (mitigated by explicit
  warnings); a deliberately installed malicious agent can exfiltrate credentials
  or run arbitrary code with the user's rights, so defense rests on the install
  chain; part of the potential audience (Windows builders) is not addressed.

## Consequences

- Positive: one binary with no third-party tooling; production tool execution on
  Linux is confined by namespaces; agents can use the full Python ecosystem with
  no import restrictions; engineering time stays on macOS and Linux.
- Negative / trade-off: macOS development has no real tool sandbox; the agent
  trust model is procedural, not technical, so the trust surfaces (the macOS
  no-sandbox warning and the per-connector install consent) must stay explicit;
  Windows users have no access at release.
- Watch: hardened Linux kernels that disable unprivileged user namespaces;
  whether the audience broadens beyond advanced builders, which would raise the
  priority of process-per-agent isolation; community demand for Windows.

## Architectural principles

- Principle #1 (Local-first): preserved; everything stays on the user machine
  regardless of the trust model.
- Principle #2 (Zero external dependency): `unshare` is native to Linux, no
  Docker, and Windows-specific stacks are avoided on the target platforms.
- Principle #3 (Minimal contract): reinforced; no additional constraint on agent
  Python (imports, syscalls, FFI stay free).
- Principle #4 (Fail fast): the macOS development mode is explicit on every
  invocation, signalling the absence of a sandbox at each call rather than only at
  startup.
- Principle #7 (Non-negotiable safeguards): on Linux production the tool sandbox
  is always active and not agent-configurable; the `StepBudget`, permission
  engine, and audit trail remain functional safeguards enforced by the runtime
  independently of the OS trust model.

## Related

- [ADR-002](ADR-002-pyo3-bridge-decoupling.md) loads the agent Python code that
  this trust model governs.
