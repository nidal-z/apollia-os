---
sidebar_position: 6
title: The agent trust model
description: An agent is a record and holds no code; the Python of a package is trusted code. What Apollia confines, what it decides call by call, and what it leaves to the choice of installing a package.
---

# The agent trust model

An agent is a record: a mission, a model, the tools it is granted, the sources
it may read. It holds no code. The engine built into the runtime runs it, and
every tool call it makes is decided by governance before it runs. The code that
does run with your rights is elsewhere: the Python a package brings, its tools
and its workflows. This page states plainly what confines each of the two, what
does not, and what an operator, especially a regulated one, must assume before
installing a package they did not write.

Overstating isolation would be worse than useless here: a regulated adopter who
believes code is sandboxed when it is not makes decisions on a false premise.
So this page is deliberately conservative about what Apollia guarantees.

## What runs where

Three things sit at different trust levels.

**An agent is data, and its tool calls are decided one by one.** A package
declares an agent as an `[[agents]]` entry of its `apollia.toml`, or the person
creates one in the application. What it can reach is what its sheet grants:
tools, sources, folders. The model chooses the next call, and the runtime, not
the model, decides whether that call runs (see below). An agent has no way to
execute anything outside the tools it was granted.

**The Python of a package is trusted code.** A package may bring tools
(`@tool`) and workflows (`@workflow`). They are loaded in-process through the
PyO3 bridge and execute with the full rights of the runtime process: the
filesystem, the network, process spawning, and whatever credentials the user
account can read. There is no OS sandbox around that code, no process per
package, and no in-language confinement. The `ctx` object a package tool
receives carries no model, no tool proxy and no memory, and a workflow adds
`ctx.llm`, `ctx.agents`, `ctx.workflows` and `ctx.user`; that is the surface the
runtime supports, not a wall. Malicious or buggy package code can do anything
the current user can do. A package with no Python, a manifest and nothing else,
brings no such code.

<!-- claim:tool-sandbox-covers-child-processes-only -->
**Two tools are the confined surface, and only their child process.** When an
agent calls `bash_executor` or `python_executor`, that tool spawns a child
process, and it is the child process that Apollia confines:

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

Every other native tool runs unconfined in the runtime process. Filesystem
tools are bounded by a path-prefix check: a canonicalised root they refuse to
leave, symlink escapes included. That root is **the union of the folders
granted to the agent**, in a conversation as in a task; an agent granted no
folder reaches none. Tools that fetch over HTTP go through an address check
that refuses loopback, private and link-local destinations, on the first URL
and on every redirect. Neither is an OS boundary.

Three consequences worth carrying. A mount namespace without `pivot_root` is not
a filesystem jail: the child sees the same filesystem you do. A path-prefix check
is an application guarantee, not a kernel one, and it binds the tools of the
runtime, not the code of a package. And a code executor granted to an agent
runs what the model wrote, with the confinement of its platform and no more.

**In this documentation the word sandbox has one meaning: the OS confinement of a
tool's child process.** It never refers to an agent, never to the path root of
the filesystem tools, and never to a disposable test environment.

## What actually holds the line

The controls are layered, and they do not all cover the same thing.

- **The preview before an install.** A package is installed from one `.apo`
  file, and whatever brings the file, it is previewed first: what it contains,
  who signed it, what each of its agents asks for. Nothing is written before
  the person accepts. The preview carries notices: the package is not signed,
  the key that signed it is not among the trusted keys, a connection an agent
  needs is missing, an MCP server of the same name already exists. An
  organisation policy can refuse a package without a valid signature
  (`[packages] require_signature`). This is the control that covers package
  code, and it is a human one: reviewing the Python of a package before
  installing it is the operator's responsibility.
- **The governance decision on every tool call.**
  <!-- claim:hitl-wired-in-chat-path-only -->
  Before a tool an agent asked for runs, one decision answers it, in a
  conversation, in a task, in a step of a plan and in an item of a batch: the
  organisation policy and a deny rule refuse it, a tool that only reads runs,
  and anything sensitive (writing, sending outside the machine, reading
  personal data, executing code) runs on a standing consent of that agent for
  that family of action, or asks a person, who answers once, always or refuse.
  A package tool is decided the same way, by the family its author declared.
  What the decision governs is the call; what a package tool does once it runs
  is its own code.
- **Grants.** What a package declares for an agent is a set of defaults. The
  person changes a tool, a source or a folder from the sheet of the agent, and
  an agent reaches only what is granted there.
- **Runtime safeguards.** The step budget and the audit trail are enforced by
  the runtime and cannot be reconfigured away by an agent. Code executors are
  never covered by a standing consent: each execution asks, except a single
  simple command a rule allows in developer mode.
- **The organisation policy.** A signed policy dropped in a system folder
  refuses families of action, tools or connections for every agent of the
  machine, ahead of any consent.

## What an operator must assume

If you install a package that carries Python you did not write or audit, assume
that code can read and exfiltrate anything on the machine your user account can
reach, including credentials. The mitigations are the review before install and
the signature of an author you trust, not a technical wall around the code. An
agent without code is a different case: what it can do is what its tools do,
and each of those calls goes through the decision above. For a regulated
deployment, that means: run Apollia under a user account scoped to only what
the workload needs, require signed packages by policy, audit package code
before installing it, and grant code executors sparingly.

## See also

- [Tools and confinement](/architecture/decisions#tools-and-sandbox) states the confinement decision and its rejected
  alternatives.
- [The permission model](/architecture/decisions#permission-model) states the
  order of the governance decision.
- [The accountability model](/explanation/accountability-model) covers audit
  and approval in depth.
- [Sovereignty and local-first](/explanation/sovereignty-and-local-first) covers
  the data-residency posture.
