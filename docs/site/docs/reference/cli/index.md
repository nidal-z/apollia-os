---
sidebar_position: 1
title: CLI reference
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

This document contains the help content for the `apollia-os` command-line program.

**Command Overview:**

* [`apollia-os`↴](#apollia-os)
* [`apollia-os start`↴](#apollia-os-start)
* [`apollia-os stop`↴](#apollia-os-stop)
* [`apollia-os status`↴](#apollia-os-status)
* [`apollia-os run`↴](#apollia-os-run)
* [`apollia-os agent`↴](#apollia-os-agent)
* [`apollia-os agent list`↴](#apollia-os-agent-list)
* [`apollia-os agent start`↴](#apollia-os-agent-start)
* [`apollia-os agent stop`↴](#apollia-os-agent-stop)
* [`apollia-os agent show`↴](#apollia-os-agent-show)
* [`apollia-os agent status`↴](#apollia-os-agent-status)
* [`apollia-os agent messages`↴](#apollia-os-agent-messages)
* [`apollia-os agent install`↴](#apollia-os-agent-install)
* [`apollia-os agent uninstall`↴](#apollia-os-agent-uninstall)
* [`apollia-os agent enable`↴](#apollia-os-agent-enable)
* [`apollia-os agent disable`↴](#apollia-os-agent-disable)
* [`apollia-os agent update`↴](#apollia-os-agent-update)
* [`apollia-os agent create`↴](#apollia-os-agent-create)
* [`apollia-os agent package`↴](#apollia-os-agent-package)
* [`apollia-os agent package list`↴](#apollia-os-agent-package-list)
* [`apollia-os agent package show`↴](#apollia-os-agent-package-show)
* [`apollia-os agent package uninstall`↴](#apollia-os-agent-package-uninstall)
* [`apollia-os agent logs`↴](#apollia-os-agent-logs)
* [`apollia-os agent validate`↴](#apollia-os-agent-validate)
* [`apollia-os agent repair`↴](#apollia-os-agent-repair)
* [`apollia-os a2a`↴](#apollia-os-a2a)
* [`apollia-os a2a skills`↴](#apollia-os-a2a-skills)
* [`apollia-os a2a invoke`↴](#apollia-os-a2a-invoke)
* [`apollia-os task`↴](#apollia-os-task)
* [`apollia-os task list`↴](#apollia-os-task-list)
* [`apollia-os task status`↴](#apollia-os-task-status)
* [`apollia-os task cancel`↴](#apollia-os-task-cancel)
* [`apollia-os task inspect`↴](#apollia-os-task-inspect)
* [`apollia-os task resume`↴](#apollia-os-task-resume)
* [`apollia-os task approvals`↴](#apollia-os-task-approvals)
* [`apollia-os eval`↴](#apollia-os-eval)
* [`apollia-os eval run`↴](#apollia-os-eval-run)
* [`apollia-os eval report`↴](#apollia-os-eval-report)
* [`apollia-os tools`↴](#apollia-os-tools)
* [`apollia-os tools list`↴](#apollia-os-tools-list)
* [`apollia-os tools enable`↴](#apollia-os-tools-enable)
* [`apollia-os tools disable`↴](#apollia-os-tools-disable)
* [`apollia-os tools config`↴](#apollia-os-tools-config)
* [`apollia-os tools config get`↴](#apollia-os-tools-config-get)
* [`apollia-os tools config set`↴](#apollia-os-tools-config-set)
* [`apollia-os tools reload`↴](#apollia-os-tools-reload)
* [`apollia-os tools credentials`↴](#apollia-os-tools-credentials)
* [`apollia-os tools credentials list`↴](#apollia-os-tools-credentials-list)
* [`apollia-os tools credentials set`↴](#apollia-os-tools-credentials-set)
* [`apollia-os tools credentials delete`↴](#apollia-os-tools-credentials-delete)
* [`apollia-os tools credentials test`↴](#apollia-os-tools-credentials-test)
* [`apollia-os tools show`↴](#apollia-os-tools-show)
* [`apollia-os tools approvals`↴](#apollia-os-tools-approvals)
* [`apollia-os tools approvals pending`↴](#apollia-os-tools-approvals-pending)
* [`apollia-os tools approvals resolved`↴](#apollia-os-tools-approvals-resolved)
* [`apollia-os audit`↴](#apollia-os-audit)
* [`apollia-os audit list`↴](#apollia-os-audit-list)
* [`apollia-os audit journal`↴](#apollia-os-audit-journal)
* [`apollia-os audit stats`↴](#apollia-os-audit-stats)
* [`apollia-os audit export`↴](#apollia-os-audit-export)
* [`apollia-os audit verify`↴](#apollia-os-audit-verify)
* [`apollia-os audit anchor`↴](#apollia-os-audit-anchor)
* [`apollia-os audit replay`↴](#apollia-os-audit-replay)
* [`apollia-os audit show`↴](#apollia-os-audit-show)
* [`apollia-os hooks`↴](#apollia-os-hooks)
* [`apollia-os hooks list`↴](#apollia-os-hooks-list)
* [`apollia-os memory`↴](#apollia-os-memory)
* [`apollia-os memory inspect`↴](#apollia-os-memory-inspect)
* [`apollia-os memory list`↴](#apollia-os-memory-list)
* [`apollia-os memory clear`↴](#apollia-os-memory-clear)
* [`apollia-os memory purge`↴](#apollia-os-memory-purge)
* [`apollia-os memory learn-procedure`↴](#apollia-os-memory-learn-procedure)
* [`apollia-os memory export`↴](#apollia-os-memory-export)
* [`apollia-os memory import`↴](#apollia-os-memory-import)
* [`apollia-os memory forget`↴](#apollia-os-memory-forget)
* [`apollia-os memory search`↴](#apollia-os-memory-search)
* [`apollia-os llm`↴](#apollia-os-llm)
* [`apollia-os llm status`↴](#apollia-os-llm-status)
* [`apollia-os llm ping`↴](#apollia-os-llm-ping)
* [`apollia-os llm chat`↴](#apollia-os-llm-chat)
* [`apollia-os llm costs`↴](#apollia-os-llm-costs)
* [`apollia-os llm backends`↴](#apollia-os-llm-backends)
* [`apollia-os llm backends list`↴](#apollia-os-llm-backends-list)
* [`apollia-os llm backends show`↴](#apollia-os-llm-backends-show)
* [`apollia-os llm backends create`↴](#apollia-os-llm-backends-create)
* [`apollia-os llm backends update`↴](#apollia-os-llm-backends-update)
* [`apollia-os llm backends delete`↴](#apollia-os-llm-backends-delete)
* [`apollia-os llm backends set-default`↴](#apollia-os-llm-backends-set-default)
* [`apollia-os llm reload`↴](#apollia-os-llm-reload)
* [`apollia-os llm setup`↴](#apollia-os-llm-setup)
* [`apollia-os model`↴](#apollia-os-model)
* [`apollia-os model list`↴](#apollia-os-model-list)
* [`apollia-os model search`↴](#apollia-os-model-search)
* [`apollia-os model show`↴](#apollia-os-model-show)
* [`apollia-os model hardware`↴](#apollia-os-model-hardware)
* [`apollia-os model delete`↴](#apollia-os-model-delete)
* [`apollia-os trigger`↴](#apollia-os-trigger)
* [`apollia-os trigger list`↴](#apollia-os-trigger-list)
* [`apollia-os trigger status`↴](#apollia-os-trigger-status)
* [`apollia-os trigger fire`↴](#apollia-os-trigger-fire)
* [`apollia-os trigger enable`↴](#apollia-os-trigger-enable)
* [`apollia-os trigger disable`↴](#apollia-os-trigger-disable)
* [`apollia-os trigger logs`↴](#apollia-os-trigger-logs)
* [`apollia-os trigger reload`↴](#apollia-os-trigger-reload)
* [`apollia-os trigger create`↴](#apollia-os-trigger-create)
* [`apollia-os trigger update`↴](#apollia-os-trigger-update)
* [`apollia-os trigger delete`↴](#apollia-os-trigger-delete)
* [`apollia-os notify`↴](#apollia-os-notify)
* [`apollia-os notify test`↴](#apollia-os-notify-test)
* [`apollia-os notify list`↴](#apollia-os-notify-list)
* [`apollia-os notify logs`↴](#apollia-os-notify-logs)
* [`apollia-os notify create`↴](#apollia-os-notify-create)
* [`apollia-os notify update`↴](#apollia-os-notify-update)
* [`apollia-os notify delete`↴](#apollia-os-notify-delete)
* [`apollia-os notify events`↴](#apollia-os-notify-events)
* [`apollia-os notify events get`↴](#apollia-os-notify-events-get)
* [`apollia-os notify events set`↴](#apollia-os-notify-events-set)
* [`apollia-os stt`↴](#apollia-os-stt)
* [`apollia-os stt status`↴](#apollia-os-stt-status)
* [`apollia-os stt transcribe`↴](#apollia-os-stt-transcribe)
* [`apollia-os stt transcriptions`↴](#apollia-os-stt-transcriptions)
* [`apollia-os stt transcriptions list`↴](#apollia-os-stt-transcriptions-list)
* [`apollia-os stt transcriptions delete`↴](#apollia-os-stt-transcriptions-delete)
* [`apollia-os stt model`↴](#apollia-os-stt-model)
* [`apollia-os stt model list`↴](#apollia-os-stt-model-list)
* [`apollia-os stt model download`↴](#apollia-os-stt-model-download)
* [`apollia-os stt config`↴](#apollia-os-stt-config)
* [`apollia-os stt config get`↴](#apollia-os-stt-config-get)
* [`apollia-os stt config update`↴](#apollia-os-stt-config-update)
* [`apollia-os onboard`↴](#apollia-os-onboard)
* [`apollia-os permissions`↴](#apollia-os-permissions)
* [`apollia-os permissions list`↴](#apollia-os-permissions-list)
* [`apollia-os permissions revoke`↴](#apollia-os-permissions-revoke)
* [`apollia-os permissions audit`↴](#apollia-os-permissions-audit)
* [`apollia-os permissions add`↴](#apollia-os-permissions-add)
* [`apollia-os chat`↴](#apollia-os-chat)
* [`apollia-os chat delete`↴](#apollia-os-chat-delete)
* [`apollia-os chat rename`↴](#apollia-os-chat-rename)
* [`apollia-os chat export`↴](#apollia-os-chat-export)
* [`apollia-os chat config`↴](#apollia-os-chat-config)
* [`apollia-os chat config get`↴](#apollia-os-chat-config-get)
* [`apollia-os chat config set`↴](#apollia-os-chat-config-set)
* [`apollia-os chat config reset`↴](#apollia-os-chat-config-reset)
* [`apollia-os chat config permissions`↴](#apollia-os-chat-config-permissions)
* [`apollia-os chat config permissions list`↴](#apollia-os-chat-config-permissions-list)
* [`apollia-os chat config permissions delete`↴](#apollia-os-chat-config-permissions-delete)
* [`apollia-os chat config authorizations`↴](#apollia-os-chat-config-authorizations)
* [`apollia-os chat config authorizations list`↴](#apollia-os-chat-config-authorizations-list)
* [`apollia-os chat config authorizations revoke`↴](#apollia-os-chat-config-authorizations-revoke)
* [`apollia-os mcp`↴](#apollia-os-mcp)
* [`apollia-os mcp list`↴](#apollia-os-mcp-list)
* [`apollia-os mcp set-approval`↴](#apollia-os-mcp-set-approval)
* [`apollia-os mcp list-pending`↴](#apollia-os-mcp-list-pending)
* [`apollia-os mcp revoke-approval`↴](#apollia-os-mcp-revoke-approval)
* [`apollia-os mcp add`↴](#apollia-os-mcp-add)
* [`apollia-os mcp remove`↴](#apollia-os-mcp-remove)
* [`apollia-os mcp show`↴](#apollia-os-mcp-show)
* [`apollia-os mcp test`↴](#apollia-os-mcp-test)
* [`apollia-os mcp restart`↴](#apollia-os-mcp-restart)
* [`apollia-os mcp update`↴](#apollia-os-mcp-update)
* [`apollia-os mcp raw-config`↴](#apollia-os-mcp-raw-config)
* [`apollia-os mcp oauth`↴](#apollia-os-mcp-oauth)
* [`apollia-os mcp oauth login`↴](#apollia-os-mcp-oauth-login)
* [`apollia-os mcp oauth status`↴](#apollia-os-mcp-oauth-status)
* [`apollia-os mcp oauth logout`↴](#apollia-os-mcp-oauth-logout)
* [`apollia-os mcp oauth client-id`↴](#apollia-os-mcp-oauth-client-id)
* [`apollia-os mcp oauth client-id set`↴](#apollia-os-mcp-oauth-client-id-set)
* [`apollia-os mcp oauth client-id clear`↴](#apollia-os-mcp-oauth-client-id-clear)
* [`apollia-os mcp oauth discover`↴](#apollia-os-mcp-oauth-discover)
* [`apollia-os mcp secret`↴](#apollia-os-mcp-secret)
* [`apollia-os mcp secret set`↴](#apollia-os-mcp-secret-set)
* [`apollia-os mcp secret delete`↴](#apollia-os-mcp-secret-delete)
* [`apollia-os mcp server`↴](#apollia-os-mcp-server)
* [`apollia-os update`↴](#apollia-os-update)
* [`apollia-os workspace`↴](#apollia-os-workspace)
* [`apollia-os workspace status`↴](#apollia-os-workspace-status)
* [`apollia-os workspace init`↴](#apollia-os-workspace-init)
* [`apollia-os review`↴](#apollia-os-review)
* [`apollia-os resilience`↴](#apollia-os-resilience)
* [`apollia-os resilience list`↴](#apollia-os-resilience-list)
* [`apollia-os resilience show`↴](#apollia-os-resilience-show)
* [`apollia-os resilience reset`↴](#apollia-os-resilience-reset)
* [`apollia-os plan`↴](#apollia-os-plan)
* [`apollia-os plan cache`↴](#apollia-os-plan-cache)
* [`apollia-os plan cache stats`↴](#apollia-os-plan-cache-stats)
* [`apollia-os plan cache clear`↴](#apollia-os-plan-cache-clear)
* [`apollia-os plan cache evict`↴](#apollia-os-plan-cache-evict)
* [`apollia-os doctor`↴](#apollia-os-doctor)
* [`apollia-os inspect`↴](#apollia-os-inspect)
* [`apollia-os logs`↴](#apollia-os-logs)
* [`apollia-os version`↴](#apollia-os-version)
* [`apollia-os connector`↴](#apollia-os-connector)
* [`apollia-os connector list`↴](#apollia-os-connector-list)
* [`apollia-os connector accounts`↴](#apollia-os-connector-accounts)
* [`apollia-os connector test`↴](#apollia-os-connector-test)
* [`apollia-os connector revoke`↴](#apollia-os-connector-revoke)
* [`apollia-os connector client-id`↴](#apollia-os-connector-client-id)
* [`apollia-os connector client-id list`↴](#apollia-os-connector-client-id-list)
* [`apollia-os connector client-id set`↴](#apollia-os-connector-client-id-set)
* [`apollia-os connector client-secret`↴](#apollia-os-connector-client-secret)
* [`apollia-os connector client-secret set`↴](#apollia-os-connector-client-secret-set)
* [`apollia-os connector api-key`↴](#apollia-os-connector-api-key)
* [`apollia-os connector api-key set`↴](#apollia-os-connector-api-key-set)
* [`apollia-os connector drive`↴](#apollia-os-connector-drive)
* [`apollia-os connector drive folder`↴](#apollia-os-connector-drive-folder)
* [`apollia-os connector drive folder list`↴](#apollia-os-connector-drive-folder-list)
* [`apollia-os connector drive folder set`↴](#apollia-os-connector-drive-folder-set)
* [`apollia-os connector drive folder reset`↴](#apollia-os-connector-drive-folder-reset)
* [`apollia-os connector drive folder picked`↴](#apollia-os-connector-drive-folder-picked)
* [`apollia-os connector drive folder picked list`↴](#apollia-os-connector-drive-folder-picked-list)
* [`apollia-os connector drive folder picked remove`↴](#apollia-os-connector-drive-folder-picked-remove)
* [`apollia-os config`↴](#apollia-os-config)
* [`apollia-os config get`↴](#apollia-os-config-get)
* [`apollia-os config set`↴](#apollia-os-config-set)
* [`apollia-os config validate`↴](#apollia-os-config-validate)
* [`apollia-os config edit`↴](#apollia-os-config-edit)
* [`apollia-os config show`↴](#apollia-os-config-show)
* [`apollia-os config reset`↴](#apollia-os-config-reset)
* [`apollia-os profile`↴](#apollia-os-profile)
* [`apollia-os profile show`↴](#apollia-os-profile-show)
* [`apollia-os profile set`↴](#apollia-os-profile-set)
* [`apollia-os profile forget`↴](#apollia-os-profile-forget)
* [`apollia-os profile reset`↴](#apollia-os-profile-reset)
* [`apollia-os profile schema`↴](#apollia-os-profile-schema)
* [`apollia-os profile export`↴](#apollia-os-profile-export)
* [`apollia-os profile import`↴](#apollia-os-profile-import)
* [`apollia-os project`↴](#apollia-os-project)
* [`apollia-os project list`↴](#apollia-os-project-list)
* [`apollia-os project create`↴](#apollia-os-project-create)
* [`apollia-os project show`↴](#apollia-os-project-show)
* [`apollia-os project update`↴](#apollia-os-project-update)
* [`apollia-os project delete`↴](#apollia-os-project-delete)
* [`apollia-os project agents`↴](#apollia-os-project-agents)
* [`apollia-os project agents list`↴](#apollia-os-project-agents-list)
* [`apollia-os project agents add`↴](#apollia-os-project-agents-add)
* [`apollia-os project agents remove`↴](#apollia-os-project-agents-remove)
* [`apollia-os project templates`↴](#apollia-os-project-templates)
* [`apollia-os project templates list`↴](#apollia-os-project-templates-list)
* [`apollia-os project templates seed-builtins`↴](#apollia-os-project-templates-seed-builtins)
* [`apollia-os project link`↴](#apollia-os-project-link)
* [`apollia-os project chats`↴](#apollia-os-project-chats)
* [`apollia-os trace`↴](#apollia-os-trace)
* [`apollia-os digest`↴](#apollia-os-digest)
* [`apollia-os completions`↴](#apollia-os-completions)
* [`apollia-os guide`↴](#apollia-os-guide)
* [`apollia-os do`↴](#apollia-os-do)
* [`apollia-os explain`↴](#apollia-os-explain)

## `apollia-os`

Apollia OS CLI binary (apollia-os)

**Usage:** `apollia-os [OPTIONS] <COMMAND>`

###### **Subcommands:**

* `start` - Start the runtime in foreground
* `stop` - Stop a running runtime
* `status` - Display runtime and agent status
* `run` - Submit a task to an agent and wait for the result
* `agent` - Agent management (list, start, stop, show, install, uninstall, enable, disable, update, create, package, logs, validate, repair)
* `a2a` - Agent-to-Agent skill discovery and direct invocation
* `task` - Task management (list, status, cancel, inspect, resume, approvals)
* `eval` - Evaluation harness (run a TOML suite against the runtime, report a JSONL)
* `tools` - Native tool governance (list, enable, disable, config, reload, credentials, show, approvals)
* `audit` - Audit trail (list, stats, export, verify, show, replay)
* `hooks` - Lifecycle hooks (list)
* `memory` - Memory management
* `llm` - LLM backend diagnostics (status, ping, chat, costs, backends, reload)
* `model` - Local model file management
* `trigger` - Trigger management (list, status, fire, enable, disable, logs, reload, create, update, delete)
* `notify` - Notification channel management (test, list, logs, create, update, delete, events)
* `stt` - Speech-to-Text management (status, transcribe, transcriptions, model, config)
* `onboard` - Launch onboarding or re-onboarding on a specific topic
* `permissions` - Permission rule management (list, revoke, audit)
* `chat` - Interactive chat REPL + persisted session hygiene (delete, rename, export)
* `mcp` - MCP server management (list, add, remove, show, test, restart, update, raw-config, set-approval, list-pending, revoke-approval, server)
* `update` - Check for and install updates from GitHub Releases
* `workspace` - Workspace inspection and initialization (status, init)
* `review` - Automated code or plan review via the apollia-review agent
* `resilience` - Circuit breaker inspection and reset (list, show, reset)
* `plan` - Plan domain management (cache: stats, clear, evict)
* `doctor` - Diagnose the local Apollia environment (no runtime required)
* `inspect` - Statically inspect a Python agent file (no runtime required)
* `logs` - Tail or follow the runtime log file
* `version` - Print the binary version (use `--json` for machine-readable output)
* `connector` - Native SaaS connector management (list, accounts, test, revoke)
* `config` - Global apollia.toml management (get, set, validate, edit, show)
* `profile` - User profile management (show, set, forget, reset, export, import)
* `project` - Project management (list, create, show, update, delete, agents, templates)
* `trace` - Print the event-sourced trace of a task
* `digest` - Aggregated activity overview (tasks + LLM costs + audit stats)
* `completions` - Generate a shell completion script (bash, zsh, fish, powershell, ...)
* `guide` - Short, task-oriented help by theme (chat, governance, audit, ...)
* `do` - Map a natural-language request to a command (local model), then run it
* `explain` - Explain a command or an error message in plain language (local model)

###### **Options:**

* `--socket <PATH>` - Unix socket path (default: the runtime socket under the data directory).
* `--json` - Output machine-readable JSON instead of human-readable text.

   Accepted at any position: before or after the subcommand and its arguments.
* `-q`, `--quiet` - Suppress all non-essential output (only success/error shown).

   When combined with `--json`, JSON output takes priority.
* `-v`, `--verbose` - Show additional details such as durations and step counts
* `--debug` - Enable internal debug logs and ORIA traces on stderr.

   Equivalent to setting `RUST_LOG=debug`.
* `--no-color` - Disable ANSI color codes even when stdout is a TTY



## `apollia-os start`

Start the runtime in foreground

**Usage:** `apollia-os start [OPTIONS]`

###### **Options:**

* `--port <PORT>` - TCP port to listen on (default: 7771)



## `apollia-os stop`

Stop a running runtime

**Usage:** `apollia-os stop`



## `apollia-os status`

Display runtime and agent status

**Usage:** `apollia-os status`



## `apollia-os run`

Submit a task to an agent and wait for the result.

Use the positional `<INPUT>` for free-text input (react/conversational agents). For worker agents that expect a typed skill payload, use `apollia-os a2a invoke <skill_id> --args '<JSON>'` instead, or pass `--input-json '<JSON>'` here to override the default text wrapping.

**Usage:** `apollia-os run [OPTIONS] <AGENT_ID> [INPUT]`

###### **Arguments:**

* `<AGENT_ID>` - Agent identifier
* `<INPUT>` - Task input text (ignored when `--input-json` is provided)

  Default value: ``

###### **Options:**

* `--input-json <JSON>` - Raw JSON payload that bypasses the `parts:[text]` wrapper.

   Use this when the target agent expects a structured input shape (e.g. an AIPInput with `data` parts, a worker skill envelope, or any custom contract).
* `--stream` - Stream task progress in real-time via SSE
* `--detach` - Submit the task and return immediately without waiting for the result.

   The task ID is printed so it can be tracked with `apollia-os task status <id>`.
* `--alternatives` - Display two alternative plans (conservative vs. exploratory) and choose which one to execute before submitting the task.

   Requires the runtime to support plan alternatives (ORIA engine with LLM).
* `--plan` - Pause after plan generation to review and approve the plan before execution. Prompts to approve, reject (with optional feedback), or quit. Incompatible with --alternatives
* `--allowed-tools <TOOL>` - Restrict this session to the listed tools only (comma-separated).

   When specified, only the named tools can be invoked. All other tools are blocked regardless of the global configuration.
* `--disallowed-tools <TOOL>` - Explicitly block the listed tools for this session (comma-separated).

   Takes priority over `--allowed-tools` when the same tool appears in both.
* `--autonomy <LEVEL>` - Autonomy tier for this run (default: assisted).

   Controls the execution budget, memory injection, and verification. Accepted values: assisted, supervised, bounded_autonomous, long_autonomous.



## `apollia-os agent`

Agent management (list, start, stop, show, install, uninstall, enable, disable, update, create, package, logs, validate, repair)

**Usage:** `apollia-os agent <COMMAND>`

###### **Subcommands:**

* `list` - List all agents (installed and/or runtime)
* `start` - Start (register) a new agent from a Python module path
* `stop` - Stop (shutdown) a running agent
* `show` - Display detailed information about an agent
* `status` - Show a compact runtime-status snapshot for `<agent_id>`
* `messages` - List in-memory A2A messages for `<agent_id>` (oldest-first within window)
* `install` - Install an agent permanently from a local path or a Git URL
* `uninstall` - Uninstall a permanently installed agent
* `enable` - Enable an installed agent (will auto-start on boot)
* `disable` - Disable an installed agent (will not auto-start on boot)
* `update` - Update an installed agent with a new Python module
* `create` - Create a new agent from an SDK template
* `package` - Manage agent packages (multi-agent bundles described by agent.toml)
* `logs` - Display recent log lines from a running agent
* `validate` - Validate an agent manifest without installing or starting the agent
* `repair` - Re-provision an installed agent's per-agent Python venv from its manifest



## `apollia-os agent list`

List all agents (installed and/or runtime)

**Usage:** `apollia-os agent list [OPTIONS]`

###### **Options:**

* `--supports-a2a` - Show only A2A-capable agents with their skill descriptors



## `apollia-os agent start`

Start (register) a new agent from a Python module path

**Usage:** `apollia-os agent start <PATH>`

###### **Arguments:**

* `<PATH>` - Path to the agent Python module



## `apollia-os agent stop`

Stop (shutdown) a running agent

**Usage:** `apollia-os agent stop <AGENT_ID>`

###### **Arguments:**

* `<AGENT_ID>` - Agent identifier



## `apollia-os agent show`

Display detailed information about an agent

**Usage:** `apollia-os agent show <AGENT_ID>`

###### **Arguments:**

* `<AGENT_ID>` - Agent identifier



## `apollia-os agent status`

Show a compact runtime-status snapshot for `<agent_id>`.

Distilled view of `agent show` focused on online / idle / error state. Useful in poll loops where the full info payload is overkill.

**Usage:** `apollia-os agent status <AGENT_ID>`

###### **Arguments:**

* `<AGENT_ID>` - Agent identifier



## `apollia-os agent messages`

List in-memory A2A messages for `<agent_id>` (oldest-first within window)

**Usage:** `apollia-os agent messages [OPTIONS] <AGENT_ID>`

###### **Arguments:**

* `<AGENT_ID>` - Agent identifier (recipient)

###### **Options:**

* `--limit <N>` - Maximum number of messages to display (server-clamped to 100)

  Default value: `20`



## `apollia-os agent install`

Install an agent permanently from a local path or a Git URL.

Accepts a local filesystem path (e.g. `./agents/my-agent.py`) or a Git remote URL (e.g. `https://github.com/user/my-agent.git`).  An optional `#<tag>` suffix pins the clone to a specific tag or branch (e.g. `https://github.com/user/my-agent.git#v1.2.0`).

**Usage:** `apollia-os agent install [OPTIONS] <SOURCE>`

###### **Arguments:**

* `<SOURCE>` - Local path to a Python module or a Git URL (with optional #tag)

###### **Options:**

* `--skip-tests` - Skip the agent test suite (not recommended, reduces validation coverage)



## `apollia-os agent uninstall`

Uninstall a permanently installed agent

**Usage:** `apollia-os agent uninstall [OPTIONS] <NAME>`

###### **Arguments:**

* `<NAME>` - Agent name (as declared in manifest)

###### **Options:**

* `--confirm` - Skip the interactive confirmation prompt



## `apollia-os agent enable`

Enable an installed agent (will auto-start on boot)

**Usage:** `apollia-os agent enable <NAME>`

###### **Arguments:**

* `<NAME>` - Agent name



## `apollia-os agent disable`

Disable an installed agent (will not auto-start on boot)

**Usage:** `apollia-os agent disable <NAME>`

###### **Arguments:**

* `<NAME>` - Agent name



## `apollia-os agent update`

Update an installed agent with a new Python module

**Usage:** `apollia-os agent update <NAME> <PATH>`

###### **Arguments:**

* `<NAME>` - Agent name
* `<PATH>` - Path to the new Python module



## `apollia-os agent create`

Create a new agent from an SDK template

**Usage:** `apollia-os agent create [OPTIONS] <NAME>`

###### **Arguments:**

* `<NAME>` - Agent name in kebab-case (e.g. my-agent)

###### **Options:**

* `--type <TYPE>` - Template type: react, conversational, or orchestrated

  Default value: `react`



## `apollia-os agent package`

Manage agent packages (multi-agent bundles described by agent.toml)

**Usage:** `apollia-os agent package <COMMAND>`

###### **Subcommands:**

* `list` - List all installed agent packages
* `show` - Show details for an installed package
* `uninstall` - Uninstall a package and all its agents and triggers



## `apollia-os agent package list`

List all installed agent packages

**Usage:** `apollia-os agent package list`



## `apollia-os agent package show`

Show details for an installed package

**Usage:** `apollia-os agent package show <NAME>`

###### **Arguments:**

* `<NAME>` - Package name



## `apollia-os agent package uninstall`

Uninstall a package and all its agents and triggers

**Usage:** `apollia-os agent package uninstall [OPTIONS] <NAME>`

###### **Arguments:**

* `<NAME>` - Package name

###### **Options:**

* `--confirm` - Skip the interactive confirmation prompt



## `apollia-os agent logs`

Display recent log lines from a running agent

**Usage:** `apollia-os agent logs [OPTIONS] <AGENT_ID>`

###### **Arguments:**

* `<AGENT_ID>` - Agent identifier (name or UUID)

###### **Options:**

* `--last <N>` - Number of recent log lines to display

  Default value: `50`
* `--follow` - Not implemented: refuses with an error naming `--last` instead



## `apollia-os agent validate`

Validate an agent manifest without installing or starting the agent

**Usage:** `apollia-os agent validate <PATH>`

###### **Arguments:**

* `<PATH>` - Path to the Python agent module



## `apollia-os agent repair`

Re-provision an installed agent's per-agent Python venv from its manifest.

Reads `~/.apollia/agents/packages/<name>/agent.toml` (or the single-file agent's manifest), then re-runs `setup_venv` with the declared `packages` list. Useful when an agent was installed before per-agent venv provisioning landed, or when a venv was deleted by hand.

**Usage:** `apollia-os agent repair <NAME>`

###### **Arguments:**

* `<NAME>` - Installed agent name (as declared in the manifest)



## `apollia-os a2a`

Agent-to-Agent skill discovery and direct invocation.

Worker agents expose typed skills (not free-text prompts). Use these sub-commands to discover skills (`a2a skills`) and invoke one with a structured payload (`a2a invoke <skill_id> --args '<JSON>'`).

**Usage:** `apollia-os a2a <COMMAND>`

###### **Subcommands:**

* `skills` - List every skill exposed by active Worker Agents
* `invoke` - Invoke a Worker skill by `skill_id` with a structured payload



## `apollia-os a2a skills`

List every skill exposed by active Worker Agents.

Output columns: SKILL_ID · AGENT · NAME · DESCRIPTION. Use `--json` for the full schemas and examples.

**Usage:** `apollia-os a2a skills`



## `apollia-os a2a invoke`

Invoke a Worker skill by `skill_id` with a structured payload.

The payload comes from `--args '<JSON>'`, from `--args-file <PATH>` (use `-` for stdin), or defaults to `{}` if neither is supplied. The runtime routes to the worker owning the skill; the caller does not need to know which agent it is.

**Usage:** `apollia-os a2a invoke [OPTIONS] <SKILL_ID>`

###### **Arguments:**

* `<SKILL_ID>` - Skill identifier (e.g. `pdf.read_text`)

###### **Options:**

* `--args <JSON>` - Structured JSON payload (overrides `--args-file`)
* `--args-file <PATH>` - Read the JSON payload from a file (use `-` for stdin)
* `--timeout <SECS>` - Optional invocation timeout in seconds (default: 120)
* `--caller <NAME>` - Caller label surfaced in observability (default: `cli`)



## `apollia-os task`

Task management (list, status, cancel, inspect, resume, approvals)

**Usage:** `apollia-os task <COMMAND>`

###### **Subcommands:**

* `list` - List recent tasks
* `status` - Display the status of a specific task
* `cancel` - Cancel a running task
* `inspect` - Display the full execution plan of an orchestrated task
* `resume` - Approve or reject a task pending HITL approval
* `approvals` - List resolved HITL approvals (accepted or rejected)



## `apollia-os task list`

List recent tasks.

With `--pending-approval`, filters to tasks awaiting HITL approval.

**Usage:** `apollia-os task list [OPTIONS]`

###### **Options:**

* `--pending-approval` - Show only tasks waiting for human approval (status = input_required)



## `apollia-os task status`

Display the status of a specific task

**Usage:** `apollia-os task status <TASK_ID>`

###### **Arguments:**

* `<TASK_ID>` - Task identifier (UUID)



## `apollia-os task cancel`

Cancel a running task

**Usage:** `apollia-os task cancel [OPTIONS] <TASK_ID>`

###### **Arguments:**

* `<TASK_ID>` - Task identifier (UUID)

###### **Options:**

* `--confirm` - Skip the interactive confirmation prompt



## `apollia-os task inspect`

Display the full execution plan of an orchestrated task.

Reads directly from `~/.apollia/plans.db`, no runtime connection required.

**Usage:** `apollia-os task inspect <ID>`

###### **Arguments:**

* `<ID>` - Task identifier (UUID)



## `apollia-os task resume`

Approve or reject a task pending HITL approval.

Exactly one of `--approve` or `--reject` must be supplied.

**Usage:** `apollia-os task resume [OPTIONS] <TASK_ID>`

###### **Arguments:**

* `<TASK_ID>` - Task identifier

###### **Options:**

* `--approve` - Approve the pending task, resumes agent execution
* `--reject` - Reject the pending task, terminates the task with REJECTED status
* `--reason <REASON>` - Human-readable reason for rejection (recommended with `--reject`)



## `apollia-os task approvals`

List resolved HITL approvals (accepted or rejected)

**Usage:** `apollia-os task approvals [OPTIONS]`

###### **Options:**

* `--pending` - Also include pending approvals



## `apollia-os eval`

Evaluation harness (run a TOML suite against the runtime, report a JSONL)

**Usage:** `apollia-os eval <COMMAND>`

###### **Subcommands:**

* `run` - Run an eval suite against the running runtime
* `report` - Re-print a previously written JSONL as a summary table



## `apollia-os eval run`

Run an eval suite against the running runtime

**Usage:** `apollia-os eval run [OPTIONS] <SUITE>`

###### **Arguments:**

* `<SUITE>` - Path to the TOML suite

###### **Options:**

* `--out <OUT>` - Write the per-run JSONL here (default: `<suite>.results.jsonl`)
* `--agent <AGENT>` - Default agent for tasks that do not name one



## `apollia-os eval report`

Re-print a previously written JSONL as a summary table

**Usage:** `apollia-os eval report <JSONL>`

###### **Arguments:**

* `<JSONL>` - Path to the JSONL produced by `eval run`



## `apollia-os tools`

Native tool governance (list, enable, disable, config, reload, credentials, show, approvals)

**Usage:** `apollia-os tools <COMMAND>`

###### **Subcommands:**

* `list` - Show the status of each native tool (active, backend, credentials)
* `enable` - Enable the *name* tool (clears any disabled flag in `governance.db`)
* `disable` - Disable the *name* tool (sets `enabled = FALSE` in `governance.db`)
* `config` - Read or update the `[tools.<name>]` configuration in `apollia.toml`
* `reload` - Reload the governance snapshot and print the effective state
* `credentials` - Manage the encrypted credentials attached to a tool
* `show` - Show the descriptor of a tool registered with the runtime
* `approvals` - Inspect the HITL queue from the tool registry's side



## `apollia-os tools list`

Show the status of each native tool (active, backend, credentials)

**Usage:** `apollia-os tools list`



## `apollia-os tools enable`

Enable the *name* tool (clears any disabled flag in `governance.db`)

**Usage:** `apollia-os tools enable <NAME>`

###### **Arguments:**

* `<NAME>` - Canonical name of the native tool



## `apollia-os tools disable`

Disable the *name* tool (sets `enabled = FALSE` in `governance.db`)

**Usage:** `apollia-os tools disable <NAME>`

###### **Arguments:**

* `<NAME>` - Canonical name of the native tool



## `apollia-os tools config`

Read or update the `[tools.<name>]` configuration in `apollia.toml`

**Usage:** `apollia-os tools config <COMMAND>`

###### **Subcommands:**

* `get` - Show the effective configuration of *name*
* `set` - Update a configuration key `<tool>.<path>` in `apollia.toml`



## `apollia-os tools config get`

Show the effective configuration of *name*

**Usage:** `apollia-os tools config get <NAME>`

###### **Arguments:**

* `<NAME>` - Native tool name (`web_search`, `web_read`, …)



## `apollia-os tools config set`

Update a configuration key `<tool>.<path>` in `apollia.toml`

**Usage:** `apollia-os tools config set <KEY_PATH> <VALUE>`

###### **Arguments:**

* `<KEY_PATH>` - Dotted key path, e.g. `web_search.backend` or `web_search.brave.timeout_secs`
* `<VALUE>` - New value (parsed according to the expected type)



## `apollia-os tools reload`

Reload the governance snapshot and print the effective state

**Usage:** `apollia-os tools reload`



## `apollia-os tools credentials`

Manage the encrypted credentials attached to a tool

**Usage:** `apollia-os tools credentials <COMMAND>`

###### **Subcommands:**

* `list` - List stored credentials (values masked)
* `set` - Store a credential `(tool, key)` after an interactive masked prompt
* `delete` - Delete the credential `(tool, key)`
* `test` - Validate a credential with a live call against the backend it targets



## `apollia-os tools credentials list`

List stored credentials (values masked)

**Usage:** `apollia-os tools credentials list [TOOL]`

###### **Arguments:**

* `<TOOL>` - Optional filter on a tool name



## `apollia-os tools credentials set`

Store a credential `(tool, key)` after an interactive masked prompt

**Usage:** `apollia-os tools credentials set <TOOL> <KEY>`

###### **Arguments:**

* `<TOOL>` - Owning tool name, or `agent` for a secret declared by an agent manifest
* `<KEY>` - Logical key name (e.g. `brave.api_key`, or an agent's `hubspot_api_token`)



## `apollia-os tools credentials delete`

Delete the credential `(tool, key)`

**Usage:** `apollia-os tools credentials delete [OPTIONS] <TOOL> <KEY>`

###### **Arguments:**

* `<TOOL>` - Owning tool name
* `<KEY>` - Logical key name

###### **Options:**

* `--confirm` - Skip the interactive confirmation prompt



## `apollia-os tools credentials test`

Validate a credential with a live call against the backend it targets

**Usage:** `apollia-os tools credentials test <TOOL>`

###### **Arguments:**

* `<TOOL>` - Tool whose credentials should be checked



## `apollia-os tools show`

Show the descriptor of a tool registered with the runtime

**Usage:** `apollia-os tools show <TOOL_NAME>`

###### **Arguments:**

* `<TOOL_NAME>` - Tool name



## `apollia-os tools approvals`

Inspect the HITL queue from the tool registry's side

**Usage:** `apollia-os tools approvals <COMMAND>`

###### **Subcommands:**

* `pending` - List approvals pending decision (tasks in `input_required`)
* `resolved` - List approvals resolved within the `--days` window



## `apollia-os tools approvals pending`

List approvals pending decision (tasks in `input_required`)

**Usage:** `apollia-os tools approvals pending`



## `apollia-os tools approvals resolved`

List approvals resolved within the `--days` window

**Usage:** `apollia-os tools approvals resolved [OPTIONS]`

###### **Options:**

* `--days <DAYS>` - Days of history to include (default: 7)

  Default value: `7`
* `--limit <LIMIT>` - Maximum number of entries to return (default: 50)

  Default value: `50`



## `apollia-os audit`

Audit trail (list, stats, export, verify, show, replay)

**Usage:** `apollia-os audit <COMMAND>`

###### **Subcommands:**

* `list` - List recent audit events (default)
* `journal` - Browse the hash-chained journal across every run, newest first
* `stats` - Display audit statistics
* `export` - Export the audit trail as JSON, up to `--limit` events
* `verify` - Verify the audit journal's hash chains and signatures
* `anchor` - Print the exportable head anchor of the global chain
* `replay` - Replay a captured run and detect divergences
* `show` - Show a run's full journal, including the model's LLM completions



## `apollia-os audit list`

List recent audit events (default)

**Usage:** `apollia-os audit list [OPTIONS]`

###### **Options:**

* `--limit <LIMIT>` - Maximum number of events to display

  Default value: `20`



## `apollia-os audit journal`

Browse the hash-chained journal across every run, newest first.

Unlike `audit list` (the tool-invocation trail) and `audit show RUN` (one run), this reads the chained journal without needing a run id up front, so the audited register is reachable by browsing.

**Usage:** `apollia-os audit journal [OPTIONS]`

###### **Options:**

* `--limit <LIMIT>` - Maximum number of entries to display

  Default value: `20`
* `--offset <OFFSET>` - Number of entries to skip, newest first. Page through with it

  Default value: `0`



## `apollia-os audit stats`

Display audit statistics

**Usage:** `apollia-os audit stats`



## `apollia-os audit export`

Export the audit trail as JSON, up to `--limit` events

**Usage:** `apollia-os audit export [OPTIONS]`

###### **Options:**

* `--output <PATH>` - Destination file (default: stdout)
* `--limit <LIMIT>` - Maximum number of events to include (default: 10000)

  Default value: `10000`



## `apollia-os audit verify`

Verify the audit journal's hash chains and signatures.

With a RUN_ID, verifies that run's per-run chain. Without an argument, verifies the whole journal: the global chain across all runs (detecting interior deletion and whole-run deletion) and the head anchor (detecting truncation of the global tail).

**Usage:** `apollia-os audit verify [RUN_ID]`

###### **Arguments:**

* `<RUN_ID>` - Identifier of the run to verify. Omit to verify the whole journal



## `apollia-os audit anchor`

Print the exportable head anchor of the global chain.

Storing this off-machine is the only defense against truncation of the global tail once the signing key can be compromised.

**Usage:** `apollia-os audit anchor`



## `apollia-os audit replay`

Replay a captured run and detect divergences.

Compares the replayed run against its captured trace across every category: LLM responses, tool outputs, and plan construction. Divergences are grouped by category in the human output and listed in the `--json` payload. `run` accepts a full run_id or an unambiguous prefix of at least 8 characters. Exit 0 = identical, exit 2 = diverged, exit 1 = any error (run not found, ambiguous prefix, incomplete trace, runtime unreachable).

**Usage:** `apollia-os audit replay <RUN>`

###### **Arguments:**

* `<RUN>` - Run identifier or unambiguous prefix



## `apollia-os audit show`

Show a run's full journal, including the model's LLM completions.

Unlike `audit list`/`export` (the tool-only audit trail), this reads the hash-chained journal so the captured reasoning (prompts/responses) is readable. Accepts a run_id or a task_id (resolved to its run_id).

**Usage:** `apollia-os audit show <RUN_OR_TASK>`

###### **Arguments:**

* `<RUN_OR_TASK>` - Run identifier, or a task_id that maps to one



## `apollia-os hooks`

Lifecycle hooks (list)

**Usage:** `apollia-os hooks <COMMAND>`

###### **Subcommands:**

* `list` - List the registered lifecycle hook handlers



## `apollia-os hooks list`

List the registered lifecycle hook handlers

**Usage:** `apollia-os hooks list [OPTIONS]`

###### **Options:**

* `--dry-run` - Read configuration from `apollia.toml` and validate it without connecting to the runtime



## `apollia-os memory`

Memory management

**Usage:** `apollia-os memory <COMMAND>`

###### **Subcommands:**

* `inspect` - Inspect the state of a memory namespace
* `list` - List every memory namespace present on disk
* `clear` - Wipe an agent's memory
* `purge` - Purge memory entries older than a day threshold
* `learn-procedure` - Record a procedure in a namespace's procedural memory
* `export` - Export a namespace's memory to a JSON file
* `import` - Import memory from a JSON file into a namespace
* `forget` - Delete a single memory entry by its UUID
* `search` - Full-text search across a namespace's episodic + semantic memory



## `apollia-os memory inspect`

Inspect the state of a memory namespace

**Usage:** `apollia-os memory inspect [OPTIONS] <NAMESPACE>`

###### **Arguments:**

* `<NAMESPACE>` - Namespace name to inspect

###### **Options:**

* `--data-dir <DIR>` - Memory data directory (default: ~/.apollia/memory/)
* `--json` - JSON output



## `apollia-os memory list`

List every memory namespace present on disk

**Usage:** `apollia-os memory list [OPTIONS]`

###### **Options:**

* `--agent <NAME>` - Filter by agent/namespace name
* `--data-dir <DIR>` - Memory data directory (default: ~/.apollia/memory/)



## `apollia-os memory clear`

Wipe an agent's memory

**Usage:** `apollia-os memory clear [OPTIONS] --agent <NAME>`

###### **Options:**

* `--agent <NAME>` - Namespace/agent name to wipe
* `--type <TYPE>` - Memory type to wipe

  Default value: `all`

  Possible values:
  - `episodic`:
    Episodic memories
  - `semantic`:
    Semantic memories
  - `procedural`:
    Procedural memories
  - `all`:
    All memory types

* `--confirm` - Confirm without an interactive prompt
* `--data-dir <DIR>` - Memory data directory (default: ~/.apollia/memory/)



## `apollia-os memory purge`

Purge memory entries older than a day threshold.

Example: `apollia-os memory purge --namespace my-agent --older-than 30` Filtered: `apollia-os memory purge --namespace my-agent --type episodic --older-than 7`

**Usage:** `apollia-os memory purge [OPTIONS] --namespace <NAME> --older-than <DAYS>`

###### **Options:**

* `--namespace <NAME>` - Target namespace
* `--older-than <DAYS>` - Delete entries created more than N days ago
* `--type <TYPE>` - Restrict the purge to a single type (default: all types)

  Possible values:
  - `episodic`:
    Episodic memories
  - `semantic`:
    Semantic memories
  - `procedural`:
    Procedural memories
  - `all`:
    All memory types

* `--data-dir <DIR>` - Memory data directory (default: ~/.apollia/memory/)
* `--confirm` - Skip the interactive confirmation prompt



## `apollia-os memory learn-procedure`

Record a procedure in a namespace's procedural memory.

Example: `apollia-os memory learn-procedure --namespace agent-x --trigger "analyse a report" --steps "1. Open, 2. Read, 3. Summarise"`

**Usage:** `apollia-os memory learn-procedure [OPTIONS] --namespace <NAME> --trigger <TEXT>`

###### **Options:**

* `--namespace <NAME>` - Target namespace
* `--trigger <TEXT>` - Exact trigger phrase for the procedure
* `--steps <STEPS>` - Procedure steps (comma- or semicolon-separated). Example: "Open the PDF, Extract revenue, Generate summary"
* `--file <FILE>` - JSON file containing {"trigger": "...", "steps": [...]}
* `--data-dir <DIR>` - Memory data directory (default: ~/.apollia/memory/)



## `apollia-os memory export`

Export a namespace's memory to a JSON file.

Example: `apollia-os memory export --namespace agent-x --output ./backup.apollia-memory`

**Usage:** `apollia-os memory export [OPTIONS] --namespace <NAME>`

###### **Options:**

* `--namespace <NAME>` - Namespace to export
* `--output <FILE>` - Output file (default: `<namespace>.apollia-memory` in the current directory)
* `--data-dir <DIR>` - Memory data directory (default: ~/.apollia/memory/)



## `apollia-os memory import`

Import memory from a JSON file into a namespace.

Example: `apollia-os memory import --namespace agent-x --input ./backup.apollia-memory --replace`

**Usage:** `apollia-os memory import [OPTIONS] --namespace <NAME> --input <FILE>`

###### **Options:**

* `--namespace <NAME>` - Target namespace
* `--input <FILE>` - Input file exported by `memory export`
* `--replace` - Mode: replace the existing namespace (default: merge)
* `--merge` - Mode: merge with the existing namespace (default)
* `--data-dir <DIR>` - Memory data directory (default: ~/.apollia/memory/)



## `apollia-os memory forget`

Delete a single memory entry by its UUID.

Searches `episodic_memories`, `semantic_memories`, and `procedural_memories` in order; removes the matching row and its FTS5 index entry. Returns exit 1 when no entry matches.

**Usage:** `apollia-os memory forget [OPTIONS] <NAMESPACE> <ENTRY_ID>`

###### **Arguments:**

* `<NAMESPACE>` - Namespace containing the entry
* `<ENTRY_ID>` - Entry UUID (matches `id` columns across the three tables)

###### **Options:**

* `--data-dir <DIR>` - Memory data directory (default: ~/.apollia/memory/)
* `--confirm` - Skip the interactive confirmation prompt



## `apollia-os memory search`

Full-text search across a namespace's episodic + semantic memory.

Returns BM25-ranked matches with their source table (episodic|semantic), content, and relevance score.

**Usage:** `apollia-os memory search [OPTIONS] <NAMESPACE> <QUERY>`

###### **Arguments:**

* `<NAMESPACE>` - Namespace to search
* `<QUERY>` - FTS5 query (whitespace-separated keywords; quotes preserved)

###### **Options:**

* `--limit <N>` - Maximum number of matches to return

  Default value: `20`
* `--source <SOURCE>` - Restrict to a single source: `episodic` or `semantic`. Omit for both

  Possible values: `episodic`, `semantic`

* `--data-dir <DIR>` - Memory data directory (default: ~/.apollia/memory/)



## `apollia-os llm`

LLM backend diagnostics (status, ping, chat, costs, backends, reload)

**Usage:** `apollia-os llm <COMMAND>`

###### **Subcommands:**

* `status` - Display the status of all configured LLM backends
* `ping` - Measure the latency of a specific LLM backend
* `chat` - Send a direct prompt to an LLM backend and print the response
* `costs` - Display aggregated usage and costs (tokens and estimated cost per backend)
* `backends` - Manage configured LLM backends (list, create, update, delete, set-default)
* `reload` - Reload the LLM router from `system.db` without restarting the runtime
* `setup` - First-run helper: configure a local LLM in one step



## `apollia-os llm status`

Display the status of all configured LLM backends

**Usage:** `apollia-os llm status`



## `apollia-os llm ping`

Measure the latency of a specific LLM backend

**Usage:** `apollia-os llm ping [BACKEND]`

###### **Arguments:**

* `<BACKEND>` - Backend name (default: the router's configured default backend)



## `apollia-os llm chat`

Send a direct prompt to an LLM backend and print the response

**Usage:** `apollia-os llm chat [OPTIONS] <PROMPT>`

###### **Arguments:**

* `<PROMPT>` - The prompt text to send to the LLM

###### **Options:**

* `--backend <BACKEND>` - Backend to use (optional, uses the configured default if omitted)



## `apollia-os llm costs`

Display aggregated usage and costs (tokens and estimated cost per backend).

Without flags, prints the cost table. Use `--get-threshold` to print `[llm] cost_alert_threshold_usd` from `apollia.toml`, or `--threshold N` to set it.

**Usage:** `apollia-os llm costs [OPTIONS]`

###### **Options:**

* `--get-threshold` - Read the cost alert threshold from `apollia.toml` instead of the cost table
* `--threshold <USD>` - Set the cost alert threshold (USD). Writes `[llm] cost_alert_threshold_usd = N` to `apollia.toml`. Pass `0` or a negative value to clear the threshold
* `--config <PATH>` - Optional config file path override (default: `~/.apollia/apollia.toml`)



## `apollia-os llm backends`

Manage configured LLM backends (list, create, update, delete, set-default)

**Usage:** `apollia-os llm backends <COMMAND>`

###### **Subcommands:**

* `list` - List all configured LLM backends
* `show` - Show the full configuration of a backend (including config_json)
* `create` - Create a new LLM backend
* `update` - Update an existing LLM backend
* `delete` - Delete an LLM backend
* `set-default` - Set a backend as the default backend



## `apollia-os llm backends list`

List all configured LLM backends

**Usage:** `apollia-os llm backends list`



## `apollia-os llm backends show`

Show the full configuration of a backend (including config_json)

**Usage:** `apollia-os llm backends show <NAME>`

###### **Arguments:**

* `<NAME>` - Backend name



## `apollia-os llm backends create`

Create a new LLM backend.

`--provider` drives the shape of the `config_json` sent to the runtime. For `llama-cpp` (local GGUF models), `--model` must be the absolute path to the .gguf file. For cloud providers, `--model` is the identifier (e.g. `claude-sonnet-4-6`, `gpt-4o`).

**Usage:** `apollia-os llm backends create [OPTIONS] --provider <PROVIDER> --model <MODEL> <NAME>`

###### **Arguments:**

* `<NAME>` - Unique backend name (snake_case or kebab-case)

###### **Options:**

* `--provider <PROVIDER>` - Provider: `llama-cpp` (local GGUF), `anthropic`, `openai`, `mistral`, `ollama`. `--kind` is accepted as an alias for backward compatibility
* `--model <MODEL>` - Model identifier or path (absolute path for `llama-cpp`)
* `--api-key <KEY>` - API key (cloud providers only). Stored as-is in `config_json.api_key`. Prefer `--api-key-env VAR_NAME` to avoid persisting the key in system.db
* `--api-key-env <VAR_NAME>` - Environment variable name holding the API key.

   The runtime reads `std::env::var(NAME)` at boot. Recommended to keep the key out of system.db.
* `--base-url <URL>` - Base URL (Ollama, self-hosted OpenAI-compatible gateway, ...)
* `--device <DEVICE>` - Device for `llama-cpp` models: `metal` (Apple), `cuda`, `cpu`

  Default value: `metal`
* `--timeout-sec <SECS>` - How long the backend may stay silent before the call is abandoned.

   This is a backstop against a wedged backend, not a latency policy. On the non-streaming path a server sends nothing until generation is complete, so this budget has to cover the slowest honest answer: a large model on modest hardware legitimately takes minutes. Values below 60 seconds are raised to 60.

  Default value: `600`
* `--context-window <TOKENS>` - Usable context window of this backend, in tokens.

   Sizes conversation compaction. A self-hosted OpenAI-compatible server does not report its window, and Ollama sizes its own from the machine's memory, so without this the runtime falls back to a generic limit that can exceed what the server actually loaded. Ollama backends are probed automatically when the model is loaded; set this to pin the value.
* `--disabled` - Create the backend disabled
* `--default` - Mark this backend as the default (only one at a time)



## `apollia-os llm backends update`

Update an existing LLM backend.

Works in merge mode: flags that are not supplied keep their current value. The runtime exposes `PUT` as replace, so the CLI fetches the existing config first and only overwrites the fields the operator changed.

**Usage:** `apollia-os llm backends update [OPTIONS] <NAME>`

###### **Arguments:**

* `<NAME>` - Backend name to update

###### **Options:**

* `--provider <PROVIDER>` - New provider (rarely useful, changes the backend implementation)
* `--model <MODEL>` - New model (absolute path for `llama-cpp`)
* `--api-key <KEY>` - New API key (cloud providers)
* `--api-key-env <VAR_NAME>` - New environment variable name for the API key
* `--base-url <URL>` - New base URL
* `--device <DEVICE>` - New device for `llama-cpp`
* `--timeout-sec <SECS>` - New inference timeout in seconds
* `--enable` - Enable the backend
* `--disable` - Disable the backend
* `--default` - Mark as default



## `apollia-os llm backends delete`

Delete an LLM backend

**Usage:** `apollia-os llm backends delete [OPTIONS] <NAME>`

###### **Arguments:**

* `<NAME>` - Backend name to delete

###### **Options:**

* `--confirm` - Confirm deletion without an interactive prompt



## `apollia-os llm backends set-default`

Set a backend as the default backend

**Usage:** `apollia-os llm backends set-default <NAME>`

###### **Arguments:**

* `<NAME>` - Backend name to mark as default



## `apollia-os llm reload`

Reload the LLM router from `system.db` without restarting the runtime.

`backends create/update/delete/set-default` write to the database but the in-memory router stays frozen until a reload. This command swaps the active router in place without interrupting running tasks.

**Usage:** `apollia-os llm reload`



## `apollia-os llm setup`

First-run helper: configure a local LLM in one step.

`--local` is the only mode supported today. It expects a `.gguf` model path, copies it into `~/.apollia/models/` (no copy when already there), and creates a backend named `local` with `provider=llama-cpp` and `is_default=true` in `system.db`. The runtime picks the new default on the next `llm reload` (or daemon restart).

**Usage:** `apollia-os llm setup [OPTIONS] --model <PATH>`

###### **Options:**

* `--local` - Use the local llama-cpp backend (required for v0.1.0; a cloud provider is declared with `llm backends create --api-key`)
* `--model <PATH>` - Path to the `.gguf` model file
* `--name <NAME>` - Backend name (default: `local`). Overwrites the existing entry of the same name

  Default value: `local`
* `--device <DEVICE>` - Device hint for llama-cpp: `metal` (macOS default), `cuda`, `cpu`. When omitted, picks `metal` on macOS and `cpu` elsewhere
* `--system-db <PATH>` - Override the system database path (default: `~/.apollia/system.db`)
* `--models-dir <DIR>` - Override the models directory (default: `~/.apollia/models/`)



## `apollia-os model`

Local model file management

**Usage:** `apollia-os model <COMMAND>`

###### **Subcommands:**

* `list` - List available .gguf model files in ~/.apollia/models/
* `search` - Search the HuggingFace registry through the runtime
* `show` - Fetch metadata + file list for a HuggingFace model
* `hardware` - Report the runtime's detected hardware profile (RAM, CPU, GPU)
* `delete` - Remove a local model file from `~/.apollia/models/`



## `apollia-os model list`

List available .gguf model files in ~/.apollia/models/

**Usage:** `apollia-os model list`



## `apollia-os model search`

Search the HuggingFace registry through the runtime

**Usage:** `apollia-os model search [OPTIONS] <QUERY>`

###### **Arguments:**

* `<QUERY>` - Free-text query (matches model id, description, tags)

###### **Options:**

* `--limit <LIMIT>` - Maximum number of hits to return (default: 20)

  Default value: `20`



## `apollia-os model show`

Fetch metadata + file list for a HuggingFace model

**Usage:** `apollia-os model show <REPO>`

###### **Arguments:**

* `<REPO>` - Repository identifier in `org/repo` form



## `apollia-os model hardware`

Report the runtime's detected hardware profile (RAM, CPU, GPU)

**Usage:** `apollia-os model hardware`



## `apollia-os model delete`

Remove a local model file from `~/.apollia/models/`

**Usage:** `apollia-os model delete [OPTIONS] <NAME>`

###### **Arguments:**

* `<NAME>` - File name relative to the models directory

###### **Options:**

* `--confirm` - Skip the confirmation prompt



## `apollia-os trigger`

Trigger management (list, status, fire, enable, disable, logs, reload, create, update, delete)

**Usage:** `apollia-os trigger <COMMAND>`

###### **Subcommands:**

* `list` - List all triggers with their status
* `status` - Show the detailed status of a trigger
* `fire` - Fire a trigger immediately (debug/test)
* `enable` - Enable a disabled trigger
* `disable` - Disable a trigger without editing apollia.toml
* `logs` - Show the firing history from SQLite
* `reload` - Reload trigger config from apollia.toml (hot reload)
* `create` - Create a new trigger (CRUD, complements hot-reload via apollia.toml)
* `update` - Update an existing trigger
* `delete` - Delete a trigger



## `apollia-os trigger list`

List all triggers with their status

**Usage:** `apollia-os trigger list`



## `apollia-os trigger status`

Show the detailed status of a trigger

**Usage:** `apollia-os trigger status <ID>`

###### **Arguments:**

* `<ID>` - Trigger identifier



## `apollia-os trigger fire`

Fire a trigger immediately (debug/test)

**Usage:** `apollia-os trigger fire <ID>`

###### **Arguments:**

* `<ID>` - Trigger identifier



## `apollia-os trigger enable`

Enable a disabled trigger

**Usage:** `apollia-os trigger enable <ID>`

###### **Arguments:**

* `<ID>` - Trigger identifier



## `apollia-os trigger disable`

Disable a trigger without editing apollia.toml

**Usage:** `apollia-os trigger disable <ID>`

###### **Arguments:**

* `<ID>` - Trigger identifier



## `apollia-os trigger logs`

Show the firing history from SQLite

**Usage:** `apollia-os trigger logs [OPTIONS] <ID>`

###### **Arguments:**

* `<ID>` - Trigger identifier

###### **Options:**

* `--last <LAST>` - Maximum number of entries to display

  Default value: `20`



## `apollia-os trigger reload`

Reload trigger config from apollia.toml (hot reload).

Rereads `[[triggers]]` from `apollia.toml`, validates the new definitions, and restarts modified sources. Invalid TOML or invalid trigger configuration returns an error without interrupting the currently-running triggers.

**Usage:** `apollia-os trigger reload`



## `apollia-os trigger create`

Create a new trigger (CRUD, complements hot-reload via apollia.toml)

**Usage:** `apollia-os trigger create [OPTIONS] --agent <AGENT> --kind <TYPE> <ID>`

###### **Arguments:**

* `<ID>` - Unique trigger identifier

###### **Options:**

* `--agent <AGENT>` - Target agent
* `--kind <TYPE>` - Source type: cron, interval, oneshot, filewatch, webhook
* `--detail <DETAIL>` - Source-specific detail: cron      → cron expression (e.g. `"0 9 * * 1"`) interval  → duration string (`30m`, `1h`, `6h`, `1d`) oneshot   → RFC 3339 timestamp filewatch → path to a file or directory webhook   → shared HMAC-SHA256 secret of at least 32 chars
* `--on-busy <ON_BUSY>` - Policy when the agent is busy when a fire arrives. `queue` enqueues the fire (default), `drop` discards it

  Default value: `queue`

  Possible values: `queue`, `drop`

* `--input <INPUT>` - Input template sent to the agent when fired



## `apollia-os trigger update`

Update an existing trigger

**Usage:** `apollia-os trigger update [OPTIONS] <ID>`

###### **Arguments:**

* `<ID>` - Trigger identifier

###### **Options:**

* `--detail <DETAIL>` - New source detail (kind is read from the existing definition)
* `--on-busy <ON_BUSY>` - New on-busy policy (`queue` or `drop`)

  Possible values: `queue`, `drop`

* `--input <INPUT>` - New input template



## `apollia-os trigger delete`

Delete a trigger

**Usage:** `apollia-os trigger delete [OPTIONS] <ID>`

###### **Arguments:**

* `<ID>` - Trigger identifier

###### **Options:**

* `--confirm` - Confirm deletion without an interactive prompt



## `apollia-os notify`

Notification channel management (test, list, logs, create, update, delete, events)

**Usage:** `apollia-os notify <COMMAND>`

###### **Subcommands:**

* `test` - Send a test notification to every active channel
* `list` - List configured notification channels with their status
* `logs` - Show the recent notification history from SQLite
* `create` - Create a new notification channel
* `update` - Update an existing notification channel
* `delete` - Delete a notification channel
* `events` - Show or modify the event types that trigger notifications



## `apollia-os notify test`

Send a test notification to every active channel.

Asks the runtime to dispatch a test payload to each channel enabled in `apollia.toml`. Exits 0 if every active channel succeeds, 1 if any channel returns an error.

**Usage:** `apollia-os notify test`



## `apollia-os notify list`

List configured notification channels with their status.

Shows the identifier, type, accepted events and state (enabled / disabled) for each channel declared in `apollia.toml`.

**Usage:** `apollia-os notify list`



## `apollia-os notify logs`

Show the recent notification history from SQLite.

Reads the `notification_logs` table in `~/.apollia/hitl.db`. Returns an empty list if the table does not exist yet.

**Usage:** `apollia-os notify logs [OPTIONS]`

###### **Options:**

* `--last <LAST>` - Number of lines to display (default: 20)

  Default value: `20`



## `apollia-os notify create`

Create a new notification channel

**Usage:** `apollia-os notify create [OPTIONS] --kind <TYPE>`

###### **Options:**

* `--kind <TYPE>` - Channel type: `desktop` or `webhook`

  Possible values: `desktop`, `webhook`

* `--url <URL>` - Target URL (required for `webhook`)
* `--id <ID>` - Channel identifier. Auto-generated as `<kind>-<timestamp>` when omitted (operator-friendly default for ad-hoc creation)
* `--label <TEXT>` - Human-readable label shown in the UI. Defaults to the id
* `--disabled` - Create the channel disabled (default: enabled)



## `apollia-os notify update`

Update an existing notification channel

**Usage:** `apollia-os notify update [OPTIONS] <ID>`

###### **Arguments:**

* `<ID>` - Channel identifier

###### **Options:**

* `--url <URL>` - New URL (for webhook)
* `--enabled <ENABLED>` - Enable or disable

  Possible values: `true`, `false`




## `apollia-os notify delete`

Delete a notification channel

**Usage:** `apollia-os notify delete [OPTIONS] <ID>`

###### **Arguments:**

* `<ID>` - Channel identifier

###### **Options:**

* `--confirm` - Confirm without an interactive prompt



## `apollia-os notify events`

Show or modify the event types that trigger notifications

**Usage:** `apollia-os notify events <COMMAND>`

###### **Subcommands:**

* `get` - Show the configured event types
* `set` - Set the active event types (comma-separated list)



## `apollia-os notify events get`

Show the configured event types

**Usage:** `apollia-os notify events get`



## `apollia-os notify events set`

Set the active event types (comma-separated list)

**Usage:** `apollia-os notify events set [EVENT]...`

###### **Arguments:**

* `<EVENT>` - Enabled event types (e.g. task_completed,task_failed,agent_error)



## `apollia-os stt`

Speech-to-Text management (status, transcribe, transcriptions, model, config)

**Usage:** `apollia-os stt <COMMAND>`

###### **Subcommands:**

* `status` - Display the STT engine status (enabled, model, backend, acceleration)
* `transcribe` - Transcribe an audio file and print the resulting text
* `transcriptions` - List recent transcriptions from the local database
* `model` - Manage local STT model files
* `config` - Manage STT configuration (backend, model, language)



## `apollia-os stt status`

Display the STT engine status (enabled, model, backend, acceleration)

**Usage:** `apollia-os stt status`



## `apollia-os stt transcribe`

Transcribe an audio file and print the resulting text

**Usage:** `apollia-os stt transcribe [OPTIONS] <FILE>`

###### **Arguments:**

* `<FILE>` - Path to the audio file (WAV format)

###### **Options:**

* `--output <PATH>` - Save the full TranscriptResult to a JSON file instead of stdout



## `apollia-os stt transcriptions`

List recent transcriptions from the local database

**Usage:** `apollia-os stt transcriptions <COMMAND>`

###### **Subcommands:**

* `list` - List recent transcriptions
* `delete` - Delete a transcription by its ID



## `apollia-os stt transcriptions list`

List recent transcriptions

**Usage:** `apollia-os stt transcriptions list [OPTIONS]`

###### **Options:**

* `--limit <LIMIT>` - Maximum number of entries to display

  Default value: `20`



## `apollia-os stt transcriptions delete`

Delete a transcription by its ID

**Usage:** `apollia-os stt transcriptions delete [OPTIONS] <ID>`

###### **Arguments:**

* `<ID>` - Transcription identifier

###### **Options:**

* `--confirm` - Skip the interactive confirmation prompt



## `apollia-os stt model`

Manage local STT model files

**Usage:** `apollia-os stt model <COMMAND>`

###### **Subcommands:**

* `list` - List `.bin` model files in ~/.apollia/models/
* `download` - Download a model from HuggingFace



## `apollia-os stt model list`

List `.bin` model files in ~/.apollia/models/

**Usage:** `apollia-os stt model list`



## `apollia-os stt model download`

Download a model from HuggingFace

**Usage:** `apollia-os stt model download <NAME>`

###### **Arguments:**

* `<NAME>` - Model name (e.g. `whisper-large-v3-fr-q5_0`)



## `apollia-os stt config`

Manage STT configuration (backend, model, language)

**Usage:** `apollia-os stt config <COMMAND>`

###### **Subcommands:**

* `get` - Show the current STT configuration
* `update` - Update the STT configuration



## `apollia-os stt config get`

Show the current STT configuration

**Usage:** `apollia-os stt config get`



## `apollia-os stt config update`

Update the STT configuration

**Usage:** `apollia-os stt config update [OPTIONS]`

###### **Options:**

* `--backend <BACKEND>` - Backend to use (whisper, disabled)
* `--model-path <MODEL_PATH>` - Path to the Whisper model
* `--language <LANGUAGE>` - Language (fr, en, auto)



## `apollia-os onboard`

Launch onboarding or re-onboarding on a specific topic

**Usage:** `apollia-os onboard [OPTIONS]`

###### **Options:**

* `--topic <TOPIC>` - Focus on a specific topic (identity, preferences, tools, domain, agents)



## `apollia-os permissions`

Permission rule management (list, revoke, audit)

**Usage:** `apollia-os permissions <COMMAND>`

###### **Subcommands:**

* `list` - List persisted rules (project + global)
* `revoke` - Revoke a rule by identifier, or every matching rule with `--all`
* `audit` - Show the permission decision history
* `add` - Add a persisted permission rule for a tool + optional argument prefix



## `apollia-os permissions list`

List persisted rules (project + global)

**Usage:** `apollia-os permissions list [OPTIONS]`

###### **Options:**

* `--scope <SCOPE>` - Filter by scope: `global`, `project` or `session`
* `--tool <TOOL>` - Filter by tool name



## `apollia-os permissions revoke`

Revoke a rule by identifier, or every matching rule with `--all`

**Usage:** `apollia-os permissions revoke [OPTIONS] [ID]`

###### **Arguments:**

* `<ID>` - Numeric identifier of a persisted rule.

   IDs prefixed with `s` denote session-scoped rules; they are not revocable from the CLI (use the desktop app or restart the daemon).

###### **Options:**

* `--all` - Revoke every rule matching `--scope`
* `--scope <SCOPE>` - Scope targeted by `--all`: `global` (default) or `project`
* `--yes` - Skip the interactive confirmation (useful for scripts)



## `apollia-os permissions audit`

Show the permission decision history

**Usage:** `apollia-os permissions audit [OPTIONS]`

###### **Options:**

* `--tool <TOOL>` - Filter by tool name
* `--limit <LIMIT>` - Maximum number of entries to display

  Default value: `50`



## `apollia-os permissions add`

Add a persisted permission rule for a tool + optional argument prefix.

Persists into `governance.db` directly (no runtime required). Session scope is not supported here because session rules live in the runtime memory; use the chat REPL approval flow instead.

**Usage:** `apollia-os permissions add [OPTIONS] --tool <NAME>`

###### **Options:**

* `--tool <NAME>` - Tool name (e.g. `file_write`, `bash_executor`)
* `--prefix <PREFIX>` - Optional argument prefix the rule applies to (e.g. a path). When omitted, the rule pre-authorizes any invocation of `--tool`, except for code executors (`bash_executor`, `python_executor`), which are never blanket-authorized. With a prefix, the rule is evaluated on every invocation against the call's argument; for a code executor it only ever covers a single simple command (no chaining, pipe, redirection or substitution)
* `--action <ACTION>` - Rule action: `allow` or `deny`

  Default value: `allow`

  Possible values: `allow`, `deny`

* `--scope <SCOPE>` - Rule scope: `project` or `global` (session is not persistable)

  Default value: `global`

  Possible values: `project`, `global`

* `--project-path <PATH>` - Project canonical path (required when `--scope project`)



## `apollia-os chat`

Interactive chat REPL + persisted session hygiene (delete, rename, export).

Without a subcommand: launches the REPL (resume with `--resume <id>`, or list recent sessions with `--list`). With a subcommand: operates on `~/.apollia/chat.db` directly; no runtime required.

**Usage:** `apollia-os chat [OPTIONS] [COMMAND]`

###### **Subcommands:**

* `delete` - Delete a persisted chat session and all of its messages
* `rename` - Set the user-defined title of a persisted chat session
* `export` - Export a persisted chat session to a file
* `config` - Manage the Chat Libre configuration (system prompt, allowed tools, backend)

###### **Options:**

* `--resume <SESSION_ID>` - Resume an existing session from its last message
* `--list` - List the 10 most recent sessions



## `apollia-os chat delete`

Delete a persisted chat session and all of its messages

**Usage:** `apollia-os chat delete [OPTIONS] <SESSION_ID>`

###### **Arguments:**

* `<SESSION_ID>` - Session id (8+ char ulid-like string returned by `chat --list`)

###### **Options:**

* `--confirm` - Skip the interactive confirmation prompt
* `--db <PATH>` - Override the chat database path (default: `~/.apollia/chat.db`)



## `apollia-os chat rename`

Set the user-defined title of a persisted chat session

**Usage:** `apollia-os chat rename [OPTIONS] <SESSION_ID> <TITLE>`

###### **Arguments:**

* `<SESSION_ID>` - Session id
* `<TITLE>` - New title (max 100 chars, leading/trailing whitespace trimmed)

###### **Options:**

* `--db <PATH>` - Override the chat database path



## `apollia-os chat export`

Export a persisted chat session to a file

**Usage:** `apollia-os chat export [OPTIONS] <SESSION_ID>`

###### **Arguments:**

* `<SESSION_ID>` - Session id

###### **Options:**

* `--output <PATH>` - Output file path. Defaults to stdout when omitted
* `--format <FORMAT>` - Output format: `markdown` (default) or `json`

  Default value: `markdown`

  Possible values: `markdown`, `json`

* `--db <PATH>` - Override the chat database path



## `apollia-os chat config`

Manage the Chat Libre configuration (system prompt, allowed tools, backend)

**Usage:** `apollia-os chat config <COMMAND>`

###### **Subcommands:**

* `get` - Print the current chat libre configuration
* `set` - Update one field of the chat libre configuration
* `reset` - Reset the configuration to the defaults (empty prompt, no tools)
* `permissions` - Manage persisted permission rules scoped to the Apollia Chat agent
* `authorizations` - Inspect or revoke in-memory session authorizations



## `apollia-os chat config get`

Print the current chat libre configuration

**Usage:** `apollia-os chat config get [OPTIONS]`

###### **Options:**

* `--db <PATH>`



## `apollia-os chat config set`

Update one field of the chat libre configuration

**Usage:** `apollia-os chat config set [OPTIONS] <KEY> <VALUE>`

###### **Arguments:**

* `<KEY>` - Field name: `system-prompt`, `allowed-tools`, or `llm-backend`
* `<VALUE>` - New value. For `allowed-tools`, expects a comma-separated list. For `llm-backend`, the literal `none` clears the backend

###### **Options:**

* `--db <PATH>`



## `apollia-os chat config reset`

Reset the configuration to the defaults (empty prompt, no tools)

**Usage:** `apollia-os chat config reset [OPTIONS]`

###### **Options:**

* `--confirm`
* `--db <PATH>`



## `apollia-os chat config permissions`

Manage persisted permission rules scoped to the Apollia Chat agent.

Mirrors the Desktop Settings → Chat permissions panel: lists or deletes rules stored in `governance.db` with `agent_id = apollia:chat`. Rules of scope `session` live in the runtime memory and are not visible here (same caveat as `permissions list`).

**Usage:** `apollia-os chat config permissions <COMMAND>`

###### **Subcommands:**

* `list` - List every persisted rule scoped to the Apollia Chat agent
* `delete` - Delete a chat-scoped permission rule by id



## `apollia-os chat config permissions list`

List every persisted rule scoped to the Apollia Chat agent

**Usage:** `apollia-os chat config permissions list [OPTIONS]`

###### **Options:**

* `--db <PATH>` - Override the `governance.db` path (default: `~/.apollia/governance.db`)



## `apollia-os chat config permissions delete`

Delete a chat-scoped permission rule by id

**Usage:** `apollia-os chat config permissions delete [OPTIONS] <ID>`

###### **Arguments:**

* `<ID>` - Rule id (as returned by `chat-config permissions list`)

###### **Options:**

* `--confirm` - Skip the confirmation prompt (required for scripts)
* `--db <PATH>` - Override the `governance.db` path



## `apollia-os chat config authorizations`

Inspect or revoke in-memory session authorizations.

These authorizations live only in the running daemon's `ChatSessionManager` (never persisted to `governance.db`). The CLI cannot reach in-memory state from outside the daemon, so this subcommand prints an explanatory error and exits 1 unless / until a runtime route is added. Use the Desktop Settings → Permissions panel or restart the daemon to clear stale authorizations.

**Usage:** `apollia-os chat config authorizations <COMMAND>`

###### **Subcommands:**

* `list` - List active in-memory session authorizations (requires runtime route)
* `revoke` - Revoke a single in-memory session authorization (requires runtime route)



## `apollia-os chat config authorizations list`

List active in-memory session authorizations (requires runtime route)

**Usage:** `apollia-os chat config authorizations list`



## `apollia-os chat config authorizations revoke`

Revoke a single in-memory session authorization (requires runtime route)

**Usage:** `apollia-os chat config authorizations revoke [OPTIONS] <SESSION_ID> <TOOL>`

###### **Arguments:**

* `<SESSION_ID>` - Session id
* `<TOOL>` - Tool name

###### **Options:**

* `--confirm` - Skip the interactive confirmation prompt



## `apollia-os mcp`

MCP server management (list, add, remove, show, test, restart, update, raw-config, set-approval, list-pending, revoke-approval, server)

**Usage:** `apollia-os mcp <COMMAND>`

###### **Subcommands:**

* `list` - List configured and, optionally, discovered MCP servers
* `set-approval` - Approve all calls to a tool on a server, persisted with the configured TTL
* `list-pending` - List all pending HITL approval requests awaiting human decision
* `revoke-approval` - Revoke a previously granted tool approval
* `add` - Register a new MCP server with the runtime (persisted in the config)
* `remove` - Remove an MCP server from the runtime
* `show` - Show the details of an MCP server
* `test` - Test the connection to an MCP server
* `restart` - Restart an MCP server
* `update` - Update the raw configuration of an existing MCP server
* `raw-config` - Show the raw persisted configuration of an MCP server
* `oauth` - Interactive OAuth (PKCE) management for HTTP/streamable-http MCP servers
* `secret` - Manage MCP server secrets (env-var values) in the OS keychain
* `server` - Launch Apollia as an MCP stdio server for external clients



## `apollia-os mcp list`

List configured and, optionally, discovered MCP servers

**Usage:** `apollia-os mcp list [OPTIONS]`

###### **Options:**

* `--discover` - Scan the local network via mDNS and append discovered servers.

   Performs a 3-second broadcast scan for `_apollia-mcp._tcp.local.` in addition to listing servers from the configuration file.
* `--config <PATH>` - Path to the MCP configuration file (default: `~/.apollia/mcp.toml`)
* `--json` - Output machine-readable JSON



## `apollia-os mcp set-approval`

Approve all calls to a tool on a server, persisted with the configured TTL.

After approval, calls to `<tool>` on `<server>` bypass the HITL suspension gate until the approval expires (default TTL: 24 h, configurable in apollia.toml).

**Usage:** `apollia-os mcp set-approval [OPTIONS] <SERVER> <TOOL>`

###### **Arguments:**

* `<SERVER>` - MCP server name (as declared in mcp.toml)
* `<TOOL>` - Tool name to approve

###### **Options:**

* `--db <PATH>` - Path to the approvals database (default: `~/.apollia/mcp_approvals.db`)
* `--ttl-hours <HOURS>` - Override the TTL for this approval, in hours (0 = never expires)

  Default value: `24`
* `--json` - Output machine-readable JSON



## `apollia-os mcp list-pending`

List all pending HITL approval requests awaiting human decision

**Usage:** `apollia-os mcp list-pending [OPTIONS]`

###### **Options:**

* `--db <PATH>` - Path to the approvals database (default: `~/.apollia/mcp_approvals.db`)
* `--json` - Output machine-readable JSON



## `apollia-os mcp revoke-approval`

Revoke a previously granted tool approval.

After revocation, calls to `<tool>` on `<server>` will be suspended again until a new approval is granted with `set-approval`.

**Usage:** `apollia-os mcp revoke-approval [OPTIONS] <SERVER> <TOOL>`

###### **Arguments:**

* `<SERVER>` - MCP server name (as declared in mcp.toml)
* `<TOOL>` - Tool name to revoke

###### **Options:**

* `--db <PATH>` - Path to the approvals database (default: `~/.apollia/mcp_approvals.db`)
* `--json` - Output machine-readable JSON



## `apollia-os mcp add`

Register a new MCP server with the runtime (persisted in the config)

**Usage:** `apollia-os mcp add [OPTIONS] <NAME>`

###### **Arguments:**

* `<NAME>` - Unique server name

###### **Options:**

* `--command <COMMAND>` - Command to launch (stdio transport) or URL (HTTP/SSE transport)
* `--url <URL>` - HTTP/SSE connection URL
* `--require-approval` - Require HITL approval for every tool call



## `apollia-os mcp remove`

Remove an MCP server from the runtime

**Usage:** `apollia-os mcp remove [OPTIONS] <NAME>`

###### **Arguments:**

* `<NAME>` - Server name

###### **Options:**

* `--confirm` - Confirm without an interactive prompt



## `apollia-os mcp show`

Show the details of an MCP server

**Usage:** `apollia-os mcp show <NAME>`

###### **Arguments:**

* `<NAME>` - Server name



## `apollia-os mcp test`

Test the connection to an MCP server

**Usage:** `apollia-os mcp test <TARGET>`

###### **Arguments:**

* `<TARGET>` - URL or command to test



## `apollia-os mcp restart`

Restart an MCP server

**Usage:** `apollia-os mcp restart <NAME>`

###### **Arguments:**

* `<NAME>` - Server name



## `apollia-os mcp update`

Update the raw configuration of an existing MCP server.

At least one of `--command`, `--url`, or `--require-approval` must be supplied. Fields that are omitted keep their previous value.

**Usage:** `apollia-os mcp update [OPTIONS] <NAME>`

###### **Arguments:**

* `<NAME>` - Server name

###### **Options:**

* `--command <COMMAND>` - New stdio command (stdio transport)
* `--url <URL>` - New HTTP/SSE URL
* `--require-approval <BOOL>` - Enable / disable the HITL approval lock

  Possible values: `true`, `false`




## `apollia-os mcp raw-config`

Show the raw persisted configuration of an MCP server.

Reads `mcp.db` directly via the runtime and returns the original definition (useful for bisecting a configuration regression).

**Usage:** `apollia-os mcp raw-config <NAME>`

###### **Arguments:**

* `<NAME>` - Server name



## `apollia-os mcp oauth`

Interactive OAuth (PKCE) management for HTTP/streamable-http MCP servers.

Same keychain entries as the Desktop wizard (`apollia-mcp-oauth/<server>`), so once a server is connected from one surface the other inherits the token.

**Usage:** `apollia-os mcp oauth <COMMAND>`

###### **Subcommands:**

* `login` - Run the interactive PKCE login flow for `<server>` and persist the token
* `status` - Report the persisted-token status for one or every configured server
* `logout` - Delete the persisted token for `<server>` from the OS keychain
* `client-id` - Manage per-env-var OAuth client-id overrides stored in the OS keychain
* `discover` - Run RFC 9728 + RFC 8414 OAuth discovery against `<server>` and print the resulting authorization server, scopes and endpoints



## `apollia-os mcp oauth login`

Run the interactive PKCE login flow for `<server>` and persist the token.

Opens the OAuth authorisation URL in the system browser, prints it on stdout (so headless setups can copy / paste it), waits for the authorisation server to redirect back to the loopback listener, then stores the resulting access + refresh tokens in the OS keychain under `apollia-mcp-oauth/<server>`.

**Usage:** `apollia-os mcp oauth login [OPTIONS] <SERVER>`

###### **Arguments:**

* `<SERVER>` - Server name as declared in `mcp.db` (matches the Desktop wizard)

###### **Options:**

* `--scopes <SCOPE>` - Optional comma-separated scope list. Omit to defer to the AS's `scopes_supported` (recommended)
* `--client-id <ID>` - Override the OAuth client id resolution (for tenants running their own AS app, usually unnecessary)
* `--db <PATH>` - Override the path to `mcp.db` (default: `~/.apollia/mcp.db`)



## `apollia-os mcp oauth status`

Report the persisted-token status for one or every configured server.

Surfaces token expiry, granted scopes, and identity claims (`sub`, `email`) without revealing the access token itself.

**Usage:** `apollia-os mcp oauth status [OPTIONS] [SERVER]`

###### **Arguments:**

* `<SERVER>` - Optional server name. When omitted, lists every server with a stored token plus those declared in `mcp.db` but unauthenticated

###### **Options:**

* `--db <PATH>`



## `apollia-os mcp oauth logout`

Delete the persisted token for `<server>` from the OS keychain.

The authorisation server is **not** notified: call the provider's revocation endpoint manually if a server-side revocation is required.

**Usage:** `apollia-os mcp oauth logout [OPTIONS] <SERVER>`

###### **Arguments:**

* `<SERVER>` - Server name to forget

###### **Options:**

* `--confirm` - Skip the confirmation prompt



## `apollia-os mcp oauth client-id`

Manage per-env-var OAuth client-id overrides stored in the OS keychain.

Mirrors the Desktop Settings → MCP → "OAuth client id" panel. Resolution chain: env var > keychain (this command) > build-time default.

**Usage:** `apollia-os mcp oauth client-id <COMMAND>`

###### **Subcommands:**

* `set` - Persist `<value>` as the OAuth client id for the env var `<env_var>`
* `clear` - Remove the persisted client id stored under `<env_var>`



## `apollia-os mcp oauth client-id set`

Persist `<value>` as the OAuth client id for the env var `<env_var>`.

The env var is the same one the connector wizard surfaces (e.g. `APOLLIA_FIGMA_CLIENT_ID`). Pass an empty value to fail validation; use `clear` to remove.

**Usage:** `apollia-os mcp oauth client-id set <ENV_VAR> <VALUE>`

###### **Arguments:**

* `<ENV_VAR>` - Env var name (e.g. `APOLLIA_FIGMA_CLIENT_ID`)
* `<VALUE>` - New client id value



## `apollia-os mcp oauth client-id clear`

Remove the persisted client id stored under `<env_var>`

**Usage:** `apollia-os mcp oauth client-id clear [OPTIONS] <ENV_VAR>`

###### **Arguments:**

* `<ENV_VAR>` - Env var name

###### **Options:**

* `--confirm` - Skip the interactive confirmation prompt



## `apollia-os mcp oauth discover`

Run RFC 9728 + RFC 8414 OAuth discovery against `<server>` and print the resulting authorization server, scopes and endpoints.

Read-only: no token is exchanged and no secret is written. Useful to confirm an HTTP MCP server's PRM + AS metadata before invoking `login`.

**Usage:** `apollia-os mcp oauth discover [OPTIONS] <SERVER>`

###### **Arguments:**

* `<SERVER>` - Server name as declared in `mcp.db`

###### **Options:**

* `--db <PATH>` - Override the path to `mcp.db` (default: `~/.apollia/mcp.db`)



## `apollia-os mcp secret`

Manage MCP server secrets (env-var values) in the OS keychain.

Mirrors the Desktop secret store: entries are keyed by `{server}:{env_var}` under the keychain service `apollia-mcp`, so a secret stored by the CLI is read transparently by the Desktop runtime.

**Usage:** `apollia-os mcp secret <COMMAND>`

###### **Subcommands:**

* `set` - Persist `<value>` as the secret for `(<server>, <env_var>)`
* `delete` - Delete the stored secret for `(<server>, <env_var>)`



## `apollia-os mcp secret set`

Persist `<value>` as the secret for `(<server>, <env_var>)`.

The value is written to the OS keychain under service `apollia-mcp` and composite key `{server}:{env_var}`. Use `delete` to remove. The CLI does not echo the value back, but it is stored as-is (no trimming beyond stripping leading / trailing whitespace).

**Usage:** `apollia-os mcp secret set <SERVER> <ENV_VAR> <VALUE>`

###### **Arguments:**

* `<SERVER>` - MCP server name (matches the name in `mcp.db` / `mcp.toml`)
* `<ENV_VAR>` - Environment variable name (e.g. `NOTION_API_KEY`)
* `<VALUE>` - Secret value to store



## `apollia-os mcp secret delete`

Delete the stored secret for `(<server>, <env_var>)`

**Usage:** `apollia-os mcp secret delete [OPTIONS] <SERVER> <ENV_VAR>`

###### **Arguments:**

* `<SERVER>` - MCP server name
* `<ENV_VAR>` - Environment variable name

###### **Options:**

* `--confirm` - Skip the interactive confirmation prompt



## `apollia-os mcp server`

Launch Apollia as an MCP stdio server for external clients.

Exposes native tools to MCP clients (Claude Desktop, VS Code, Cursor). Use `--with-runtime` to additionally expose `submit_task`.

**Usage:** `apollia-os mcp server [OPTIONS]`

###### **Options:**

* `--with-runtime` - Enable the `submit_task` tool by starting the full Apollia runtime.

   Without this flag, only the 9 stateless native tools are exposed.

  Default value: `false`
* `--sandbox-root <PATH>` - Sandbox root for file tools (default: user home directory).

   File operations are restricted to this directory tree.



## `apollia-os update`

Check for and install updates from GitHub Releases

**Usage:** `apollia-os update [OPTIONS]`

###### **Options:**

* `--check` - Only check for a newer version without downloading or installing
* `--yes` - Install without asking for interactive confirmation



## `apollia-os workspace`

Workspace inspection and initialization (status, init)

**Usage:** `apollia-os workspace <COMMAND>`

###### **Subcommands:**

* `status` - Show the status of the current workspace
* `init` - Initialise APOLLIA.md in the current directory



## `apollia-os workspace status`

Show the status of the current workspace

**Usage:** `apollia-os workspace status`



## `apollia-os workspace init`

Initialise APOLLIA.md in the current directory

**Usage:** `apollia-os workspace init [OPTIONS]`

###### **Options:**

* `--force` - Overwrite APOLLIA.md if it already exists

  Default value: `false`



## `apollia-os review`

Automated code or plan review via the apollia-review agent

**Usage:** `apollia-os review [OPTIONS]`

###### **Options:**

* `--task <ID>` - Apollia task ID whose execution plan should be reviewed
* `--pr <N>` - GitHub pull-request number to fetch via `gh pr diff`
* `--diff <FILE>` - Local diff / patch file to analyse



## `apollia-os resilience`

Circuit breaker inspection and reset (list, show, reset)

**Usage:** `apollia-os resilience <COMMAND>`

###### **Subcommands:**

* `list` - List all registered circuit breakers with their state and counters
* `show` - Show the state of a single circuit breaker
* `reset` - Reset a circuit breaker to CLOSED state immediately



## `apollia-os resilience list`

List all registered circuit breakers with their state and counters

**Usage:** `apollia-os resilience list`



## `apollia-os resilience show`

Show the state of a single circuit breaker

**Usage:** `apollia-os resilience show <TOOL_NAME>`

###### **Arguments:**

* `<TOOL_NAME>` - Tool name as registered in the Tool Registry



## `apollia-os resilience reset`

Reset a circuit breaker to CLOSED state immediately

**Usage:** `apollia-os resilience reset [OPTIONS] <TOOL_NAME>`

###### **Arguments:**

* `<TOOL_NAME>` - Tool name to reset

###### **Options:**

* `--confirm` - Skip the interactive confirmation prompt



## `apollia-os plan`

Plan domain management (cache: stats, clear, evict)

**Usage:** `apollia-os plan <COMMAND>`

###### **Subcommands:**

* `cache` - Plan cache management (stats, clear, evict)



## `apollia-os plan cache`

Plan cache management (stats, clear, evict)

**Usage:** `apollia-os plan cache <COMMAND>`

###### **Subcommands:**

* `stats` - Display cache statistics (total entries, hits, oldest/newest entry)
* `clear` - Remove all cached plans
* `evict` - Evict entries older than `--max-age-days` days (default: 7)



## `apollia-os plan cache stats`

Display cache statistics (total entries, hits, oldest/newest entry)

**Usage:** `apollia-os plan cache stats`



## `apollia-os plan cache clear`

Remove all cached plans.

Prompts for confirmation unless `--confirm` is passed.

**Usage:** `apollia-os plan cache clear [OPTIONS]`

###### **Options:**

* `--confirm` - Skip the interactive confirmation prompt. `--force` is the name this flag published before the rule of section 2 and stays accepted



## `apollia-os plan cache evict`

Evict entries older than `--max-age-days` days (default: 7)

**Usage:** `apollia-os plan cache evict [OPTIONS]`

###### **Options:**

* `--max-age-days <DAYS>` - Maximum entry age in days before eviction

  Default value: `7`
* `--confirm` - Skip the interactive confirmation prompt



## `apollia-os doctor`

Diagnose the local Apollia environment (no runtime required).

Verifies the Apollia home directory, the config file, local SQLite databases, the Python bridge, and the runtime socket. Prints a status table and exits 0 when healthy, 1 when at least one check errors.

**Usage:** `apollia-os doctor`



## `apollia-os inspect`

Statically inspect a Python agent file (no runtime required).

Loads the agent module in isolation, introspects its manifest, and checks declared datasources, templates, secrets, and permissions without starting the runtime. Exits 0 when the inspection succeeds (warnings allowed), 1 when it fails.

**Usage:** `apollia-os inspect <PATH>`

###### **Arguments:**

* `<PATH>` - Path to the agent `.py` file



## `apollia-os logs`

Tail or follow the runtime log file.

Defaults to `~/.apollia/logs/runtime.log`. When this file is absent the command prints a hint explaining how to redirect daemon stderr.

**Usage:** `apollia-os logs [OPTIONS]`

###### **Options:**

* `--file <PATH>` - Path to the log file (default: `~/.apollia/logs/runtime.log`)
* `--last <N>` - Print only the last `N` lines (default: 50)

  Default value: `50`
* `-f`, `--follow` - Follow the file and stream new lines as they are appended



## `apollia-os version`

Print the binary version (use `--json` for machine-readable output)

**Usage:** `apollia-os version`



## `apollia-os connector`

Native SaaS connector management (list, accounts, test, revoke).

Operates on the multi-account keyring without requiring the runtime to be started. Accounts are connected from the desktop app (Settings > Integrations); the OAuth flow needs a browser redirect, so there is no CLI equivalent.

**Usage:** `apollia-os connector <COMMAND>`

###### **Subcommands:**

* `list` - List all native SaaS connectors registered in this build
* `accounts` - List OAuth-connected accounts for one or all providers
* `test` - Probe the connector for an account by calling the userinfo endpoint
* `revoke` - Revoke the stored token for `(provider, account)`
* `client-id` - Manage OAuth client_id overrides in `~/.apollia/oauth-clients.toml`
* `client-secret` - Manage OAuth client_secret overrides in `~/.apollia/oauth-clients.toml`
* `api-key` - Manage API key overrides (Google Picker) in `~/.apollia/oauth-clients.toml`
* `drive` - Manage per-account Google Drive folder preferences



## `apollia-os connector list`

List all native SaaS connectors registered in this build.

Output covers the connector id, display name, publisher, and the services it exposes (e.g. `gmail`, `gcal`, `gdrive`).

**Usage:** `apollia-os connector list`



## `apollia-os connector accounts`

List OAuth-connected accounts for one or all providers

**Usage:** `apollia-os connector accounts [OPTIONS]`

###### **Options:**

* `--provider <PROVIDER>` - Filter by provider: `google` or `microsoft`. Omit to list both



## `apollia-os connector test`

Probe the connector for an account by calling the userinfo endpoint.

Returns the live identity claim and the scopes the upstream Authorization Server reports as granted, the same shape used by `connector.check()` inside the runtime.

**Usage:** `apollia-os connector test <PROVIDER> <ACCOUNT>`

###### **Arguments:**

* `<PROVIDER>` - Provider id: `google` or `microsoft`
* `<ACCOUNT>` - Account identifier (email when supplied during OAuth login)



## `apollia-os connector revoke`

Revoke the stored token for `(provider, account)`.

Only the local keyring entry is cleared, the upstream Authorization Server is not notified. Use the provider's web revocation page for a server-side revocation.

**Usage:** `apollia-os connector revoke [OPTIONS] <PROVIDER> <ACCOUNT>`

###### **Arguments:**

* `<PROVIDER>` - Provider id
* `<ACCOUNT>` - Account id to revoke

###### **Options:**

* `--confirm` - Skip the confirmation prompt (required for scripts)



## `apollia-os connector client-id`

Manage OAuth client_id overrides in `~/.apollia/oauth-clients.toml`.

Power-user / Expert Mode: lets a CLI operator plug in their own Google or Microsoft client_id without rebuilding the binary. Resolution chain per provider is `env var > oauth-clients.toml > compiled default`.

**Usage:** `apollia-os connector client-id <COMMAND>`

###### **Subcommands:**

* `list` - List every provider's effective client_id + source + override
* `set` - Set the client_id override for `<provider>`



## `apollia-os connector client-id list`

List every provider's effective client_id + source + override

**Usage:** `apollia-os connector client-id list`



## `apollia-os connector client-id set`

Set the client_id override for `<provider>`.

Pass an empty string (`""`) to clear the override.

**Usage:** `apollia-os connector client-id set <PROVIDER> <CLIENT_ID>`

###### **Arguments:**

* `<PROVIDER>` - Provider id: `google` or `microsoft`
* `<CLIENT_ID>` - New client_id value. Empty string clears the override



## `apollia-os connector client-secret`

Manage OAuth client_secret overrides in `~/.apollia/oauth-clients.toml`.

Required by Google (Installed App needs a secret) and a no-op for Microsoft (public client per spec). File is created on demand with `0o600` permissions on Unix.

**Usage:** `apollia-os connector client-secret <COMMAND>`

###### **Subcommands:**

* `set` - Set the client_secret override for `<provider>`



## `apollia-os connector client-secret set`

Set the client_secret override for `<provider>`.

Pass an empty string (`""`) to clear the override. The CLI does not echo the secret back, but it is written to `~/.apollia/oauth-clients.toml`.

**Usage:** `apollia-os connector client-secret set <PROVIDER> <CLIENT_SECRET>`

###### **Arguments:**

* `<PROVIDER>` - Provider id: `google` or `microsoft`
* `<CLIENT_SECRET>` - New client_secret value. Empty string clears the override



## `apollia-os connector api-key`

Manage API key overrides (Google Picker) in `~/.apollia/oauth-clients.toml`.

Google-only today. Microsoft slot is reserved for the OneDrive File Picker if added later.

**Usage:** `apollia-os connector api-key <COMMAND>`

###### **Subcommands:**

* `set` - Set the API key override for `<provider>`



## `apollia-os connector api-key set`

Set the API key override for `<provider>`.

Pass an empty string (`""`) to clear the override.

**Usage:** `apollia-os connector api-key set <PROVIDER> <API_KEY>`

###### **Arguments:**

* `<PROVIDER>` - Provider id: `google` or `microsoft`
* `<API_KEY>` - New API key value. Empty string clears the override



## `apollia-os connector drive`

Manage per-account Google Drive folder preferences.

Operates on `~/.apollia/drive-prefs.toml` and is independent of the runtime. The `picked` sub-group lists folders captured via the Desktop Picker (the CLI cannot pick, no UI, but can review and remove them).

**Usage:** `apollia-os connector drive <COMMAND>`

###### **Subcommands:**

* `folder` - Manage the per-account Drive root folder path



## `apollia-os connector drive folder`

Manage the per-account Drive root folder path

**Usage:** `apollia-os connector drive folder <COMMAND>`

###### **Subcommands:**

* `list` - List the folder override + effective path for every Google account
* `set` - Set the folder path override for `<account>`
* `reset` - Reset the folder override for `<account>` (falls back to the default)
* `picked` - Manage the picked-folder list captured via the Desktop Drive Picker



## `apollia-os connector drive folder list`

List the folder override + effective path for every Google account

**Usage:** `apollia-os connector drive folder list`



## `apollia-os connector drive folder set`

Set the folder path override for `<account>`

**Usage:** `apollia-os connector drive folder set <ACCOUNT> <PATH>`

###### **Arguments:**

* `<ACCOUNT>` - Account id (typically the Google email)
* `<PATH>` - New folder path (e.g. `Apollia/Workspace`)



## `apollia-os connector drive folder reset`

Reset the folder override for `<account>` (falls back to the default)

**Usage:** `apollia-os connector drive folder reset [OPTIONS] <ACCOUNT>`

###### **Arguments:**

* `<ACCOUNT>` - Account id

###### **Options:**

* `--confirm` - Skip the interactive confirmation prompt



## `apollia-os connector drive folder picked`

Manage the picked-folder list captured via the Desktop Drive Picker

**Usage:** `apollia-os connector drive folder picked <COMMAND>`

###### **Subcommands:**

* `list` - List the picked Drive folders persisted for `<account>`
* `remove` - Remove a picked folder from the persisted list



## `apollia-os connector drive folder picked list`

List the picked Drive folders persisted for `<account>`

**Usage:** `apollia-os connector drive folder picked list <ACCOUNT>`

###### **Arguments:**

* `<ACCOUNT>` - Account id



## `apollia-os connector drive folder picked remove`

Remove a picked folder from the persisted list

**Usage:** `apollia-os connector drive folder picked remove [OPTIONS] <ACCOUNT> <FOLDER_ID>`

###### **Arguments:**

* `<ACCOUNT>` - Account id
* `<FOLDER_ID>` - Drive folder id (the same id surfaced by `picked list`)

###### **Options:**

* `--confirm` - Skip the interactive confirmation prompt



## `apollia-os config`

Global apollia.toml management (get, set, validate, edit, show).

Edits the on-disk config without touching the runtime. The section helpers `tools config` and `stt config` remain available for their respective slices.

**Usage:** `apollia-os config <COMMAND>`

###### **Subcommands:**

* `get` - Print the on-disk apollia.toml (or a single value at `KEY_PATH`)
* `set` - Write `VALUE` at `KEY_PATH`, preserving formatting and comments
* `validate` - Parse the config file and report any error
* `edit` - Open the config file in `$EDITOR`
* `show` - Print the resolved configuration (parsed struct as JSON)
* `reset` - Wipe `~/.apollia/`, an irreversible factory reset



## `apollia-os config get`

Print the on-disk apollia.toml (or a single value at `KEY_PATH`).

`KEY_PATH` follows dotted notation, e.g. `llm.default` or `runtime.bind_addr`.

**Usage:** `apollia-os config get [OPTIONS] [KEY]`

###### **Arguments:**

* `<KEY>` - Optional dotted key path

###### **Options:**

* `--file <PATH>` - Optional config file path override



## `apollia-os config set`

Write `VALUE` at `KEY_PATH`, preserving formatting and comments.

`VALUE` is parsed as a TOML scalar: bare booleans, integers, and floats are recognized; everything else is treated as a string. Use a quoted argument when the value contains spaces or special characters.

**Usage:** `apollia-os config set [OPTIONS] <KEY> <VALUE>`

###### **Arguments:**

* `<KEY>` - Dotted key path
* `<VALUE>` - Value to write

###### **Options:**

* `--file <PATH>` - Optional config file path override



## `apollia-os config validate`

Parse the config file and report any error.

Exits 0 when the file is absent or valid, 1 otherwise.

**Usage:** `apollia-os config validate [OPTIONS]`

###### **Options:**

* `--file <PATH>` - Optional config file path override



## `apollia-os config edit`

Open the config file in `$EDITOR`.

Refuses to run when stdout is not a TTY or `--json` is set.

**Usage:** `apollia-os config edit [OPTIONS]`

###### **Options:**

* `--file <PATH>` - Optional config file path override



## `apollia-os config show`

Print the resolved configuration (parsed struct as JSON).

Cascades through file > defaults. Environment overlay is performed at runtime startup and is not reproduced here.

**Usage:** `apollia-os config show [OPTIONS]`

###### **Options:**

* `--file <PATH>` - Optional config file path override



## `apollia-os config reset`

Wipe `~/.apollia/`, an irreversible factory reset.

Deletes every SQLite database, log, journal, memory file, OAuth client override, and apollia.toml stored under `~/.apollia/`. Keychain entries (OS-managed) are NOT touched: use `connector revoke` or `mcp oauth logout` to clear those.

**Usage:** `apollia-os config reset [OPTIONS]`

###### **Options:**

* `--confirm` - Skip the interactive confirmation prompt (required for scripts)
* `--dry-run` - Print the resolved Apollia home and the entries that would be removed, but do not delete anything
* `--home <PATH>` - Override the Apollia home path (default: `~/.apollia`)



## `apollia-os profile`

User profile management (show, set, forget, reset, export, import).

Operates on `~/.apollia/user_memory.db` directly; no runtime required.

**Usage:** `apollia-os profile <COMMAND>`

###### **Subcommands:**

* `show` - Display every key currently stored in the global user profile
* `set` - Insert or replace the value of `KEY`
* `forget` - Remove the entry stored at `KEY`
* `reset` - Reset the entire profile (deletes every entry)
* `schema` - Print the canonical schema (the known structured fields)
* `export` - Dump every entry as a JSON array on stdout (or to `--output`)
* `import` - Import entries from a JSON file produced by `export`



## `apollia-os profile show`

Display every key currently stored in the global user profile

**Usage:** `apollia-os profile show [OPTIONS]`

###### **Options:**

* `--db <PATH>` - Optional override for the user_memory.db path



## `apollia-os profile set`

Insert or replace the value of `KEY`

**Usage:** `apollia-os profile set [OPTIONS] <KEY> <VALUE>`

###### **Arguments:**

* `<KEY>` - Profile key (e.g. `name`, `email`, `preferences.tone`)
* `<VALUE>` - Value to store

###### **Options:**

* `--db <PATH>`



## `apollia-os profile forget`

Remove the entry stored at `KEY`

**Usage:** `apollia-os profile forget [OPTIONS] <KEY>`

###### **Arguments:**

* `<KEY>` - Profile key to remove

###### **Options:**

* `--confirm` - Skip the interactive confirmation prompt
* `--db <PATH>`



## `apollia-os profile reset`

Reset the entire profile (deletes every entry)

**Usage:** `apollia-os profile reset [OPTIONS]`

###### **Options:**

* `--confirm` - Skip the confirmation prompt
* `--db <PATH>`



## `apollia-os profile schema`

Print the canonical schema (the known structured fields)

**Usage:** `apollia-os profile schema [OPTIONS]`

###### **Options:**

* `--db <PATH>`



## `apollia-os profile export`

Dump every entry as a JSON array on stdout (or to `--output`)

**Usage:** `apollia-os profile export [OPTIONS]`

###### **Options:**

* `--output <PATH>` - Optional destination file (default: stdout)
* `--db <PATH>`



## `apollia-os profile import`

Import entries from a JSON file produced by `export`

**Usage:** `apollia-os profile import [OPTIONS] --input <PATH>`

###### **Options:**

* `--input <PATH>` - Source file (JSON array of `ProfileEntry`)
* `--overwrite` - Overwrite existing entries with the same key
* `--db <PATH>`



## `apollia-os project`

Project management (list, create, show, update, delete, agents, templates).

Operates locally on `~/.apollia/projects.db`; the runtime does not need to be running.

**Usage:** `apollia-os project <COMMAND>`

###### **Subcommands:**

* `list` - List every registered project (alphabetical)
* `create` - Create a new project and print its id
* `show` - Print the full detail of a project (documents, providers, agents)
* `update` - Update one or more mutable fields on an existing project
* `delete` - Delete a project and cascade its documents/providers
* `agents` - List the agents linked to a project
* `templates` - List or seed the available project templates
* `link` - Link (or unlink) a chat session to a project
* `chats` - List chat sessions linked to a project



## `apollia-os project list`

List every registered project (alphabetical)

**Usage:** `apollia-os project list [OPTIONS]`

###### **Options:**

* `--db <PATH>`



## `apollia-os project create`

Create a new project and print its id

**Usage:** `apollia-os project create [OPTIONS] <NAME>`

###### **Arguments:**

* `<NAME>` - Project display name

###### **Options:**

* `--description <DESCRIPTION>` - Optional one-line description
* `--instructions <INSTRUCTIONS>` - Optional initial instructions (Markdown)
* `--workspace <DIR>` - Optional workspace directory used by context providers
* `--db <PATH>`



## `apollia-os project show`

Print the full detail of a project (documents, providers, agents)

**Usage:** `apollia-os project show [OPTIONS] <ID>`

###### **Arguments:**

* `<ID>` - Project id (UUID)

###### **Options:**

* `--db <PATH>`



## `apollia-os project update`

Update one or more mutable fields on an existing project

**Usage:** `apollia-os project update [OPTIONS] <ID>`

###### **Arguments:**

* `<ID>` - Project id

###### **Options:**

* `--name <NAME>`
* `--description <DESCRIPTION>`
* `--instructions <INSTRUCTIONS>`
* `--workspace <DIR>`
* `--db <PATH>`



## `apollia-os project delete`

Delete a project and cascade its documents/providers

**Usage:** `apollia-os project delete [OPTIONS] <ID>`

###### **Arguments:**

* `<ID>` - Project id

###### **Options:**

* `--confirm` - Skip the confirmation prompt
* `--db <PATH>`



## `apollia-os project agents`

List the agents linked to a project

**Usage:** `apollia-os project agents <COMMAND>`

###### **Subcommands:**

* `list` - List agent names linked to a project
* `add` - Link an agent to a project
* `remove` - Unlink an agent from a project



## `apollia-os project agents list`

List agent names linked to a project

**Usage:** `apollia-os project agents list [OPTIONS] <PROJECT>`

###### **Arguments:**

* `<PROJECT>` - Project id

###### **Options:**

* `--db <PATH>`



## `apollia-os project agents add`

Link an agent to a project

**Usage:** `apollia-os project agents add [OPTIONS] <PROJECT> <AGENT>`

###### **Arguments:**

* `<PROJECT>` - Project id
* `<AGENT>` - Agent name

###### **Options:**

* `--db <PATH>`



## `apollia-os project agents remove`

Unlink an agent from a project

**Usage:** `apollia-os project agents remove [OPTIONS] <PROJECT> <AGENT>`

###### **Arguments:**

* `<PROJECT>` - Project id
* `<AGENT>` - Agent name

###### **Options:**

* `--confirm` - Skip the interactive confirmation prompt
* `--db <PATH>`



## `apollia-os project templates`

List or seed the available project templates

**Usage:** `apollia-os project templates <COMMAND>`

###### **Subcommands:**

* `list` - List the available templates (builtin + custom)
* `seed-builtins` - Re-seed the builtin templates into the database



## `apollia-os project templates list`

List the available templates (builtin + custom)

**Usage:** `apollia-os project templates list [OPTIONS]`

###### **Options:**

* `--db <PATH>`



## `apollia-os project templates seed-builtins`

Re-seed the builtin templates into the database

**Usage:** `apollia-os project templates seed-builtins [OPTIONS]`

###### **Options:**

* `--db <PATH>`



## `apollia-os project link`

Link (or unlink) a chat session to a project.

Writes `chat_sessions.project_id` directly via `apollia_runtime::chat::ChatSessionRepository` so the runtime does not need to be running. Pass `--unlink` to clear the session's project link instead of setting it; the project_id positional is then ignored.

**Usage:** `apollia-os project link [OPTIONS] --session <ID> <PROJECT_ID>`

###### **Arguments:**

* `<PROJECT_ID>` - Project id (UUID returned by `project list`)

###### **Options:**

* `--session <ID>` - Chat session id (returned by `chat --list`)
* `--unlink` - Clear the session's project_id instead of setting it
* `--chat-db <PATH>` - Override the chat database path (default: `~/.apollia/chat.db`)



## `apollia-os project chats`

List chat sessions linked to a project

**Usage:** `apollia-os project chats [OPTIONS] <PROJECT_ID>`

###### **Arguments:**

* `<PROJECT_ID>` - Project id (UUID)

###### **Options:**

* `--chat-db <PATH>` - Override the chat database path



## `apollia-os trace`

Print the event-sourced trace of a task

**Usage:** `apollia-os trace [OPTIONS] <TASK_ID>`

###### **Arguments:**

* `<TASK_ID>` - Task identifier

###### **Options:**

* `--format <FORMAT>` - Force JSON output even without global `--json`

  Default value: `human`

  Possible values: `human`, `json`




## `apollia-os digest`

Aggregated activity overview (tasks + LLM costs + audit stats)

**Usage:** `apollia-os digest [OPTIONS]`

###### **Options:**

* `--since <SINCE>` - Time window: 24h, 7d, or 30d

  Default value: `24h`

  Possible values:
  - `24h`:
    Last 24 hours
  - `7d`:
    Last 7 days
  - `30d`:
    Last 30 days




## `apollia-os completions`

Generate a shell completion script (bash, zsh, fish, powershell, ...)

**Usage:** `apollia-os completions <SHELL>`

###### **Arguments:**

* `<SHELL>` - Target shell

  Possible values: `bash`, `elvish`, `fish`, `powershell`, `zsh`




## `apollia-os guide`

Short, task-oriented help by theme (chat, governance, audit, ...).

With no topic, lists the available topics.

**Usage:** `apollia-os guide [TOPIC]`

###### **Arguments:**

* `<TOPIC>` - Topic to display



## `apollia-os do`

Map a natural-language request to a command (local model), then run it.

Shows the mapped command as a dry-run and asks for confirmation unless -y.

**Usage:** `apollia-os do [OPTIONS] <REQUEST>`

###### **Arguments:**

* `<REQUEST>` - The request, in natural language

###### **Options:**

* `-y`, `--yes` - Skip the confirmation prompt (non-interactive use)



## `apollia-os explain`

Explain a command or an error message in plain language (local model)

**Usage:** `apollia-os explain <TEXT>`

###### **Arguments:**

* `<TEXT>` - The command or error text to explain



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>
