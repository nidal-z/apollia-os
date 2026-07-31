---
sidebar_position: 5
title: Native tool catalog
---

# Native tool catalog

The native tools the runtime exposes to agents out of the box. They are wired in
a single place (`crates/apollia-tools/src/native_dispatcher.rs`) and every one of
them runs through the same governed path: subject to the permission engine, the
autonomy tier, and the audit trail.

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
configured; when it is absent, they are not registered.

Store the optional Brave key with:

```sh
apollia-os tools credentials set web_search brave.api_key
```

## Code execution

| Tool | Purpose | Key parameters |
|---|---|---|
| `bash_executor` | Execute a shell command. Prefer targeted, fast commands over broad scans. | `command`, `timeout_secs`, `working_dir` |
| `python_executor` | Execute Python code in the agent's per-agent virtualenv (only pre-installed packages are available). | `code`, `timeout_secs` |

Both run under the sandbox and resource limits described in
[the agent trust model](/explanation/agent-trust-model).

## Filesystem

Every filesystem tool is confined to the agent's sandbox root. Paths are
relative to that root; attempts to escape it are rejected.

| Tool | Purpose | Key parameters |
|---|---|---|
| `file_read` | Read a file, with optional offset and limit for large files. Returns UTF-8 text with line numbers. | `path`, `offset`, `limit` |
| `file_write` | Write content to a file, creating intermediate directories and overwriting if it exists. | `path`, `content` |
| `file_list` | List files and directories with type and size, optionally recursive. | `path`, `recursive` |
| `file_edit` | Replace exact text in a file. Fails if `old_text` is not found or is not unique (unless `replace_all`). | `path`, `old_text`, `new_text`, `replace_all` |
| `file_glob` | Find files matching a glob pattern (`**` for recursive), sorted by modification time. | `pattern`, `path` |
| `file_grep` | Search file contents for a regex pattern; returns matching lines with path, line number, and optional context. Binary files are skipped. | `pattern`, `path`, `glob`, `context_lines`, `case_insensitive`, `max_results` |

## Notebooks

Jupyter `.ipynb` tools, sandbox-confined, nbformat v4 only.

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
| `permission_rule_add` | Persist a new permission rule, tagged with the calling agent's identity. | `tool_name`, `action` (`allow`/`deny`), `scope` (`global`/`project`/`agent`), `arg_prefix`, `project_path`, `agent_id`, `expires_at` |
| `permission_rule_remove` | Remove a rule by id. | `rule_id` |
| `permission_rule_list` | List rules, optionally filtered. Read-only. | `tool_name`, `created_by`, `scope` |

For how permission rules and autonomy tiers shape what runs without asking, see
[Autonomy tiers](/explanation/autonomy-tiers) and
[the accountability model](/explanation/accountability-model).
