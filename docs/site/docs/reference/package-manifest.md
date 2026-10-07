---
sidebar_position: 4.5
title: Package manifest (apollia.toml)
description: "Every table and key of the apollia.toml at the root of a package, the rules a manifest must hold, and a complete example."
---

# Package manifest (apollia.toml)

A package is a folder with an `apollia.toml` at its root. The manifest declares
what the package delivers: agents, which are data, Python tools, Python
workflows, the connections it needs, the MCP servers it brings, its Python
dependencies and the triggers it proposes. `apollia-os package build` turns the
folder into a `.apo` archive, and reading the manifest never runs the code of
the package.

This file shares its name with the configuration file of the runtime, described
in [Configuration (apollia.toml)](/reference/configuration). They are two
different files: this one sits at the root of a package folder and travels
inside the archive.

## Strict schema

The schema is strict. A key the schema does not know is refused, and the refusal
names the full path of the key:

```text
apollia.toml: unknown key `agents[0].defaults.model.temprature`
```

Nothing is ignored, so what you write is exactly what Apollia reads.

## Tables

| Table | Cardinality | Declares |
| --- | --- | --- |
| `[package]` | one, required | The identity of the package. |
| `[[agents]]` | zero or more | Agents: a mission, examples and defaults. No code. |
| `[[tools]]` | zero or more | Python tools, each reachable as `pkg:<package>/<name>`. |
| `[[workflows]]` | zero or more | Python workflows with their declared steps. |
| `[[requires]]` | zero or more | Connections the package needs. |
| `[[mcp_servers]]` | zero or more | MCP servers the package brings. |
| `[pip]` | zero or one | Python dependencies of the package environment. |
| `[[triggers]]` | zero or more | Triggers the package proposes. |

A package declares at least one agent, one tool or one workflow.

## `[package]`

| Key | Type | Required | Meaning |
| --- | --- | --- | --- |
| `name` | string | yes | Identifier of the package, `[a-z0-9_-]+`. Prefix of its tools: `pkg:<name>/<tool>`. |
| `version` | string | yes | Semver version of the package, for example `1.2.0`. |
| `publisher` | string | yes | Publisher shown when the package is installed. |
| `description` | string | no | What the package delivers. |
| `sdk` | string | yes | Semver requirement on the SDK the package was written against, for example `">=0.3, <0.4"`. |

## `[[agents]]`

An agent is data. The built-in agent engine runs it; the package ships no code
for it.

| Key | Type | Required | Meaning |
| --- | --- | --- | --- |
| `id` | string | yes | Identifier, `[a-z0-9_-]+`, unique in the package. |
| `name` | string | yes | Display name. |
| `mission` | string | yes | What the agent is for, in the words of its author. Used as its instructions. |
| `examples` | list of strings | no | Example requests shown on the sheet of the agent. |

### `[agents.defaults]`

Every value here is a default the package recommends, never a constraint: an
override the user sets on their machine takes precedence, and a package update
keeps it.

| Key | Type | Meaning |
| --- | --- | --- |
| `access` | list of strings | Tools granted by default: a native tool name, `mcp:<server>/<tool>`, or `pkg:<package>/<tool>`. A `pkg:` entry must name a tool of this package. |
| `sources` | list of strings | Sources checked by default. `"profile"` lets the agent read the user profile. |
| `autonomy` | string | Autonomy tier: `assisted`, `supervised`, `bounded_autonomous` or `long_autonomous`. Without it, the global default applies. |
| `planning` | string | Whether the agent plans a task before acting on it: `never`, `auto` or `always`. Without it, `auto`. |

### `[agents.defaults.model]`

| Key | Type | Meaning |
| --- | --- | --- |
| `model` | string | Model the package recommends. |
| `preset` | string | Preset of request settings the package recommends. |

`[agents.defaults.model.request]` carries the request settings themselves:
`temperature`, `top_p`, `top_k`, `repetition_penalty`, `max_tokens` and `seed`.

## `[[tools]]`

One entry per Python tool. `python -m apollia describe <folder>` writes these
entries from the functions decorated with `@tool`, so that the manifest and the
code cannot disagree.

| Key | Type | Required | Meaning |
| --- | --- | --- | --- |
| `name` | string | yes | Identifier, `[a-z0-9_-]+`, unique in the package. |
| `module` | string | yes | Python module, relative to the package root, for example `tools/invoices.py`. |
| `description` | string | yes | What the tool does, for the model and for the sheet of the agent. |
| `action` | string | yes | Action family, see below. |
| `inputs` | table | no | JSON Schema of the inputs. |

### Action families {#action-families}

A tool and a workflow each declare one action family: what calling it does to
the world. Every family except `read` makes the access sensitive.

| Family | Meaning | Sensitive |
| --- | --- | --- |
| `read` | Reads data that is not personal. | no |
| `read_personal` | Reads personal data: mail, calendar, the documents of a user. | yes |
| `write_or_delete` | Writes or deletes. | yes |
| `send_external` | Sends something outside the machine. | yes |
| `execute_code` | Executes code. | yes |

## `[[workflows]]`

One entry per Python workflow, written by `python -m apollia describe <folder>`
from the functions decorated with `@workflow`.

| Key | Type | Required | Meaning |
| --- | --- | --- | --- |
| `name` | string | yes | Identifier, `[a-z0-9_-]+`, unique in the package. The workflow is addressed as `<package>/<name>`. |
| `module` | string | yes | Python module, relative to the package root. |
| `description` | string | yes | What the workflow does. |
| `action` | string | yes | Action family: the most sensitive thing the workflow does. |
| `inputs` | table | no | JSON Schema of the inputs. Its `x-order` key lists them in the order the launch form follows. |
| `steps` | list of strings | no | Names of the steps, in order. |

## `[[requires]]`

| Key | Type | Required | Meaning |
| --- | --- | --- | --- |
| `type` | string | yes | Connection type, as the connection catalogue names it, for example `gmail`. |
| `reason` | string | no | Why the package needs it, shown when the connection is missing. |

## `[[mcp_servers]]`

The keys, defaults and meaning are those of a server in `mcp.toml`.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `name` | string | required | Server name, `[a-z0-9_-]+`. Prefix of its tools: `mcp:<name>/<tool>`. |
| `command` | string | empty | Executable to spawn; empty for a network transport. |
| `args` | list of strings | empty | Arguments handed to the command. |
| `env` | table | empty | Environment of the server process; `${VAR}` placeholders are allowed. |
| `transport` | string | `stdio` | `stdio`, `streamable-http` or `sse`. |
| `url` | string | none | Server URL for a network transport. |
| `init_timeout_secs` | integer | `30` | Timeout of the `initialize` handshake. |
| `call_timeout_secs` | integer | `60` | Timeout of one `tools/call`. |
| `max_response_bytes` | integer | `8388608` | Cap on one server response. |
| `max_tools` | integer | `256` | Cap on the tools kept from `tools/list`. |
| `tags` | list of strings | empty | Tags applied to every tool of the server. |

## `[pip]`

| Key | Type | Meaning |
| --- | --- | --- |
| `packages` | list of strings | Requirements installed in the environment of the package. Pin them (`pypdf==4.3.1`). |

## `[[triggers]]` {#triggers}

A package proposes triggers; installing it does not arm them on its own.

| Key | Type | Required | Meaning |
| --- | --- | --- | --- |
| `id` | string | yes | Identifier, `[a-z0-9_-]+`, unique among the triggers. |
| `target` | string | yes | The agent or the workflow of this package that the trigger starts. |
| `input` | string | no | Message sent to an agent each time the trigger fires. |
| `source` | table | yes | When it fires, see below. |

`[triggers.source]` is tagged by `type`:

| `type` | Keys | Fires |
| --- | --- | --- |
| `cron` | `schedule` | On a cron expression, for example `"0 8 * * MON"`. |
| `interval` | `every` | At a fixed interval: `"30m"`, `"1h"`, `"1d"`. |
| `file_watch` | `recursive` (default `false`) | When a folder changes. The user picks the folder at install time. |
| `webhook` | none | On an HTTP call. The secret is set at install time, never in the manifest. |

## Validation

Beyond the schema, a manifest holds these rules. The first one broken is
reported, and the package is neither built nor installed.

| Rule | Message |
| --- | --- |
| The package declares at least one agent, workflow or tool. | `the package declares no agent, no workflow and no tool` |
| The package name, every identifier and every MCP server name match `[a-z0-9_-]+`. | `` `<field>` value `<value>` must match [a-z0-9_-]+ `` |
| `version` is semver. | `` package version `<value>` is not semver `` |
| `sdk` is a semver requirement. | `` sdk requirement `<value>` is not a semver requirement `` |
| Agents, tools and workflows share one namespace with no duplicate. Trigger identifiers are unique, and so are MCP server names. | `` identifier `<id>` is declared more than once `` |
| A default access `pkg:<package>/<tool>` names a tool of this package. | `` agent `<agent>` grants `<access>`, which is not a tool of package `<package>` `` |
| A trigger targets an agent or a workflow of this package. | `` trigger `<trigger>` targets `<target>`, which is no agent or workflow of the package `` |

`apollia-os package build` adds one check of its own: it refuses a folder whose
Python modules define tools or workflows that the manifest does not describe.

## Complete example

```toml
[package]
name = "invoices"
version = "1.2.0"
publisher = "Example Accounting"
description = "Reads supplier invoices and drafts the reminders."
sdk = ">=0.3, <0.4"

[[agents]]
id = "invoice-clerk"
name = "Invoice clerk"
mission = "Read the supplier invoices of the month and list the late ones."
examples = ["Which invoices are late?", "Draft a reminder for ACME."]

[agents.defaults]
access = ["file_read", "mcp:ledger/lookup", "pkg:invoices/extract_totals"]
sources = ["profile"]
autonomy = "supervised"
planning = "always"

[agents.defaults.model]
model = "qwen3-8b-instruct"
preset = "balanced"

[agents.defaults.model.request]
temperature = 0.2
max_tokens = 2048

[[tools]]
name = "extract_totals"
module = "tools/extract.py"
description = "Extract the total, the due date and the supplier of an invoice."
action = "read_personal"

[tools.inputs]
type = "object"
required = ["path"]

[tools.inputs.properties.path]
type = "string"
description = "Path of the invoice file."

[[workflows]]
name = "monthly-reminders"
module = "workflows/reminders.py"
description = "Find the late invoices and send one reminder per supplier."
action = "send_external"
steps = ["collect", "confirm", "send"]

[workflows.inputs]
type = "object"

[workflows.inputs.properties.month]
type = "string"

[[requires]]
type = "gmail"
reason = "Reminders are sent from the user's mailbox."

[[mcp_servers]]
name = "ledger"
command = "uvx"
args = ["ledger-mcp"]

[pip]
packages = ["pypdf==4.3.1"]

[[triggers]]
id = "first-monday"
target = "monthly-reminders"

[triggers.source]
type = "cron"
schedule = "0 8 1-7 * MON"
```

## Related

- [Package and distribute](/how-to/package-and-distribute-an-agent) for the
  sequence from an empty folder to an installed package.
- [Write a tool](/how-to/write-a-worker) and
  [Write a workflow](/how-to/write-a-director) for the Python the manifest
  describes.
- The `package` commands in the [CLI reference](/reference/cli).
