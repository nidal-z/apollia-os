---
sidebar_position: 6
title: The agent trust model
description: An agent is arbitrary Python. What Apollia isolates and what it does not, which boundaries are real today, and what that means for what you run.
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
user can do. This is a deliberate v0.1.0 decision (see [tools and confinement](/architecture/decisions#tools-and-sandbox)): the
audience is builders who write or audit the agents they run.

<!-- claim:tool-sandbox-covers-child-processes-only -->
**Two tools are the confined surface, and only their child process.** When an
agent calls `bash_executor` or `python_executor`, that tool spawns a child
process, and it is the child process, not the agent, that Apollia confines:

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

Every other tool runs unconfined in the runtime process. Filesystem tools are
bounded by a path-prefix check: a canonicalised root they refuse to leave, symlink
escapes included. That root is the workspace in a chat session and **the user's
whole home directory** for an installed agent. Network tools are bounded by an
application-level allowlist. Neither is an OS boundary.

Three consequences worth carrying. A mount namespace without `pivot_root` is not
a filesystem jail: the child sees the same filesystem you do. A path-prefix check
is an application guarantee, not a kernel one, and does not survive a tool that
ignores it. And none of it applies to the agent's own code, which can reach
directly what the tools refuse.

**In this documentation the word sandbox has one meaning: the OS confinement of a
tool's child process.** It never refers to the agent, never to the path root of
the filesystem tools, and never to a disposable test environment.

## What actually holds the line

Because the agent is not sandboxed, the real controls are procedural and
human-in-the-loop, layered as defense in depth.

- **Audit before install.** The operator is responsible for reviewing an agent
  before installing it. The command-line install prints a notice restating that
  the agent will run with full user rights and no sandbox.
- **Human approval (HITL).** In a chat session, file writes, edits, shell and
  Python execution route through an approval wrapper whose default decision is to
  ask: the action surfaces to the operator rather than running silently.
  <!-- claim:hitl-wired-in-chat-path-only -->
  **This wrapper is not placed on an installed agent's dispatcher.** An agent's
  own `ctx.tools` calls meet no human checkpoint, which is consistent with the
  rest of this page: an agent already runs arbitrary Python under your account,
  so a gate on one call path would not contain a hostile one. Treat HITL as
  supervision of the conversational path, not as containment of an agent.
- **Capability declarations.** An agent's manifest declares the tools, secrets,
  data sources, and messaging it intends to use, and the matching `ctx.*`
  interfaces enforce those allowlists by default. This is least-privilege
  ergonomics, not an OS boundary: an unsandboxed agent can ignore `ctx.secrets`
  and read the environment directly. Treat the allowlists as a clarity and
  convenience mechanism, not as containment.
- **Runtime safeguards.** The step budget and the audit trail are enforced by the
  runtime, independently of the OS trust model, and cannot be reconfigured away by
  the agent. Persisted permission rules apply on the chat path, evaluated per
  invocation; code executors are excluded from every blanket authorization and
  only match through a prefix rule restricted to a single simple command.

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

- [Tools and confinement](/architecture/decisions#tools-and-sandbox) states the confinement decision and its rejected
  alternatives.
- [The accountability model](/explanation/accountability-model) covers audit
  and approval in depth.
- [Sovereignty and local-first](/explanation/sovereignty-and-local-first) covers
  the data-residency posture.
