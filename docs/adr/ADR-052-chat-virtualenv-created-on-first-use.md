# ADR-052: The chat virtualenv is created on first use

- Status: Accepted
- Date: 2026-08-04

## Context

`python_executor` runs code inside a per-agent virtualenv:
`<venv_base_dir>/<agent_id>/venv/`. `PythonExecutor::new` locates a system
Python 3 and computes the path of the virtualenv interpreter, but deliberately
does not create the virtualenv. Creation belongs to `setup_venv`, documented as
an agent `INITIALIZING` step, because installing packages at execution time is
forbidden by design: an agent's dependency set is declared in its manifest and
resolved before it runs.

That covers an installed agent that declares packages. It covers nothing else.
The supervisor's provisioning returns early when `manifest.packages` is empty,
and a chat session has no manifest at all: its executor is constructed inline
when the chat dispatcher is built, and nothing ever called `setup_venv` for it.
`python_bin` therefore pointed at a file that was never created, and every call
from a chat failed with `spawn_failed: No such file or directory (os error 2)`.
On the test machine `~/.apollia/venvs` did not exist at all.

The chat path did once create it. A lazy `setup_venv` call lived in
`invoke_python`, the hardcoded fast-path invoker replaced when every tool
converged on the shared dispatcher. The replacement carried the constructor
over and dropped the creation, and the orphaned function stayed in the tree
under `#[allow(dead_code)]`, still documenting a behaviour nothing performed.

The dispatcher is rebuilt for every user message, not once per session, so
where creation happens is a latency decision as much as a correctness one.

## Decision

We adopt creation on first execution: `PythonExecutor::run` creates the
virtualenv when it is missing, once per executor, guarded by a
`tokio::sync::OnceCell` so concurrent calls do not race on the same directory.

The call passes an empty package list. The rule that packages are installed
before execution and never during it is unchanged: an agent that declares
packages is still provisioned at `INITIALIZING`, and the execution-time call
never carries any. `setup_venv` was already idempotent, so an executor that was
provisioned up front pays one `Path::exists()`.

Two further points follow from the same defect:

- The virtualenv directory name is sanitized. Chat identifiers have the shape
  `apollia:chat:<uuid>`, and `:` is illegal in a Windows path component, so the
  raw identifier would have made the tool unusable there even once created.
- Chat sessions share one virtualenv (`apollia-chat`) instead of one per
  session. They declare no packages, so they have nothing to isolate from each
  other, and a per-session interpreter tree left roughly 15 MB behind after
  every conversation.

## Alternatives considered

### Create at dispatcher wiring (rejected)
- Pros: the first Python call is as fast as the second; failure surfaces at
  wiring rather than mid-turn.
- Cons: the dispatcher is rebuilt on every message, so this puts a
  `python -m venv` (seconds, `ensurepip`) on the path of every turn, and pays
  it for the overwhelming majority of conversations that never run Python. A
  cost borne by everyone to spare one caller its own latency.

### Provision at daemon start (rejected)
- Pros: paid once, never on a user's turn.
- Cons: the daemon would create an interpreter tree on machines where no chat
  ever runs Python, and it moves a chat-scoped concern into boot. Startup work
  that most installs do not need is how boot times rot.

### Chosen: create on first execution
- Pros: the cost falls on the call that asks for it, once. Nothing is created
  for sessions that never run Python.
- Trade-offs: the first `python_executor` call in a fresh install takes a few
  seconds longer, and the failure mode of a broken host surfaces at that call
  rather than at wiring.

## Consequences

- Positive: `python_executor` works from a chat session, which is where a user
  meets it first.
- Positive: the two failures stay distinguishable. No system Python 3 is
  `PythonUnavailable` at construction, naming how to install one; a virtualenv
  that could not be built is `VenvCreationFailed`, carrying what `python -m
  venv` refused. Neither is reported as a spawn failure any more.
- Negative / trade-off: a few seconds on the first Python call of a fresh
  install, with no progress indication while it runs.
- Watch: a failed creation leaves the guard unset so the next call retries.
  This is deliberate, a full disk or a not-yet-writable directory should not
  poison the executor for the rest of the session, but it also means a
  permanently broken host repeats the attempt on every call.

## Architectural principles

- Principle #2 (zero external dependency): unchanged. The virtualenv is built
  from the operator's own interpreter, with no network access unless packages
  are declared, and none are here.
- Principle #4 (fail fast): partially deviated, and deliberately. The absence
  of a system Python is still detected at construction; the absence of the
  virtualenv is now handled at first use instead of being detected early. The
  scope of the deviation is chat sessions, which have no startup phase to fail
  in.

## Related

- [ADR-049](ADR-049-windows-in-scope-for-v0-1-0.md) why a path component that
  is illegal on Windows is a defect and not a detail.
