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
code executors are never blanket-authorized), and the autonomy tier's step
budget.

<!-- claim:journal-records-the-tool-calls-of-a-run -->
The audit journal records the tool calls of a conversation: the chat loop
announces each call with its run, emits `ToolOutputCaptured` for every tool
result and `LlmResponseCaptured` for every completion, and the journal
subscriber turns them into signed, chained entries. Tool outputs and model
completions from a chat session are therefore persisted verbatim in an
append-only register, readable with `audit show <run>` and checkable with
`audit verify`. A chat session additionally keeps the tool-call record its
assistant message carries, serialized into the `tool_calls_json` column of
`chat_messages`, which is not an audited record.

An agent is granted these tools by name, in its access list: `access` under
`[agents.defaults]` in the [package manifest](/reference/package-manifest),
which the person can change on the sheet of the agent. Each call is dispatched
by the canonical tool name listed below. The Python of a package does not call
them: `ctx.tools` is `None` in a tool and in a workflow.

## List the effective state

`apollia-os tools list` prints every native tool with its enabled state, its
active backend, and its credential status:

```sh
apollia-os tools list
```

Disable or re-enable a tool with `apollia-os tools disable <name>` and
`apollia-os tools enable <name>`. The setting is written to the governance
database. A tool is off when that database disables it or when the `disabled`
list of `[tools]` in `apollia.toml` names it: the agent runners and the
conversation path both read the union of the two, so an agent that invokes a
disabled tool receives an `UnknownTool` error.

On the conversation path, `file_write`, `file_edit`, `notebook_edit`,
`bash_executor`, `python_executor` and `http_fetch` are built apart, rooted at
the folders granted to the agent, and a disabled one is not built at all. The
state is read at each message, so a tool disabled during a conversation is gone
from the next message on, without a restart.

## Availability and credentials

Most tools are always compiled in. Four are gated behind a build feature and are
absent if the runtime is compiled without it. One tool reads an optional
credential.

| Tool | Build feature | Credential |
|---|---|---|
| `http_fetch` | `http` | none |
| `web_search` | `web-search` | `brave.api_key` (optional; falls back to DuckDuckGo) |
| `web_read` | `web-read` | none |
| `memory_search`, `memory_write`, `profile_propose` | `memory-search` | none |
| all other native tools | always compiled | none |

`python_executor` requires a system Python 3 on the host (see the platform
notes below).

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
never used, because command validation encodes POSIX shell semantics.
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

<!-- claim:file-tools-root-is-the-granted-folders -->
The roots are the folders granted to the agent, the built-in chat agent
included, in a conversation as in a task. A folder of the machine is a
connection, and its tools are granted to an agent on its sheet: the reading
tools reach every folder granted, the writing tools only the folders granted
for writing. The first folder granted, by the date it was added, is where a
relative path lands. Neither the home folder, the data directory nor the folder
of a former project is a root. An agent granted no folder has file tools that refuse,
saying how to grant one.

| Tool | Purpose | Key parameters |
|---|---|---|
| `file_read` | Read a file, with optional offset and limit for large files. Returns UTF-8 text with line numbers. A PDF, Word (`.docx`), PowerPoint (`.pptx`) or Excel (`.xlsx`, `.xlsm`, `.xls`, `.ods`) document is read as the text it holds, whole or one page, slide or sheet at a time. | `path`, `offset`, `limit`, `page`, `sheet` |
| `file_write` | Write content to a file, creating intermediate directories and overwriting if it exists. | `path`, `content` |
| `file_list` | List files and directories with type and size, optionally recursive. | `dir`, `recursive` |
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
| `http_fetch` | Perform HTTP GET/POST/PUT/PATCH/DELETE requests. Returns status, headers, and body (capped at 1 MB). On the agent path it fails closed: no code path supplies the host allowlist, so every call is refused with `no_allowlist`. On the chat path a wrapper allowlists the host of the requested URL for that one call, so any public host is reachable once the call is approved; the SSRF guard still refuses private, loopback and link-local addresses. | `url`, `method`, `headers`, `body`, `timeout_secs` |
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
| `memory_write` | Keeps a note in the agent's own memory under a key (`remember`), replacing what the key held, or drops it (`forget`). Only the agent's own namespace is written. | `action`, `key`, `value` |
| `profile_propose` | Proposes one lasting fact about the person for their profile. The call is put to the person every time, with the fact shown; nothing is written before they accept. A field of the profile form is refused. | `key`, `fact` |
| `source_search` | Search of the indexed folders and files checked for the agent, by keywords, and by meaning when an embeddings model is installed. Each result carries its excerpt, document, location, score and source, and the answer says how it was ranked. A source that is not checked is refused; a source whose connection is paused is not searched, and the answer lists it under `paused`. | `query`, `sources`, `limit` (1 to 50) |
| `code_context` | Reads the state of a code folder checked for the agent, at the moment of the call: its git branch, status and recent commits, its rules file (`APOLLIA.md`) and its file tree. Nothing of the folder reaches the prompt otherwise. | `folder`, `parts` (`git`, `rules`, `tree`) |

Each of these tools is offered to an agent only when its source is checked
for it, whatever its access lists: `memory_search` and `memory_write` when its
memory is, `profile_propose` when the profile is, `source_search` when at least
one indexed folder or file is, `code_context` when a code folder is. Checking Memory on the sheet of an agent is what
opens its memory, to read and to write, and neither memory tool asks for
consent: they act on the agent's own space. The memory of an agent is one
space, named after the agent and kept across its conversations and its tasks.

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

No native tool reads or writes the permission rules in the governance
database, so an agent cannot grant itself access. The rules are managed by the
user, from the desktop or with `apollia-os permissions`.

For how permission rules and autonomy tiers shape what runs without asking, see
[Autonomy tiers](/explanation/autonomy-tiers) and
[the accountability model](/explanation/accountability-model).
