---
sidebar_position: 6
title: The agent trust model
---

# The agent trust model

An agent is arbitrary Python code. Apollia runs it in the same process as the
runtime, with the same rights as the person who started Apollia. This page states
plainly what that means, what confines the agent and what does not, and what an
operator, especially a regulated one, must assume before deploying an agent they
did not write.

Overstating isolation would be worse than useless here: a regulated adopter who
believes an agent is sandboxed when it is not makes decisions on a false premise.
So this page is deliberately conservative about what Apollia guarantees.

## What runs where

Two bodies of code sit at different trust levels.

**Agent Python is trusted code.** It is loaded in-process through the PyO3 bridge
and executes with the full rights of the runtime process: the filesystem, the
network, process spawning, and read access to credentials in the keyring. There
is no OS sandbox around the agent itself, no process-per-agent isolation, and no
in-language confinement. A malicious or buggy agent can do anything the current
user can do. This is a deliberate v0.1.0 decision (recorded in ADR-003): the
audience is builders who write or audit the agents they run.

**Native tools are the confined surface.** When an agent calls a tool like the
shell or the Python executor, that tool spawns a child process, and it is the
child process, not the agent, that Apollia confines:

- On Linux, tool commands run inside PID and mount namespaces via `unshare`.
- On macOS, there is no OS sandbox for tools; Apollia emits a warning on every
  tool invocation so the absence is impossible to miss. Production tool isolation
  requires Linux.
- On every Unix, tool child processes carry per-process resource limits applied
  with `setrlimit`: CPU time and open file descriptors everywhere, plus address
  space on Linux (macOS rejects the address-space limit, so Apollia does not set
  it there).
<!-- claim:windows-has-no-tool-sandbox -->
- On Windows there is **no confinement at all**: no namespaces, and no resource
  limits either, because `setrlimit` has no Windows equivalent and the function
  that applies it is empty on non-Unix targets. A tool call on Windows runs with
  the same rights as the application. `bash_executor` additionally needs a POSIX
  shell on `PATH` (Git Bash, WSL or MSYS2) and fails without one.

The distinction matters: the sandbox protects the host from a tool call, not from
the agent's own code.

## What actually holds the line

Because the agent is not sandboxed, the real controls are procedural and
human-in-the-loop, layered as defense in depth.

- **Audit before install.** The operator is responsible for reviewing an agent
  before installing it. The command-line install prints a notice restating that
  the agent will run with full user rights and no sandbox.
- **Human approval (HITL).** Sensitive actions route through a permission engine
  whose default decision is to ask. A file write or an outbound call surfaces an
  approval to the operator rather than running silently. This is the primary gate
  for anything that leaves a mark.
- **Capability declarations.** An agent's manifest declares the tools, secrets,
  data sources, and messaging it intends to use, and the matching `ctx.*`
  interfaces enforce those allowlists by default. This is least-privilege
  ergonomics, not an OS boundary: an unsandboxed agent can ignore `ctx.secrets`
  and read the environment directly. Treat the allowlists as a clarity and
  convenience mechanism, not as containment.
- **Runtime safeguards.** The step budget, the audit trail, and the permission
  engine are enforced by the runtime, independently of the OS trust model, and
  cannot be reconfigured away by the agent.

## What an operator must assume

If you deploy an agent you did not write or audit, assume it can read and exfil
anything on the machine that your user account can reach, including credentials.
The mitigations are the install-chain review and the approval prompts, not a
technical wall around the agent. For a regulated deployment, that means: run
Apollia under a user account scoped to only what the workload needs, keep the
approval gate on for anything sensitive, and audit agents before installing them.

## Where the isolation is heading

The v0.1.0 posture is honest about its limits, and several of them are on the
roadmap rather than shipped:

- Out-of-process, OS-sandboxed execution for untrusted agents.
- Per-profile tool enforcement (network namespace and egress allowlists, a
  writable-scope mount), so the declared sandbox profile becomes a real
  constraint rather than metadata.
- A true filesystem jail for the shell and Python executors.
- Enforcement of the sovereignty profile (`local_only`) as an automatic gate.

Until those land, the trust model above is the whole story. When they do, this
page and the decision record will be updated to match, never ahead of the code.

## See also

- ADR-003 records the sandbox and agent-trust decision and its rejected
  alternatives.
- [The accountability model](/explanation/accountability-model) covers audit,
  approval, and rollback in depth.
- [Sovereignty and local-first](/explanation/sovereignty-and-local-first) covers
  the data-residency posture.
