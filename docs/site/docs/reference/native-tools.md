---
sidebar_position: 5
title: Native tool catalog
---

# Native tool catalog

The native tools the runtime exposes to agents out of the box. They are wired in
a single place (`crates/apollia-tools/src/native_dispatcher.rs`).

<!-- claim:chat-tool-governance-path -->
On the chat path, every call runs through the same governed route: the human
approval gate with persisted permission rules (name-only allow rules
pre-authorize a tool, argument-prefix rules are evaluated per invocation, and
code executors are never blanket-authorized), the autonomy tier's step
budget, and the audit trail.

An agent reaches these tools through `ctx.tools`, or has them handed to a ReAct
loop via `ctx.tools.describe(<name>)`. Each call is dispatched by the canonical
tool name listed below.

## List the effective state

`apollia-os tools list` prints every native tool with its enabled state, its
active backend, and its credential status:

```sh
apollia-os tools list
```

Disable or re-enable a tool with `apollia-os tools disable <name>` and
`apollia-os tools enable <name>`. A disabled tool is excluded from the dispatcher
entirely, so any agent that invokes it receives an `UnknownTool` error.

## Availability and credentials

Most tools are always compiled in. Four are gated behind a build feature and are
absent if the runtime is compiled without it. One tool reads an optional
credential.

| Tool | Build feature | Credential |
|---|---|---|
| `http_fetch` | `http` | none |
| `web_search` | `web-search` | `brave.api_key` (optional; falls back to DuckDuckGo) |
| `web_read` | `web-read` | none |
| `memory_search` | `memory-search` | none |
| all other native tools | always compiled | none |

The `permission_rule_*` tools additionally require a governance database to be
configured; when it is absent, they are not registered. `python_executor`
requires a system Python 3 on the host (see the platform notes below).

Store the optional Brave key with:

```sh
apollia-os tools credentials set web_search brave.api_key
```

## Code execution

| Tool | Purpose | Key parameters |
|---|---|---|
| `bash_executor` | Execute a shell command. Prefer targeted, fast commands over broad scans. | `command`, `timeout_secs`, `working_dir` |
| `python_executor` | Execute Python code in the agent's per-agent virtualenv (only pre-installed packages are available). | `code`, `timeout_secs` |

### Platform availability

What confines the spawned child process differs per OS; the vocabulary and the
full picture live in [the agent trust model](/explanation/agent-trust-model).

| OS | `bash_executor` | `python_executor` | Child-process confinement |
|---|---|---|---|
| Linux | available, runs via `/bin/sh` | available, needs `python3` or `python` | `unshare` PID + mount namespaces (needs `CAP_SYS_ADMIN`), plus resource limits |
| macOS | available, runs via `/bin/sh` | available, needs `python3` or `python` | resource limits only (CPU, open files), no OS sandbox |
| Windows | requires a POSIX shell on `PATH` (Git Bash, MSYS2 or WSL) | available, needs an installed Python 3 | none |

<!-- claim:bash-executor-requires-posix-shell -->
On Windows, `bash_executor` refuses with an error naming the missing
prerequisite when no POSIX shell is on `PATH`; `cmd.exe` and PowerShell are
never used, because command validation encodes POSIX shell semantics (ADR-049).
One resolved shell both validates and executes every command, on every OS.

<!-- claim:python-executor-locates-windows-interpreter -->
`python_executor` locates the system interpreter per platform: on Windows it
probes `python`, then the `py -3` launcher, then `python3` last, and rejects
the Microsoft Store stub that answers to `python3` on stock installs.

<!-- claim:unavailable-tool-surfaces-reason -->
A code-execution tool that cannot start on this host stays callable and
returns the reason for its unavailability (what is missing, how to install
it) instead of a bare `UnknownTool` error. This holds in a chat session as
well as for an installed agent.

<!-- claim:python-venv-created-on-first-use -->
`python_executor` runs inside a virtualenv, never against the system
interpreter directly. An installed agent gets its own, provisioned from the
packages its manifest declares. A chat session declares none and shares a
single virtualenv, created the first time a chat actually runs Python; the
first such call therefore pays a few seconds, and later ones do not. Two
failures stay distinct: no Python 3 on the host is reported at construction
time and names how to install one, while a virtualenv that could not be
created reports what `python -m venv` refused.

## Filesystem

<!-- claim:tool-sandbox-covers-child-processes-only -->
Every filesystem tool is restricted to the agent's workspace root by a
canonicalised path-prefix check, an application guarantee, not an OS sandbox
(the trust model reserves that word for child-process confinement).

<!-- claim:absolute-paths-resolve-inside-workspace-root -->
Paths may be relative to that root or absolute: an absolute path is accepted
when its canonical form stays under the root, so platform aliases of an
in-root path (macOS `/var` vs `/private/var`, Windows `\\?\` verbatim
prefixes) resolve instead of being refused. Symlink escapes and any path
whose real target leaves the root are rejected.

<!-- claim:chat-file-root-is-home-without-project -->
Which directory is the root depends on the session. With a project open, it is
the project directory. In a chat with no project, it is your home directory: the
assistant is meant to reach the files you actually own, and the barrier on that
path is the approval you are asked for before a write, not a narrower root. The
system temporary directory is used only when the home directory cannot be
resolved at all.

| Tool | Purpose | Key parameters |
|---|---|---|
| `file_read` | Read a file, with optional offset and limit for large files. Returns UTF-8 text with line numbers. | `path`, `offset`, `limit` |
| `file_write` | Write content to a file, creating intermediate directories and overwriting if it exists. | `path`, `content` |
| `file_list` | List files and directories with type and size, optionally recursive. | `path`, `recursive` |
| `file_edit` | Replace exact text in a file. Fails if `old_text` is not found or is not unique (unless `replace_all`). | `path`, `old_text`, `new_text`, `replace_all` |
| `file_glob` | Find files matching a glob pattern (`**` for recursive), sorted by modification time. | `pattern`, `path` |
| `file_grep` | Search file contents for a regex pattern; returns matching lines with path, line number, and optional context. Binary files are skipped. | `pattern`, `path`, `glob`, `context_lines`, `case_insensitive`, `max_results` |

## Notebooks

Jupyter `.ipynb` tools, confined to the same workspace root as the filesystem
tools, nbformat v4 only.

| Tool | Purpose | Key parameters |
|---|---|---|
| `notebook_read` | Read and format the cells of a notebook (type and source) for LLM consumption. | `path` |
| `notebook_edit` | Edit a notebook via atomic cell operations: edit source (outputs cleared), insert, delete, or update metadata. Applied in order. | `path`, `operations` |

## Network

| Tool | Purpose | Key parameters |
|---|---|---|
| `http_fetch` | Perform HTTP GET/POST/PUT/PATCH/DELETE requests. Returns status, headers, and body (capped at 1 MB). Restricted to the agent's host allowlist. | `url`, `method`, `headers`, `body`, `timeout_secs` |
| `web_search` | Search the web and return ranked results (title, URL, snippet). Defaults to DuckDuckGo; uses Brave when a key is configured. | `query`, `max_results` |
| `web_read` | Fetch a public URL and return its extracted readable article text. Rejects private, loopback, and link-local addresses (SSRF guard). HTML and plain text only. | `url`, `max_chars`, `include_metadata` |

`web_search` does not honour the agent network allowlist: enabling it in the
chat tool picker is the user's explicit opt-in to search-engine egress. Content
returned by `web_read` and `web_search` comes from untrusted third-party sites
and is treated as data, not instructions.

## Memory

| Tool | Purpose | Key parameters |
|---|---|---|
| `memory_search` | Full-text search (FTS5, BM25 ranking) over the agent's own namespace and declared shared namespaces. FTS5 operators are escaped automatically. | `query`, `namespace`, `limit`, `source` |

Memory retrieval is always agent-initiated: the runtime never injects memory into
an agent's prompt. The built-in conversational assistant is the exception, and
these tools are not how it does it. See
[the eight principles](/explanation/the-8-principles).

## Interaction

| Tool | Purpose | Key parameters |
|---|---|---|
| `ask_user` | Ask the user one or more questions and wait for their responses. Supports open, single-choice, and multi-choice questions. | `questions`, `context` |

`ask_user` is registered only when the runtime provides a pending-input channel
(interactive chat). Task-mode agents surface an `input_required` result instead.

## Permission governance

Agent-driven management of the permission rules in the governance database. Each
of these is subject to human-in-the-loop approval before it takes effect.

| Tool | Purpose | Key parameters |
|---|---|---|
| `permission_rule_add` | Persist a new permission rule, tagged with the calling agent's identity. `arg_prefix` scopes the rule to arguments starting with that prefix, evaluated on every invocation; for a code executor it only ever covers a single simple command. | `tool_name`, `action` (`allow`/`deny`), `scope` (`global`/`project`/`agent`), `arg_prefix`, `project_path`, `agent_id`, `expires_at` |
| `permission_rule_remove` | Remove a rule by id. | `rule_id` |
| `permission_rule_list` | List rules, optionally filtered. Read-only. | `tool_name`, `created_by`, `scope` |

For how permission rules and autonomy tiers shape what runs without asking, see
[Autonomy tiers](/explanation/autonomy-tiers) and
[the accountability model](/explanation/accountability-model).
