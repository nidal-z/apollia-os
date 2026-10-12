# Changelog

All notable changes to Apollia OS are documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).
This project follows [Semantic Versioning 2.0.0](https://semver.org/).

## [0.3.0] - Unreleased

The desktop application is named Apollia Desktop from this version on.
Apollia OS keeps naming the engine under it, the `apollia-os` command line and
the SDK.

Changes since `0.2.0-preview`. This release does not chase new capabilities: it
makes the product readable and usable without opening a file of code or of
configuration. An agent becomes a sheet anyone can read, the places where the
AI runs come down to "on this machine" or "on a server", and every access an
agent holds is granted, shown and journaled the same way.

The version is a minor bump with breaking changes, listed with their migration
at the end of this entry. From this version on, tags no longer carry the
`-preview` suffix. The date and the tag land when the release is published.

### Highlights

- **An agent reads without code.** Each agent has a sheet that states its
  mission, its model, the accesses it was granted, the sources it may consult
  and its rights. Everything on the sheet is adjusted in place and goes back to
  the defaults in one gesture.
- **Agents ship as one file.** A `.apo` package carries any combination of
  agents, workflows and Python tools, signed or not. Installing is opening the
  file: a preview shows what the package contains and who published it before
  anything is installed.
- **Run an agent on a whole file, not only in a conversation.** An agent runs
  on every row of a workbook, a CSV file or any other source as a batch that
  can be paused, stopped, retried, survives a restart, and exports its result
  as Excel, CSV or JSON.
- **One question for inference: where does the AI run?** On this machine,
  Apollia picks the engine (llama.cpp, or MLX on Apple Silicon), checks that
  the model fits and loads it on demand. On a server, any OpenAI-compatible
  address is declared, tested and lists its models. Changing the answer
  changes no agent.
- **One list of connections.** Mailboxes, folders, connectors, the built-in
  tools and MCP servers read as one list. A connection is added in two clicks
  from a curated selection and granted to an agent family of action by family
  of action.
- **Free by default, governed when the organisation asks.** An agent calls
  only what it was granted, and asks for consent on first sensitive use. An
  organisation may add a signed policy that fixes settings and decides which
  connections, servers, models and packages are allowed; it shows only when it
  blocks.
- **One developer mode.** A single switch, off by default, replaces the
  Operator and Builder profiles and reveals the technical surfaces: the
  OpenAI-compatible local server, Apollia as an MCP server, the logs and the
  embedded documentation.
- **Apollia keeps running without the window.** The runtime installs as a
  service of the user session, and the desktop attaches to it and drives the
  same agents, tasks, approvals and models.

### Added

#### Agents, workflows and packages

- `.apo` package archives are built, read and verified. A package holds any
  combination of agents, workflows and Python tools; a package with no agent is
  valid.
- A package is previewed before it is installed, and its publisher key can be
  trusted from the preview. Packages and distribution metadata carry a
  verifiable signature.
- An agent created in the interface is exported as a `.apo` package, signed or
  not.
- A package or an agent is uninstalled while keeping or erasing its memory and
  history.
- `@tool` and `@workflow` in the SDK declare the tools and workflows a package
  ships. `python -m apollia describe <folder>` writes them into the package's
  `apollia.toml`, which is read with a strict schema that names the offending
  key by its full path.
- A workflow of a package runs step by step, with nested calls bounded, and
  takes a date, a long text or a file among its inputs.
- An agent without code runs on one request, outside any conversation.
- An agent runs on every item of a source as a batch that survives a restart.
  A batch is paused, stopped and retried, is granted its accesses for the run,
  reads a workbook or a CSV file from a proposed sheet and header row, writes
  its results to a copy of the workbook, and exports them as Excel, CSV or
  JSON.
- A trigger starts an agent or a workflow, set from the agent sheet.
- An agent carries its own autonomy tier, declared by its package and
  overridable by the user.
- An agent is always available: loaded on demand and unloaded when idle.
- Agents and workflows are listed and invoked on the A2A card.
- The guide and the first-launch assistant ship as declarative agents, each
  in an embedded package.
- An agent is created from a short form: what it is for, what it gives back,
  what it never does, what it may consult. The agent workshop, a workflow
  shipped in a third embedded package, asks up to three questions about what
  the form leaves unsaid, each with answers to pick, and writes the sheet for
  review.
- An agent whose Memory is checked keeps notes of its own with `memory_write`
  and finds them again with `memory_search`, across its conversations. An
  agent whose Profile is checked may propose a fact for the profile; each
  proposal is put to the person.
- The steps of a package workflow call the model through `ctx.llm`, each run
  within a budget of calls, and `ctx.llm.complete` says whether the model
  reasons before it answers.
- A help icon on every screen opens the guide on that screen, in a side panel.
- A workflow stops on a question to the user and goes on with the answer.
- Example agents are proposed in the Agents screen and installed on demand:
  web watch, mail triage, document analysis and request qualification.

#### Inference and models

- An OpenAI-compatible server is declared by its address, tested, and lists
  its models with the context window each one really serves.
- The macOS arm64 bundle ships the MLX engine. The format of a model picks the
  local engine: GGUF on llama.cpp, MLX on its own engine.
- Inference engines get an update channel of their own. Engines are downloaded
  on demand, or imported from a disk, against a signed manifest; the
  accelerated NVIDIA build is fetched when a card is detected instead of
  coming in a separate installer.
- Installed models come from an index that covers GGUF and MLX. The curated
  model selection renders offline, with publishers marked, and model cards are
  read signed from the online registry.
- A model shows what it can do and a short list of its variants. A variant
  downloads whole, and its vision projector reaches the engine.
- A model imported from a disk is checked, and an unverified origin is marked.
- Embeddings and dictation models join the registry, sized for the machine and
  verified on download.
- Model presets, the model of each agent and the default model are stored. An
  agent's request settings (temperature, top-p, penalties, token limit) resolve
  over the model, its package and its preset, and reach any engine as well as
  `ctx.llm.complete`.
- A model's default preset carries its load settings to the engine, and the
  reasoning setting of a model is read from its chat template.
- A local model is loaded by hand after an estimate, the last one is reloaded
  in one gesture, the resident models are listed and unloaded, and a model
  unloads itself when idle unless it is pinned.
- The local engine serves several requests at once when the machine has room.
- Embeddings are served by an instance that never evicts the chat model.
- Images attached to a chat message reach a model that reads them.
- Every model call of a chat turn is measured and totalled under its answer.
- One AI screen gathers the loaded models, the connected servers, the
  catalogue and the detail of a model, and says on a gauge whether a model
  fits this computer before it is loaded.
- A model can be loaded with a draft model of its vocabulary.
- The AI screen offers the accelerated engine when an NVIDIA card is detected.

#### Connections and tools

- Every connection of the machine reads as one list, and the built-in tools
  join it as the Apollia source.
- A connection from the curated selection is added in two clicks, with a
  single warning. The public catalogue is queried only on "Explore all", with
  a search and an accurate "official" badge.
- A mailbox is added with its address and an application password, over IMAP,
  read-only.
- A folder of the machine is added as a connection.
- Each connected account publishes its own connector tools.
- Every tool carries its action family. An MCP tool arrives classified by its
  hints or by the curated selection.
- A connection is granted to an agent family by family, and a remote
  connection is marked on the agent sheet.
- Python MCP servers run on the embedded interpreter, with no `uv` to install.
  Node.js is installed through the signed engine manifest when a server needs
  it, and npm servers are pinned and installed once from an integrity lock.
- The Google OAuth client is set on its connection page, with its secret in
  the secret store.
- A "Try" action on every connection, at the end of the addition and from its
  page, opens a conversation limited to the tools of that connection.

#### Sources and memory

- What an agent may consult is a source of one registry, and each source has a
  hybrid search index of its own that follows the files and is dropped with
  its source.
- An agent searches the sources checked for it through a tool it calls;
  nothing is injected into its prompt.
- The text of PDF, Word and PowerPoint documents is read, and a workbook or a
  CSV file is read as cells of text.
- A code folder checked for an agent gives it code context, read on demand.
- The memory of the agents is searched by meaning as well as by words.
- The memory of an agent lives on its sheet and in the profile.
- Several profiles are stored side by side, one by default, each with its own
  facts. The profile of a conversation is chosen in its panel.

#### Governance

- An agent calls only the tools, agents and workflows its access list grants.
  The Python tools of a package are granted like any other tool.
- Agents state a governance contract, and every access change is journaled.
- A consent request has three answers, and a write asks on the same card. An
  "always" holds for one agent and one action family.
- Every decision waiting for the user is listed and answered in one place.
- A signed organisation policy is read once at boot. It governs connections,
  inference servers, model imports, the model registry, the packages that may
  install and the settings it fixes. A setting the policy imposes reads as
  locked. The organisation key, the policy file and its signature are created
  and checked from the command line, and a screen shows what is allowed and
  why.
- The journal records consents, refusals, rules, batches and policy.

#### Developer mode and local server

- A compatible server listens on its own port, off by default, switched on and
  off while the runtime runs. It serves the loaded local models in the OpenAI
  and Anthropic formats, loads models on demand, answers CORS preflights when
  asked to, keeps and streams its recent logs, and can be opened to the local
  network.
- The compatible server gets named tokens, shown once and revocable.
- Apollia acts as an MCP server, and an MCP client reaches only the agents and
  workflows the user exposes.
- The documentation site is embedded in the desktop and readable offline.
- The desktop keeps a log file, and the engine log level is set from the
  settings.
- The System page shows the engines and the hardware.
- The configuration file can be edited in the settings.
- A Developer screen gathers the server, its tokens, call examples to copy,
  the logs, the HTTP reference of the native API and the documentation.

#### Runtime and daemon

- `apollia-os service install`, `uninstall` and `status` run the runtime at
  logon without administrator rights, on macOS, Linux and Windows.
- The desktop decides at startup whether to start its own runtime, attach to a
  running one, or refuse with a reason. Attached, it serves the same surfaces:
  agents, tasks, approvals, questions, conversations, models, connections,
  memory, dictation, onboarding and packages.
- Every write of `apollia.toml` goes through one writer, served under
  `/api/v1/config`. A saved or hand-edited file applies without a restart, or
  says that it waits for one.
- `[api].port` sets the port of the native API, for the daemon and the
  desktop.
- The update channel chooses between stable and preview.

#### Chat and settings

- The chat picks its agent and its model from one selector.
- The chat appearance settings set the font, the width, the density and the
  reasoning card.
- The onboarding reminders are listed in the settings, and each one can be
  shown again.
- The models folder is chosen in General, and the Hugging Face token is kept
  in the secret store.
- The downloads panel opens when a download starts.
- The chat shows up to four conversations side by side.
- A conversation carries its own settings and notes, shows its prompt
  template, and can keep the reasoning of its earlier answers.
- An answer is continued, branched off, rewritten or removed from the row
  under it.
- The first launch can be replayed from the danger zone of the settings.

### Changed

- The built-in chat becomes the Apollia agent: an agent like any other, with a
  sheet, that cannot be uninstalled. Its former settings (system prompt,
  allowed tools, backend) are carried over at startup and journaled.
- Every agent runs through one executor, whether it comes from a package, from
  the interface or from the chat.
- An agent's model is resolved at each call, with a signalled fallback.
- A new agent created without code is granted the reading family by default,
  and nothing more.
- The file tools reach only the folders granted to the agent. The home folder,
  the data directory and the working volume are no longer roots.
- One governance actor decides every tool call by action family.
- Ollama is reached as any OpenAI-compatible server. Its context window is
  read, never set by Apollia.
- One memory estimator decides whether a model fits, and a local model that
  would saturate the machine is refused before launch.
- Model search is bounded to official publishers and recognised converters.
- Every surface (CLI, daemon, desktop) reads and writes the one `apollia.toml`
  under `~/.apollia`, written with its documentation on a first start. The
  file has one strict schema: an invalid file is refused at the line of its
  fault, and every key has an effect or is withdrawn.
- The settings open in a window above the current screen and are ranked by use
  in two groups, Settings and System.
- The OAuth integrations leave the settings: each client and account is set on
  the page of its connection.
- MCP is named in developer mode only; elsewhere a server is a connection.
- Transcriptions and Observability move to a developer group of the sidebar,
  and the full Memory screen shows in developer mode.
- The onboarding goes from the welcome straight to the AI setup.
- Tool steps and reasoning are shown plainly in the chat, each tool card is
  redone, and the code card shows the whole command.
- The interface texts follow one vocabulary, held free of banned technical
  terms.
- Cancelling a task stops its execution instead of relabelling it.
- `GET /api/v1/health` answers without a token on the TCP listener and reveals
  the product marker and the version, nothing else.
- The sidebar has two ranks. The Home screen and the task list screen are
  retired: the application opens on the chat, and a run is read where its
  agent is.
- The journal is the one audit register. Its screen exports it and verifies
  its integrity.
- A step of a plan is decided by governance, like any other tool call.

### Removed

- The dedicated cloud LLM providers (`anthropic`, `mistral`, and `openai`
  pointed at the OpenAI cloud), and the Anthropic and Vertex clients.
- The Projects screen, the `apollia-os project` noun, the project routes of
  the API and the "always for this project" consent scope.
- The Operator and Builder selectors, in the top bar, in Appearance and in the
  onboarding.
- The guided tours and the "Getting started" band.
- The native tools that let an agent write its own permission rules.
- `ctx.user_context` in the SDK.
- The `dangerous` flag of `@tool`: sensitivity follows the action family.
- The configuration keys that nothing read, among them `[llm.runner]`,
  `[llm.vertex]`, the pricing overrides and the cost alert threshold with its
  CLI flags and its strip on the LLM page, `[filesystem]` and
  `[chat] default_workspace`.
- The native Ollama client and the `num_ctx` it sent on each call.
- The separate CUDA installers. Their update manifest is still published, as
  a copy of the standard one, so that the copies already installed keep
  updating.
- Every way of writing an agent in Python, in the SDK and in the runtime: an
  agent is a record of a package, and code brings tools and workflows.
- The approval level of an MCP server: an MCP tool is decided on its action
  family, like any other tool.
- The audit stores and views the journal made redundant.

### Fixed

- In plan mode, a local model could repeat itself inside a text field of the
  plan it was proposing and never finish: the conversation showed it thinking
  for as long as it was left. The fields of a plan step are described and
  bounded, and a short length bound of a tool schema now reaches the grammar of
  the local engine, which ends the string.
- A turn that ends by proposing a plan no longer reads as an answer that could
  not be produced.
- The window that launches a batch kept none of what was filled in: it reset
  itself while open.
- A call that asked the local engine for a structured answer never came back
  on a model whose template opens on a channel marker: the schema was sent as a
  raw grammar, which binds the first token. It now travels as
  `response_format`, so the model reasons first and the constraint applies to
  its answer. `apollia-os do` maps a request through a schema for the same
  reason, and works on every backend.
- A standing consent on a code executor covered every later call of the same
  exchange. It now covers the current call only.
- Cancelling a task left its Python code running and writing. The running
  dispatch is now cancelled, and the task is reported cancelled once it has
  stopped.
- A tool disabled on an MCP server or a connector was still offered to the
  agent.
- A local preset that fixed the number of slots lost the context window of its
  backend.
- A key set by one tool could be read by no engine, because the CLI, the
  daemon and the desktop resolved the configuration file in different places.
- Launching the desktop while a daemon was serving removed the daemon's socket
  and ended without a word.
- With file-based secret storage, the desktop wrote MCP secrets to the OS
  keyring while the runtime looked for them in the encrypted files.
- Keys that had no reader on the daemon now reach it: `[llm.observability]
  debug_log_prompt`, `[tools] max_output_chars`, `[observability]` and
  `[runtime] startup_timeout_secs`.

### Security

- An agent can no longer grant itself a permission: the tools that wrote
  permission rules are gone, and rules stay the user's, from the desktop and
  `apollia-os permissions`.
- An agent calls only the tools, agents and workflows it was granted, and its
  file tools reach only the folders granted to it.
- An unreadable governance database or a refused organisation policy fails
  closed: every sensitive tool call and every new connection is refused,
  reading continues, and the window says why.
- A downloaded model is kept only when its SHA-256 matches.
- Engines, Node.js and packages are installed only after a signature and a
  digest hold. An older manifest than the last one accepted is refused.
- Curated Python MCP servers install from a hash lock of their whole
  dependency closure, and npm servers from an integrity lock; one altered
  digest refuses the installation.
- Every connection secret goes through one secret store. A backend key given
  in clear and the Hugging Face token are moved to it.
- The profile is no longer pushed to agents through the context. An agent
  reads it only when its profile source is checked, and each read is traced.
- The compatible server listens on the loopback interface unless the user
  opens it to the local network; agents and workflows called from the network
  require a token. The native API keeps refusing a non-loopback bind without a
  token.
- The organisation policy, not the user profile, forbids cloud connections.
- Images attached to a message never reach a trace or a log.

### Breaking changes and migration

#### Cloud LLM providers are removed

`--provider anthropic` and `--provider mistral` are refused, and `openai` now
means any OpenAI-compatible server at an address you designate. An `openai`
backend with no address is refused instead of defaulting to the OpenAI cloud.

On upgrade, every backend that reaches a cloud API is deleted: every
`anthropic` or `mistral` backend, and every `openai` backend with no address or
on `api.openai.com`, `api.mistral.ai` or `api.anthropic.com`. If the default
backend was one of them, the default moves to the local engine when there is
one. Self-hosted `openai` backends are kept unchanged.

To migrate: declare an OpenAI-compatible server instead. To keep reaching a
hosted model, put a gateway you operate (LiteLLM is the recommended one) in
front of it and declare the gateway's address. A file that still carries
`[llm.vertex]` loads with a warning and the section does nothing.

#### Projects become sources

The Projects screen and the project concept are gone. At the first start, each
former project is converted: its documents become file sources grouped under
the project's name, its instructions become a searchable file of the same
group (searched, never injected into a prompt), its working folder becomes a
folder connection granted to no agent, its memory spaces are merged into the
agent's memory, and its conversations keep their agent. The sources are
checked for the Apollia agent and for each agent the project linked.

Nothing is dropped in silence: a document or a folder that is gone or
unreadable, or an agent that is no longer installed, is listed in a report,
shown once in the desktop and readable with `apollia-os source report`.

To migrate: read the report, then grant the imported folder connections to the
agents that need them, on their sheets. `apollia-os project`, the `project`
field of `POST /api/v1/sessions`, `PUT /api/v1/sessions/{id}/project` and
`GET /api/v1/projects/{id}/sessions` no longer exist; scripts that used them
move to the source commands and routes. A consent given "always for this
project" no longer holds beyond its session.

#### Operator and Builder are replaced by the developer mode

There is one switch, in Settings > Developer, off by default, stored as
`[desktop] developer_mode`. A former Builder choice turns the mode on at the
first launch. The onboarding no longer asks for a usage profile, and
`PUT /api/v1/onboarding/profile` is removed. Transcriptions, Observability, the
full Memory screen, the name "MCP" and the local server show in developer mode
only.

#### File access and standing consents

An agent with no folder granted no longer reaches the home folder: its file
tools refuse with a message that says how to grant one. The folders named by
`[filesystem] trusted_paths` and `[chat] default_workspace` are added once as
folder connections, granted to no agent (the home folder and the data
directory excepted). To migrate: grant a folder connection on the agent's
sheet, for reading or for writing.

Permission rules lose the project and session scopes: a rule targets one agent
or every agent. Rules written before are upgraded once at startup and each
conversion is journaled. A blanket allow becomes a standing consent of its
agent for one action family; a standing allow on a code executor or on a tool
of unknown family is removed and will be asked again.

#### SDK contract

- `ctx.user_context` no longer exists. Read the profile through `ctx.profile`,
  which answers only when the agent's profile source is checked.
- `@tool` no longer takes `dangerous`; declare the action family that says
  what the tool does.
- A package manifest (`apollia.toml` of a package) refuses any key it does not
  know. Fix the key the error names.
- An agent's tools, agents and workflows are an access list: a tool the agent
  was not granted is not offered to it. Declare the defaults in the package,
  or grant them on the agent sheet.
- Rust embedders: `RuntimeContext::new_with_llm` takes one argument less, and
  `AIPTask.project_id` is removed.
- An agent is no longer written in Python. It is a record of a package
  (`[[agents]]` in `apollia.toml`), and code brings tools and workflows,
  declared with `@tool` and `@workflow`. An existing Python agent is rewritten
  as a package: its mission and accesses as a record, its code as tools and
  workflows. The "Your first agent" tutorial builds one end to end.

#### Configuration file and withdrawn keys

The only configuration file read is `~/.apollia/apollia.toml` (or an explicit
`--file` on the config commands, or `--config` on `apollia-os start`). A file
left in the working directory, under `$XDG_CONFIG_HOME/apollia` or under
`~/.config/apollia` is neither read nor imported: `apollia-os doctor` names it
with the command that moves it.

The file has a strict schema. The daemon refuses to start on an unknown key or
an invalid value and prints each fault at its line; the desktop starts on the
defaults and reports the fault. `apollia-os config validate` lists every
fault.

Withdrawn keys still load, with a `config.key.retired` warning, and do
nothing. Remove them:

- `[llm.runner]`, `[llm.vertex]`, `[llm.pricing_overrides]`,
  `llm.cost_alert_threshold_usd`
- `llm.observability.log_token_usage`, `log_latency`, `log_cost` (tokens and
  latency are always logged)
- `tools.file_path_extraction_pattern`
- `[filesystem]` and `chat.default_workspace` (replaced by folder connections)
- `[memory]`, `[budget]`, `[a2a]`, `[oria]`, `[registry]`, `[permissions]`
- `[triggers]`, `[notifications]`, `[stt]` (managed from the desktop or the
  API)
- `[ui] link_preview_enabled` moves to `[desktop] link_preview_enabled`; the
  former key is still read while the new one is absent.

#### CUDA installers

The separate `-cuda` installers are no longer built. The standard installer
offers the accelerated NVIDIA engine on the AI screen when a card is detected,
downloads it against the signed engine manifest, and can import it from a disk
on a machine with no network.

An installation made from a `-cuda` installer keeps updating: its manifest,
`latest-cuda.json`, is still published as a copy of `latest.json` and brings
it to the standard installer of its system, which then offers the accelerated
engine. Nothing has to be reinstalled by hand.

## [0.2.0-preview]

Second preview, superseded by 0.3.0. Changes since `0.1.0-preview`:
schema-constrained LLM output, typed human-in-the-loop pauses, and the defects
found by driving those paths against a real daemon.

The version is a minor bump: the preview adds public surface (`schema=` on
`ctx.llm`, the typed pause and its answer, new API parameters and CLI flags) and
moves `hitl.db` to schema v2. Nothing is removed from the public API.

### Added

- `ctx.llm.complete(..., schema=...)` and `ctx.llm.chat(..., schema=...)`
  constrain generation, validate the answer against the same JSON Schema, and
  resolve to the validated value as a native Python object. A failure raises
  `apollia.errors.StructuredOutputError`, which carries the JSON path of the
  offending node and a `kind`: the schema cannot become a constraint, the
  backend has no structured output mode, or the answer is refused. Nothing is
  retried. A constrained call charges one step against the `StepBudget`, and the
  journal records that generation was constrained and a fingerprint of the
  schema, never the schema. `MockLlmProxy` in `apollia.testing` accepts
  `schema=` and returns the value, and gains `stream`.
- In `apollia-llm`, `json_schema_to_gbnf`, `schema_validate` and
  `CompletionRequest.response_schema`. The embedded llama-server receives GBNF,
  every other OpenAI-compatible provider receives `response_format`, and a
  backend with no structured mode (Anthropic, Vertex) answers
  `StructuredOutputUnavailable` instead of dropping the constraint in silence.
  On a reasoning model a constrained call reads `reasoning_content` and
  `content` as one channel.
- Typed pauses. An agent pauses on a question (`genre`: `choix`, `source`,
  `seuil`, `definition`, `confirmation`, with propositions) or on an approval of
  a named `geste` with its `risque` (`apollia_core::HitlPayload`). It reads the
  answer through `ctx.is_resumed` and `ctx.input_response` (`approved`,
  `reason`, `answer`, `payload`, `context`) and can pause several times in one
  task. `NeedHumanInput` and `AIPResult.input_required` take a `payload` typed
  by `apollia.hitl`. An invalid payload fails the task with
  `INVALID_INPUT_PAYLOAD`.
- `POST /api/v1/tasks/{id}/resume` takes `answer`, checked against the pending
  payload (422 `INVALID_ANSWER` leaves the task paused). `POST /api/v1/tasks`
  takes an optional `skill_id`. `GET /api/v1/tasks` finds `input_required` tasks
  and lists agent, skill, `created_at`, prompt and payload, and
  `GET /api/v1/approvals/pending` carries `payload` and `skill_id`.
  `just test-hitl` proves the path on a real daemon.
- A structured MCP tool result reaches the agent with a `structured` key holding
  the object as the server built it; `content` is unchanged.
- `GET /api/v1/audit/journal?agents=<name>,<name>` and `apollia-os audit journal
  --agent` narrow the journal to the runs of those agents.
- `A2AInvocationResult` carries `run_id`, the key the chained journal keeps an
  invocation under.
- `apollia-os mcp add` and `mcp update` take a repeatable `--arg` for the
  command's arguments, and `--transport`. A URL alone means `streamable-http`.
- Desktop: the operator chooses the model download folder, in onboarding (LLM
  and STT) and in the Model Hub, for machines whose default `~/.apollia/models`
  drive has no room.
- Model recommendations measured against the machine. Onboarding, `GET
  /api/v1/llm/recommend` and `apollia-os model recommend` rank live
  HuggingFace files instead of a hardcoded list. A curated table
  (`families.toml`) names the generations, which one supersedes which, and a
  capability score per size; a newer generation is never ranked below one it
  supersedes. Each candidate's GGUF header is read by range request (a few
  hundred KiB, stopping at the end of the header) to check that the bundled
  llama.cpp loads its architecture and pre-tokenizer, from tables generated
  against the pinned engine tag (`scripts/gen_llama_support.py --check` runs as
  a pre-commit hook). Memory is estimated per layer, the way the engine
  allocates it: sliding-window, hybrid and recurrent layers, per-layer KV heads.
  The planner searches quantisations (not only `Q4_K_M`), judges a mixture of
  experts by the share it reads per token, and ranks by estimated throughput on
  unified memory, a discrete card with partial offload, or the processor alone.
- The embedded engine is launched with an `-ngl` planned from the same model on
  a discrete card, so a model larger than the card runs split instead of failing
  to load. `APOLLIA_LLAMA_N_GPU_LAYERS` still wins.
- The context window is chosen in onboarding and in the backend settings
  (**Context window**), stored as `config_json.context_window`, and honoured
  everywhere: the embedded engine launches each model with that window (one
  process per model and window), Ollama is asked for it on every call, and
  compaction and the context gauge are sized against it.
  `APOLLIA_LLAMA_N_CTX` still wins over a backend's value. A window larger than
  the model's training length (read from its GGUF header, or from Ollama's
  `/api/show`) is capped at it, and the recommendation row says so.
- A tool call whose arguments do not fit the tool's schema (a missing required
  parameter, a misnamed one, a string for a list, a value outside an enum) does
  not run: the model gets every problem at once and the tool's usage card, its
  parameters with types, allowed values and descriptions. The check tolerates
  what executors accept (`"30"` for an integer). A call repeated verbatim a
  third time in one turn is answered with a reminder instead of running.
- A native Ollama client (`/api/chat`) replaces the OpenAI-compatible path for
  Ollama backends. It sends `options.num_ctx`, separates a thinking model's
  reasoning, and carries tool calls with object arguments. A stored `.../v1`
  endpoint keeps working.

### Changed

- The interpreter agents run on is the bundled one on all three systems. A
  system interpreter is used only when the operator names it in
  `[tools] python_interpreter`, checked by `apollia-os config set` and by the
  Advanced settings page: it must exist, start and report the bundled minor
  version, or it is dropped with a warning.
- A declined pause that carries a payload resumes the agent with `approved:
  false`; a prompt-only pause keeps failing with `REJECTED`. `hitl.db` goes to
  schema v2.
- The Google connector executors moved from `apollia-desktop` to
  `apollia-runtime`, next to the Microsoft ones; the desktop keeps the consent
  half that opens a browser.
- The Python API client covers every operation the spec declares, including
  `list_audit_journal`, the registry reads and the STT reload. `regen.sh` pins
  both generator versions and refuses to run without `ruff`.
- Documentation: the human-in-the-loop how-to (both locales) and the SDK
  reference describe the typed pause; the MCP pages and the connection wizard
  say the per-server approval level governs agent tasks, not chat.
- The file-watch trigger tests wait for the watch to be registered instead of a
  fixed delay.
- The recommendation rows name the memory they are measured against: a model
  that runs entirely on a discrete card is graded against the card's video
  memory, a model too large for it is reported as a split between video memory
  and system RAM (never as a comfortable fit), and the line under the list names
  RAM and video memory separately. The memory figure includes the engine's own
  buffers. A model estimated below four tokens per second is withheld while a
  faster one fits. The "no pre-tokenizer" caveat is raised only for BPE
  vocabularies, the one kind the engine reads the field for.
- Connector tools (Google, Microsoft) are advertised to the chat model only
  while that connector has an account, checked on every turn. On a fresh
  session they were about 9000 of the 13400 tokens of a first call.
- The embedded llama-server runs with `--reasoning-format auto`, so a template
  that opens its own reasoning channel (Gemma 4) is parsed by the engine.
- Choosing another model during onboarding updates the `local` backend instead
  of leaving the first one wired.
- The assisted-tier system prompt no longer carries the Google chaining rules on
  every call; the Google tool descriptions already hold them, and are sent only
  while an account is connected.
- CI and CodeQL run on pushes to `develop` too, and CodeQL can be dispatched by
  hand. The Rust test job frees the runner's unused SDKs and builds with line
  tables only, after it ran out of disk. Release build artifacts are kept seven
  days instead of thirty, which had filled the artifact quota.
- `rustls` 0.23.45 (RUSTSEC-2026-0285) and `devalue` 5.9.4 (GHSA-9rgm-9g3h-6x36).
- The three runtime tests that exercise a Unix socket are Unix-only, so the
  runtime test suite compiles on Windows.

### Fixed

- Apollia Desktop failed to start on Windows with `python313.dll` not found when
  another Python was installed and the user had no administrator rights. The DLL
  is now staged beside the executable. Not declared fixed until an installation
  on such a machine confirms it.
- A configured system proxy no longer swallows requests to loopback backends.
  The embedded llama-server, a local Ollama, the STT runner and local MCP servers
  failed with a silent transport error on networks that require a proxy, and the
  llama-server startup health check went through the proxy too, so the engine
  reported `did not become healthy within 180s` while it was running. On
  Windows the proxy bypass list usually names `localhost` and not `127.0.0.1`,
  which is why one spelling worked and the other timed out.
- The name `localhost` in an endpoint the operator configured resolves to IPv4
  first, then IPv6, instead of following the OS resolver order. On Windows a
  local Ollama, which binds `127.0.0.1`, was unreachable under
  `http://localhost:11434`. The Ollama default is now `http://127.0.0.1:11434/v1`
  in the CLI, the router and the desktop form.
- An MCP server declared `requires_approval` gated no call on the task path; it
  now pauses on an approval naming `<server>/<tool>`, runs once when approved and
  raises `ToolApprovalDenied` when declined. An agent requiring a tool of such a
  server now installs (`POST /api/v1/agents` answered 400) and every run of it is
  gated, including the CLI chat-agent runner.
- A task with no `skill_id` reached only `@on_message`, so an agent whose one
  entry point is a `@skill` failed `NO_HANDLER` when a trigger fired it. A lone
  skill is now reached.
- `AIPResult.input_required` returned from a skill paused nothing.
- `POST /api/v1/a2a/invoke` answered `result.task_id: ""`.
- A journaled `ctx.llm` call and the chat agent loop named
  `<resolved-by-router>` or an empty model instead of the model that answered.
- A Google connector declared by an installed agent (`gmail.send`) answered
  `UnknownTool` under `apollia-os start`.
- The block that injects past session summaries into the first message of a free
  chat is framed as background not to act on, after a local model re-issued a
  stale tool call from an unrelated session.
- The onboarding recommendation step loaded forever: an effect re-ran the full
  HuggingFace resolution each time the previous one finished. It now runs once,
  under a 90 s deadline, and the answer is cached for ten minutes.
- An Ollama backend ran at Ollama's own default window (4096 tokens on a 16 GB
  card) whatever Apollia believed: the OpenAI-compatible endpoint has no field
  for it, and Ollama cut longer prompts from the front without an error. The
  model lost its system prompt, its tools and then the conversation, and
  Apollia compacted on every turn.
- The settings dialog's context size was written to a key nothing read, and the
  HuggingFace auto-fill put `max_new_tokens` in it. The field is now the context
  window, and a row saved with the old key is honoured.
- Gemma 4 reasoning (`<|channel>thought ... <channel|>`) was shown as answer
  text, and an unmatched closing marker leaked into the chat.
- Compaction could summarise away the turn being answered, injected its summary
  as a fake user message, and kept a thinking model's reasoning in the summary.
  The current turn is protected, the summary goes to the system prompt as
  background, and reasoning is stripped.
- The context gauge summed every call of a turn; it now shows the last call's
  tokens against the window.
- Parsing an automation request could panic on text holding a character whose
  lowercase form is longer (`İ`), found by the `parse_automation` fuzz target.
- `apollia-os model recommend` had no parsing test and no end-to-end invocation.

## [0.1.0-preview]

First public preview, superseded by 0.3.0. Local-first Rust runtime for
autonomous AI agents.

The whole tree carries this version: the twenty-one Rust crates, the Python
SDK, the built-in agents, and the desktop bundle. The desktop bundle files are
stamped `0.1.0-1` instead, on all three platforms (`tauri.conf.json` names the
`.dmg`, the `.deb`, the `.AppImage` and the Windows installers alike), because
the MSI product version has to be numeric.

The `Changed` and `Fixed` entries below describe work done inside the preview
cycle, before anything was published.

### Added

**Runtime core**

- Tokio-based agent runtime with `EventBus`, `AgentRegistry`, `TaskRouter`,
  `ExecutionCoordinator`, `Supervisor`, and `ShutdownController` (graceful
  drain).
- `APIServer` (axum) on Unix socket and TCP port 7771.
- REST: `POST /tasks`, `GET /tasks/:id`, `GET /agents`, `POST /agents`, and
  related endpoints.
- Server-sent events at `GET /tasks/:id/stream` for real-time progress.
- ORIA orchestration engine: observer-driven classification (`Direct` vs
  `Orchestrated`), tri-dimensional step budget (steps, tool calls, wall
  clock), resilience layer with circuit breakers and exponential retry.

**PyO3 bridge (AIP)**

- Native async Rust to Python bridge via `pyo3` and `pyo3-async-runtimes`.
- Decorator-based agent contract: a class carrying `@agent` plus at least one
  `@skill` or `@on_message` method. The bridge refuses an object without
  `__apollia_dispatch__`.
- `ToolProxy`, `MemoryInterface`, and `AIPBridge` exposing the Rust runtime
  to Python.
- `AgentLoader` trait decoupling the runtime from PyO3.

**LLM**

- LLM router serving local GGUF models through an embedded `llama-server`
  (upstream llama.cpp), alongside Anthropic and OpenAI providers.
- Local speech to text through whisper in the `apollia-runner` sidecar, on
  the card through Metal on macOS and Vulkan on Windows, on the processor
  elsewhere; the daemon picks the runner its bundle carries.
- Meta planner for next-step suggestions, plan caching, and orchestrated
  decision points.
- Token budget tracking per session with hard and soft limits.

**Tools**

- Tool registry actor with at-startup resolution of required and optional
  tools.
- Native tools: `bash_executor` (Linux namespaces, macOS dev mode),
  `python_executor` (per-agent venv isolation), file IO suite
  (`file_read`, `file_write`, `file_edit`, `file_glob`, `file_grep`),
  `web_search`, `web_read`, `http_fetch`, `memory_search`, `notebook_*`.
- Audit trail with SQLite WAL, fire-and-forget logging, SHA-2 input hashes.
- HITL approval store with permission rules scoped to session, project, or
  agent.

**Memory**

- Multi-layer memory: episodic (events + importance), semantic (facts +
  confidence), procedural (procedures + triggers).
- SQLite with FTS5 and BM25 for full-text search across layers.
- Namespace isolation, lazy store opening, access-level enforcement.

**MCP**

- Model Context Protocol client: 18-entry catalog of curated servers,
  stdio and HTTP transports. Inbound MCP server (stdio) exposes native tools.
- Custom MCP server installation through the desktop UI or CLI.
- OAuth orchestration for MCP servers that require it.

**Connectors**

- Google: Gmail, Calendar, Drive (workspace), Sheets, Tasks, Docs, Forms,
  Slides, YouTube.
- Microsoft: Outlook mail, Outlook calendar, OneDrive.

**Triggers and notifications**

- Trigger engine: cron, interval, oneshot, file watch, webhook sources.
- Notification engine with desktop and webhook channels, per-budget alerts.

**Desktop**

- Tauri 2 application sharing the runtime and Python interpreter with the
  CLI.
- Svelte 5 frontend with TypeScript, Vite, Tailwind, Bits UI.
- Built-in onboarding agent, and a companion that answers from the
  documentation shipped with the binary.

**CLI**

- `apollia-os` binary at near-parity with the Desktop (40+ subcommands).
- `--json` and `--quiet` global flags on every command.
- POSIX exit codes: 0 success, 1 usage, 2 runtime, 3 task failed, 4 timeout,
  5 canceled.
- End-to-end smoke suite (`tests/cli/cli-e2e.sh`): 296 assertions across the
  OFFLINE and RUNTIME tracks (167 and 129), plus 7 structural captures in the
  LLM CAPTURE track, which is gated on a real model.

**SDK**

- Python `apollia` package: `@agent`, `@skill`, `@on_message`,
  `@orchestrated` decorators.
- Context Protocols: `ctx.llm`, `ctx.memory`, `ctx.a2a`, `ctx.tools`,
  `ctx.notify`, `ctx.logger`.
- Testing helpers and mock proxies for unit testing agents in isolation.

**Build and packaging**

- Workspace MSRV: Rust 1.89.
- Cross-compilation hints in `Cross.toml`.
- `deny.toml` for license, banned crates, and advisory checks.
- `cargo deny` green at release time (advisories, bans, licenses, sources),
  on every pull request and again in the weekly deep audit.

**Documentation**

- Public documentation site (Docusaurus) with a capstone end-to-end walkthrough.
- Operator help corpus.
- `AGENTS.md` rulebook and the `docs/agents/` engineering corpus.

**Verification**

- Concurrency verification: abstract Loom models of the runtime actor
  algorithms in the standalone `apollia-loom-models` crate, and a Miri
  undefined-behavior suite over the FFI-adjacent pure helpers in `apollia-aip`.
  Both are dev-only (no runtime dependency) and run as advisory nightly CI jobs.
- Formal verification: bounded Kani symbolic proofs of the two cardinal
  invariants, the non-bypassable `StepBudget` (`apollia-oria`) and the mailbox
  lease/ack fence (`apollia-runtime`), each with an in-tree proptest mirror that
  runs under `cargo test`. Dev-only; an advisory nightly `kani` CI job runs the
  proofs (Kani links its own toolchain via rustup, so it is CI-only).

### Changed

- Timing-dependent runtime tests (registry actor-death, router degraded-agent
  event, router cancel-vs-late-completion) now use event-driven awaits and
  bounded poll-until-condition instead of fixed sleeps, removing flakiness.

### Fixed

- Mailbox lease exclusivity: `ack`/`nack` are now fenced on the leasing
  `run_id` via a new `lease_owner` column, so a stale consumer whose lease was
  re-leased to another run can no longer delete or requeue the message the new
  owner is processing.
- `StepBudget` step/tool-call increment used a checked `+` that could panic in
  debug at `u32::MAX`; it now saturates.

### Security

- `cargo deny check advisories` green at release time. There is no
  `cargo audit` job: `cargo deny` is the single advisory gate.
- Twenty-two advisories are suspended in `deny.toml`, each with its lift
  condition. Twenty are unmaintained-crate notices on transitive dependencies;
  two are pyo3 0.24 soundness advisories, RUSTSEC-2026-0176 (out-of-bounds
  read) and RUSTSEC-2026-0177 (missing `Sync` bound), whose fix lands in
  pyo3 0.29. Their exploit paths (`new_closure`, the `nth` iterator adapter)
  are on no Apollia code path. Lift condition: the pyo3 0.29 migration.
- Private vulnerability reporting via GitHub Security Advisories.

[0.3.0]: https://github.com/Apollia-OS/apollia-os/compare/v0.2.0-preview...main
[0.2.0-preview]: https://github.com/Apollia-OS/apollia-os/compare/v0.1.0-preview...v0.2.0-preview
[0.1.0-preview]: https://github.com/Apollia-OS/apollia-os/releases/tag/v0.1.0-preview
