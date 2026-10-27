# Changelog

All notable changes to Apollia OS are documented in this file.

The format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).
This project follows [Semantic Versioning 2.0.0](https://semver.org/).

## [0.3.1] - Unreleased

Changes since `0.3.0`. Connections are rebuilt from the screen to the engine:
one page shows every connection with its true state, one window adds any of
them, and a connection is checked, repaired, paused or removed in one gesture
from its card. Google Workspace works as in 0.3.0; only the place where its
OAuth client is entered has moved, into developer mode.

### Highlights

- **One page for every connection.** Accounts, mailboxes, folders and
  services share one list and one card. Each connection says whether it works,
  why not, and the one action that repairs it; what needs repair comes first.
- **One window to add a connection.** A single search answers from Apollia's
  selection, this computer, the public catalogue and a service's address, and
  every addition goes through the same three steps: connect it, choose which
  agents may use it, try it.
- **Pause and remove for real.** A pause keeps every access and runs nothing
  until it is resumed; a removal says what goes with the connection before it
  erases it.
- **Services start when they are needed.** MCP servers start at their first
  call and stop after a while without one, and Apollia's launch no longer waits
  for any of them.
- **Microsoft asks for less.** Apollia asks for your own OneDrive files only,
  reads only the folders and files you choose, and guides a work account whose
  organisation requires an administrator's approval.

### Added

#### The Connections page

- A list and a card side by side, with a search and a filter by kind
  (accounts, mailboxes, services, folders). The page keeps its selection, its
  search and its filter when you leave it and come back. Each account is a line
  of its own, and a connection a package installed says which package brought
  it.
- Five states: **Working**, **Needs repair** with its reason in plain words,
  **Paused**, **Blocked by your organisation**, and **Not checked yet** for a
  connection nothing has tried, listed with the others rather than at the top.
  A state found by a check, a use or a sign-in says when it was found;
  **Paused** and **Not checked yet** carry no time. A state is kept across restarts and reaches the page as soon
  as it changes.
- **Check** tries a connection of any kind on request: an account signs in, a
  mailbox opens, a folder is looked for, a service is started and asked for its
  tools.
- The one action that repairs: **Sign in again**, **Enter the password**,
  **Choose the folder**, **Start again** or **See the rule**. A folder chosen
  again keeps its connection and its agents' accesses, and is searched in its
  new content at once: nothing from the former folder comes back in an answer,
  even while the new one is read.
- **Pause** and **Resume** on every kind of connection. While paused, its
  agents keep their accesses, a call of one of its tools is refused with the
  reason and written in the Activity log, a mailbox's as any other, a folder
  is neither read, listed, written nor searched (a file tool or the context of
  a code folder in it is refused the same way, and the search says it is
  paused), the agent's sheet marks the access **Paused**, and a service is
  stopped and never started again, at launch or on request. The pause holds across a restart. A resume shows on
  the list and the card at once.
- **Remove** lists what goes with the connection, its settings, the files
  installed for it, its sign-in, its secrets, the tools each agent held through
  it, for a folder its source and index, and for a Microsoft account the
  OneDrive folders and files chosen for it, then erases them and names
  anything it could not erase.
- **Agents that use it** on the card, and **Give to an agent** to give a
  connection to several agents at once, each gift written in the Activity log.
  An agent on the default access keeps it when it is given a connection: the
  connection is added on top, marked **On top** on its sheet.
- **Activity over 7 days** on the card, when it was last used and how many
  actions each agent took with it, and **See in the journal**, which opens the
  Activity log on that connection alone. A mailbox shares its tools with every
  other mailbox, so its activity is not counted on its own.
- The tools of a service added outside Apollia's selection can be given
  another action family with **Change**. The choice prevails over what the
  server declares, is kept across restarts until the service is removed, and
  is written in the Activity log with the family before and after.
- The technical detail of a connection, its identifier, origin and raw
  reason, shows in developer mode only.

#### Adding a connection

- One window with a single search, answered in four groups: **Offered by
  Apollia**, **On this computer** (a folder, a mailbox), **Public catalogue**,
  asked only on **Explore all** and marked **Not verified**, and **Connect a
  service by its address**. The card of each entry says what it asks before
  anything is added.
- One flow for every kind, in three steps: **Connect**, **Who can use it**,
  where Apollia, the agent of the Chat, comes checked and the other agents can
  be checked, and **Try it**. Cancelling before the connection exists brings
  the search back.
- A service reached by its address takes a name, the address and, when it
  asks for one, a key. Its transport, a local command, headers and variables
  are in developer mode.
- Apollia's selection is read from a signed online index when the window
  opens, so an entry can be added or corrected without a new version. An index
  whose signature does not hold is refused, and the copy kept on the computer
  or the selection built into the application is shown instead.
  `mcp.catalogue_enabled = false` turns the online index off, and
  `mcp.catalogue_url` and `mcp.catalogue_mirror_url` name where it is read.
- The selection reads in the language of the interface. A service added from
  it is named after its entry, so a translated label no longer changes the
  accesses its agents hold.

#### Chat

- When a request needs a connection that is not there, Apollia says so and
  offers **Connect** it in the conversation; when it is connected but not
  given to Apollia, it offers **Allow Apollia**. Once done, the request is
  played again. Apollia looks this up through a tool of its own, on this
  computer, against its selection; nothing is added to the prompt, and an
  entry the organisation policy would refuse is never offered.
- The **@** picker offers the resources of a service that is asleep, from the
  list it last gave. Reading one starts the service. A paused service is never
  started, and its resources are not offered.

#### Microsoft 365

- **OneDrive files Apollia can read** on the card of a Microsoft account:
  browse the drive, choose folders and files, remove a choice. Agents read
  only inside what is chosen, and nothing at all until something is.
- When a work account's organisation requires an administrator to approve
  Apollia, the sign-in window shows the approval address and a request
  written for the administrator, both ready to copy, and the connection reads
  **Blocked by your organisation**. The block lifts as soon as an account
  connects.

#### API

- `GET /api/v1/connections/{id}/agents` and `POST /api/v1/connections/{id}/grants`
  read and give the agents of a connection.
- `POST /api/v1/connections/{id}/check`, `POST /api/v1/connections/{id}/pause`
  and `POST /api/v1/connections/{id}/resume`.
- `DELETE /api/v1/connections/{id}` removes a connection of any kind and
  answers what was erased; `?dry_run=1` erases nothing and answers what would
  be.
- `PUT /api/v1/connections/{id}/tools/{tool}/family` gives a tool another
  family.
- `tool_prefix` on `GET /api/v1/audit/journal` and `GET /api/v1/audit/export`,
  and `GET /api/v1/audit/activity`, which counts what each agent did with a
  connection.
- `GET /api/v1/connectors/onedrive/{account_id}/choices`, `.../browse` and
  `.../choices/{item_id}` read and change the OneDrive choices of an account.
- A `ConnectionStateChanged` event announces each state found.

### Changed

- MCP servers start at the first call of one of their tools and stop after
  `mcp.idle_stop_secs` without a call, 900 seconds by default (60 to 86400),
  to start again at the next call. Apollia's launch waits for none of them; a
  server whose tools are not known yet is started at launch in the background
  to learn them. What a server last published is kept in `mcp.db`, so its
  tools stay offered while it sleeps and survive a restart.
- `mcp.tool_loading` is read when Apollia starts and no longer applies while
  it runs: restart Apollia after changing it. `mcp.idle_stop_secs` is read at
  start too.
- An MCP tool found through `tool_search` is decided on the family its server
  declares, on every path that calls it, as a tool declared up front already
  was. A tool its server declares read-only is therefore no longer
  unclassified in deferred loading: it runs without asking, or offers
  **Always for this agent** when it reads an account.
- Microsoft accounts ask for `Files.Read`, your own files, instead of
  `Files.Read.All`, and Apollia holds the limit of the chosen OneDrive folders
  and files on the computer.
- A value typed as a header when a service is added or edited, and a value of
  a variable whose name marks it secret, goes to the secret store; the
  settings of the connection keep only a reference to it, and the raw settings
  read back through the API mask it.
- The OAuth client of an account provider, your own Google client or your own
  Microsoft registration, is entered in developer mode, in the technical
  detail of the card.
- A connection the organisation policy forbids reads **Blocked by your
  organisation**, and **See the rule** on its card opens **Permissions**, in
  place of a separate list.
- `confirm()` dialogs on the Connections page are replaced by confirmations
  in the page.

### Upgrading from 0.3.0

- **OneDrive reads nothing until you choose.** A Microsoft account already
  connected keeps working for mail and calendar, but its agents read no
  OneDrive file until folders or files are chosen on its card, under
  **OneDrive files Apollia can read**.
- **Secrets move to the secret store.** At the first launch of 0.3.1, the
  header values and secret variable values that 0.3.0 kept in the settings of
  an MCP server are moved to the secret store, and each server moved is
  logged. Nothing has to be typed again.
- **Restart after changing `mcp.tool_loading`.** A change saved while Apollia
  runs waits for the next start.
- **Google Workspace is unchanged.** It still needs your own OAuth client;
  its section now shows in developer mode.
- **No way back to 0.3.0.** 0.3.1 raises the schema of several databases under
  `~/.apollia` at its first start, and 0.3.0 refuses to open a database whose
  schema is newer than its own. Keep a copy of the folder before upgrading if
  you may go back.

## [0.3.0] - 2026-10-20

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
`-preview` suffix.

### Highlights

- **An agent reads without code.** Each agent has a sheet that states its
  mission, its model, the accesses it was granted, the sources it may consult
  and its rights. Everything on the sheet is adjusted in place and goes back to
  the defaults in one gesture.
- **Agents ship as one file.** A `.apo` package carries any combination of
  agents, workflows and Python tools, signed or not. Installing is opening the
  file: a preview shows what the package contains and who published it before
  anything is installed.
- **Run an agent on a whole folder, a workbook or a mailbox.** A batch is told
  once what to do with each item and which columns to answer in; its results
  read as a table that sorts and filters, open in a spreadsheet, end on a
  synthesis when asked, and continue in a conversation with the agent. It can
  be paused, stopped, retried, survives a restart, and runs again on another
  source with what it asked.
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
- **One developer mode, and a runtime that outlives the window.** A single
  switch, off by default, replaces the Operator and Builder profiles and
  reveals the technical surfaces: the OpenAI-compatible local server, where
  agents can be exposed as models, Apollia as an MCP server, the logs and the
  embedded documentation. The runtime installs as a service of the user
  session, and the desktop attaches to it and drives the same agents, tasks,
  approvals and models.

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
- `apollia-os package build <folder> [--sign]`, `package inspect`, `package
  preview`, `package install` and `package trust import|list|remove` build,
  read, install and trust packages from the command line.
- A Library screen lists the installed packages with their publisher, version,
  content and signature. A package is opened, updated from a file through its
  preview, or uninstalled from it, and the uninstall says what goes and what
  stays. The packages shipped inside Apollia read as one entry that cannot be
  uninstalled.
- A package installed while the runtime runs brings its Python tools and
  workflows at once, with no restart, and an uninstall removes them, with the
  triggers of its workflows.
- A package or an agent is uninstalled while keeping or erasing its memory and
  history.
- `@tool` and `@workflow` in the SDK declare the tools and workflows a package
  ships. `python -m apollia describe <folder>` writes them into the package's
  `apollia.toml`, which is read with a strict schema that names the offending
  key by its full path.
- A workflow of a package runs step by step, with nested agent and workflow
  calls stopped at three levels, and takes a date, a long text or a file among
  its inputs. `apollia-os workflow run|status|logs|cancel` drive it from the
  command line, and a new `[workflows]` section of `apollia.toml` sets the
  budget of a run started on its own.
- The Agents screen lists agents and workflows together. A workflow has a
  sheet of its own (steps, action family, connections it needs, run history),
  and "Launch" opens a form built from its declared inputs, checks every field
  before the run, shows each step live, stops it, and launches it again with
  the same inputs.
- An agent without code runs on one request, outside any conversation.
- An agent runs on every item of a source as a batch that survives a restart.
  A batch is paused, resumed, stopped and retried, is granted its accesses for
  the run, and exports its results as Excel, CSV or JSON. It is launched from
  an agent's sheet ("Process a batch"), by dropping a workbook or a CSV file in
  the chat ("Process each row"), or with `apollia-os batch run`;
  `batch status|list|accesses|pause|resume|stop|retry|export` follow it, each
  with `--json`.
- A batch reads a workbook or a CSV file from a proposed sheet and header row,
  previewed on its first twenty rows, and sends the agent only the columns
  checked. It reads a folder, one item per file with its full path and the
  text a reader extracts (PDF, Word, PowerPoint, workbooks, CSV, plain text),
  its sub-folders on request, and never writes into it. It also reads a
  mailbox, one item per message, over IMAP or on Outlook, filtered by folder,
  period and unread state, and never writes back to it. The launch dialog
  offers the connected folders and mailboxes, a folder picked on this
  computer, and a way to add a mailbox.
- A batch is told at launch what the agent does with each item, and the
  fields its answer gives, each with what it holds: the final answer of an
  item is held to those fields by a JSON schema, which a local engine turns
  into a grammar, and each field is a column of the results, of the copy of a
  workbook and of every export. On request, once the last item has run, the
  agent writes a synthesis of the table of results, kept with the batch and
  written beside a file or a folder source.
- A batch runs one item at a time by default and up to sixteen, and is
  interrupted once a set number of items have failed (ten by default, five to
  fifty, or never). An item that needs an access not granted at launch waits
  for a decision, answered once or refused from the list of waiting
  decisions, while the other items go on.
- The results of a batch go by default to a dated copy beside its file, named
  in the language of the interface (`<name> (résultats YYYY-MM-DD).xlsx` in
  French, `(results …)` in English). On request, and for `.xlsx` and
  `.xlsm` only, they go into the original after a dated backup, under a write
  grant that is journaled. A CSV file gets a copy with the new column, and a
  `.xls`, `.xlsb` or `.ods` file a new `.xlsx` copy of its values.
- Batches have their own entry in the sidebar, in the command palette and
  behind the "agents at work" pill. A batch opens on its own view: one row per
  item, live progress, the commands, and each item in full in the side panel.
  A running item says what its agent is doing (the tool it uses and on what,
  thinking or writing) and for how long. A history lists every batch, filtered
  by agent, outcome and file, and an ended batch is deleted from it or from
  its view, with its items; the tasks that ran them are kept.
- The end of a batch, and an item waiting for an access, raise a system
  notification.
- A finished batch opens on what was asked at launch (the instruction, the
  columns, the synthesis) and on the files it wrote, each opened in its
  application or shown in its folder. Its results read as a table, one row per
  item and one column per field: a header sorts (amounts read as numbers), the
  fields of few values are filters with their counts, and a cell an answer
  left empty is marked to be read again. "Continue in the Chat" opens a
  conversation with its agent, the table joined, and "Run this batch again"
  opens the launch dialog on what it asked.
- The creation of an agent is a draft that outlives its screen and the
  application: the form, the workshop that prepares the sheet, which goes on
  while the screen is away, and the sheet under review with its edits. The
  list of agents offers to resume or discard it.
- A file added as a source of an agent offers to let it open the file, by the
  reading of the connected folder that holds it, or by connecting that folder
  on the spot; the sources of a sheet connect a folder of this computer
  directly.
- A trigger starts an agent or a workflow, set from the agent sheet.
- An agent carries its own autonomy tier, declared by its package and
  overridable by the user; `apollia-os run --autonomy` overrides it for one
  run.
- An agent has a planning mode, never, auto (the default) or always, declared
  under `[agents.defaults] planning` and overridable. When it plans, every step
  of the plan uses the agent's model and only the tools it was granted.
- `apollia-os agent set`, `agent reset` and `agent list --definitions` change
  an agent, put one field or the whole agent back to its defaults, and list the
  agents as resolved; `agent show` prints the resolved definition.
- An agent is always available: loaded on demand and unloaded when idle,
  after `[runtime] agent_idle_unload_secs` (600 by default). It reads as
  available, degraded or unavailable.
- A tool output too long for the context is kept whole on disk, and the model
  receives its beginning and the path instead of a cut output.
- An agent reaches another agent or a workflow only through an `agent:<id>` or
  `workflow:<package>/<name>` access.
- Agents without code and installed workflows are listed on the A2A card,
  each with its kind and input schema, and invoked through
  `POST /api/v1/a2a/invoke`. A client names an invocation with `invocation_id`
  and cancels it with `DELETE /api/v1/tasks/{invocation_id}`.
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
  web watch, mail triage, document analysis and request qualification. The
  request qualification example comes with a demonstration workbook and three
  reference documents, which "Try with the example" copies to a folder and
  opens as a source and as a batch.

#### Inference and models

- An OpenAI-compatible server is declared by its address, tested, and lists
  its models with the context window each one really serves.
- The macOS arm64 bundle ships an MLX engine, vllm-mlx 0.5.0, run on the
  embedded interpreter. The format of a model picks the local engine: GGUF on
  llama.cpp, MLX on vllm-mlx. System > Engines picks the engine of each format
  (`[engines] gguf`, `[engines] mlx`), shows the channel, version and update of
  each, keeps them updated (`[engines] auto_update`, on by default) and purges
  old builds on request.
- Inference engines get an update channel of their own. Engines are downloaded
  on demand, or imported from a disk, against a signed manifest; the
  accelerated NVIDIA build is fetched when a card is detected instead of
  coming in a separate installer.
- Installed models come from an index that covers GGUF and MLX. The model
  registry holds about two hundred cards across the main open model families
  (Qwen, Gemma, Llama, Mistral AI, Microsoft, DeepSeek, IBM, NVIDIA, OpenAI,
  Cohere and others), embeddings models (BGE-M3 recommended when it fits) and
  a distilled French Whisper dictation model. The cards ship inside the
  application and read offline; a signed online index, when reachable, takes
  precedence. `[models] registry_url` and `registry_enabled` change or turn off
  that index.
- The discovery window groups models by maker, searches every repository and
  format, and gives each model a card with its download options, a verdict for
  this computer, one recommended variant, the author's README and "Use in a new
  conversation".
- A model shows what it can do and a short list of its variants. A variant
  downloads whole, and its vision projector reaches the engine.
- A model imported from a disk is checked, and an unverified origin is
  marked; `models.import.require_verified` refuses it.
- Embeddings and dictation models join the registry, sized for the machine and
  verified on download.
- Model presets, the model of each agent and the default model are stored. An
  agent's request settings (temperature, top-p, top-k, min-p, penalties, stop
  words, token limit) resolve over the model, its package and its preset, and
  reach any engine as well as `ctx.llm.complete`.
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
  fits this computer before it is loaded. A model opens a panel with its
  information and, in developer mode, its loading and inference settings,
  saved as presets of the model.
- Settings > Model defaults sets the default context window, the load
  guardrails, model unloading and a maximum image size (`[llm.defaults]`,
  `[llm.images] max_side_px`); a larger image is scaled down before it is
  sent. An option unloads the loaded models before another one loads
  (`[llm.residency] unload_before_load`).
- System > Hardware lists each GPU, each of which can be held back
  (`llm.devices.disabled`), and monitors memory, video memory and processor,
  also served by `GET /api/v1/llm/hardware/usage`. `GET /api/v1/llm/hardware`
  reports what a model can occupy on the machine.
- A conversation shows the models that are loading.
- A model can be loaded with a draft model of its vocabulary.
- The AI screen offers the accelerated engine when an NVIDIA card is detected.

#### Connections and tools

- Every connection of the machine reads as one list, and the built-in tools
  join it as the Apollia source.
- A connection from the curated selection is added in two clicks, with a
  single warning. The public catalogue is queried only on "Explore all", with
  a search and an accurate "official" badge.
- A mailbox is added with its address and an application password, over IMAP,
  read-only, from the window or with `apollia-os connector imap test|add|list`
  (the password is read from standard input or `APOLLIA_IMAP_PASSWORD`).
- A folder of the machine is added as a connection, and the addition offers,
  checked by default, to let Apollia read it.
- Each connected account publishes its own connector tools, named after it
  (`conn:<provider>:<account>/<operation>`), so two accounts of one provider
  publish two sets of tools.
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
  CSV file is read as cells of text. `file_read` reads these formats too,
  whole or one page, slide or sheet.
- A chat message carries up to four PDF, Word, PowerPoint or Excel files of
  20 MiB each, and their text reaches the model.
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
- Every decision waiting for the user is listed in one place, To review, and
  answered on the same card as in the chat, with a short delay to cancel.
- An Authorizations screen shows, agent by agent, the families granted, the
  standing consents and the fine rules, each withdrawn in one click with an
  undo, or all at once. Writing a fine rule takes the developer mode.
- A signed organisation policy is read once at boot. It governs connections,
  inference servers, model imports, the model registry, the packages that may
  install and the settings it fixes. A setting the policy imposes reads as
  locked. The organisation key, the policy file and its signature are created
  and checked from the command line, and a screen shows what is allowed and
  why.
- The journal records consents, refusals, rules, batches, policy, every
  change of `apollia.toml` (the keys changed, their origin and the digest of
  the file before and after, never the values) and every call to the
  compatible server (never its content). Its entries read as summaries with
  their detail on demand, filtered by agent, family, nature, result and
  period, in the window and with `apollia-os audit journal`.
- The organisation policy can make the journal required, keep its entries for
  a set number of days, force or forbid the detail in its exports, and write a
  daily summary file. It can only close the compatible server, never open it.

#### Developer mode and local server

- A compatible server listens on its own port, off by default, switched on and
  off while the runtime runs. It serves the loaded local models in the OpenAI
  and Anthropic formats, loads models on demand, answers CORS preflights when
  asked to, keeps and streams its recent logs, and can be opened to the local
  network.
- The compatible server gets named tokens, shown once and revocable.
- An agent or a workflow marked as exposed is served by the compatible server
  as the model `apollia/<id>`, on chat completions, responses and messages,
  with its steps in `reasoning_content`. An access nobody granted answers 409
  `approval_pending`.
- A request to the compatible server may bring its own MCP server over HTTP
  when `[server] allow_request_mcp` is on (off by default); it runs governed,
  and the organisation policy can refuse it.
- `server.separate_reasoning_content` (on by default) keeps the reasoning out
  of `content` in the answers of the compatible server.
- Apollia acts as an MCP server, and an MCP client reaches only the agents and
  workflows the user exposes.
- The documentation site is embedded in the desktop and readable offline.
- The desktop keeps a log file, and the engine log level is set from the
  settings.
- The System page shows the engines and the hardware.
- The configuration file can be edited in the settings.
- A Developer screen gathers the server, its tokens, call examples to copy,
  the logs, the HTTP reference of the native API and the documentation.
- A switch in Settings > Developer keeps Apollia running without the window:
  it installs the session service and relaunches the desktop attached to it.
  The organisation policy can forbid it.
- The native API covers what only the desktop did before, among them:
  connecting a Google or Microsoft account for a hosted caller with its own
  redirect address, browsing the MCP registry, setting an MCP server secret
  (never answered back), streaming the event bus (`GET /api/v1/events`),
  pausing a chat turn, the profile, the pending questions, governance, memory
  export and import, and new families for loaded models, engines, inference
  servers, presets, agent definitions and drafts, batches, workflows and
  their runs, packages, connections, embeddings and dictation. The OpenAPI
  document and the TypeScript and Python clients carry them.
- The LLM chat and complete routes take `schema`, `temperature` and `seed`,
  and return the validated answer under `value`. `GET /api/v1/llm/status` says
  whether each backend carries a seed.

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
- A top bar icon with a counter opens the downloads panel, and an option
  opens it when a download starts.
- The chat shows up to four conversations side by side.
- Conversations are filed in folders, dragged in and out of them, and found by
  what their messages say. `apollia-os chat list` shows the folder of each.
- A deleted conversation goes to a trash and can be restored, unless a setting
  makes deletion final. `apollia-os chat delete` stays final.
- The composer shows what the model can do (reasoning, vision) and a compact
  context gauge.
- The Integrations tab of a conversation lists what its agent reaches; an
  access is switched off or proposed for that conversation only, and "Apply to
  the agent" copies the conversation's model, preset and instructions to the
  agent.
- The chat settings add the sort order of the list, the send key, automatic
  naming (`[desktop] chat_auto_naming`), one empty conversation at a time, the
  token counts and a shortcut to regenerate the last answer. The appearance
  settings add the side of the navigation, Markdown or raw answers, generation
  figures, scrolling, and bubbles or blocks.
- `apollia-os profile list|show|create|rename|set|set-default|delete` manage
  the profiles, each with `--json`.
- Settings > Integrations sets what Apollia grants when nobody chose: the
  accesses of a new agent (`[tools.new_agent]`) and the profile switch of a new
  conversation (`chat.share_profile_default`).
- The notification channels (desktop or web call) are set in the settings,
  each with a test.
- A conversation carries its own settings and notes, shows its prompt
  template, and can keep the reasoning of its earlier answers.
- An answer is continued, branched off, rewritten or removed from the row
  under it.
- The first launch can be replayed from the danger zone of the settings.
- The first launch is a sequence of steps: the language, the AI (the one step
  that cannot be skipped, where a local model is picked or a server declared,
  its download going on through the next steps), the search and dictation
  models, a test question answered by the chosen model, the profile, a first
  agent (from a file, created or taken from an example) and the connections
  that agent still needs.

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
- The first launch no longer asks for a usage profile and leads to a working
  agent (see Added).
- The Inbox becomes To review, the list of what waits; the history of the
  decisions already taken is no longer kept there.
- The Tools and Permissions sections leave the settings: the tools live under
  Connections and the rules on the Authorizations screen. Notifications move
  from the sidebar into the settings, and the Model Hub page becomes the
  "Discover models" window of the AI screen.
- Most screens are redone on one surface, one row per item: the connections,
  the agent sheet in tabs (what it is, what it did, what it keeps), the chat in
  one column, the models as cards.
- Tool steps and reasoning are shown plainly in the chat, each tool card is
  redone, and the code card shows the whole command.
- The interface texts follow one vocabulary, held free of banned technical
  terms.
- Cancelling a task stops its execution instead of relabelling it.
- `GET /api/v1/health` answers without a token on the TCP listener and reveals
  the product marker and the version, nothing else.
- `apollia-os audit export` exports the hash-chained journal, with the filters
  of `audit journal`, as CSV or JSON; `--detail` adds the content, with a
  warning.
- On the LLM chat routes, a schema the engine cannot express, a backend with
  no structured mode, or an answer the schema refuses gets a 422 naming the
  offending path, instead of a 503.
- A reload of the configuration keeps `[llm.routing]` and reaches the agents
  already running.
- The embedded llama.cpp engine moves to build b11541.
- A file access an agent is refused says how to grant it: a folder already
  connected is granted on the agent's sheet, a new one is added in
  Connections first.
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
- `apollia-os run --alternatives` (nothing offered the plans to choose from),
  and `--allowed-tools` and `--disallowed-tools`, replaced by the access list
  of the agent.
- The Start, Stop, Enable and Disable actions of agents in the desktop, and
  the stopped and stopping states; the command line keeps `agent enable` and
  `agent disable`.
- `apollia-os tools approvals resolved`, the route of the resolved approvals,
  and the resolved listing of `task approvals`, which lists what waits.
- The session authorizations, chat rules and audit sections of the
  Permissions settings.

### Fixed

- A batch on a folder failed on every item: the item named its file by its
  path in the folder, which no file tool resolves, and the replans that
  followed were built without the tools of the agent, so they invented tools
  it did not hold and ended on `MAX_REPLAN_EXCEEDED`. A replan is now held to
  the tools the agent is granted, like the first plan.
- On a model served by MLX, a conversation was named by the reasoning of the
  model ("Here's a thinking process"); its title is now asked with reasoning
  off, and a line that opens as reasoning is refused.
- Leaving the creation of an agent lost the form, the answers given to the
  workshop and the sheet under review, and stopped the workshop.
- The export of a batch wrote the whole answer of an item, as JSON, in one
  column, and the full text of every file of a folder.

- A tool call whose argument a local model wrote as text, a list, an object, a
  boolean or a number, was refused ("invalid type: string, expected a
  sequence", "expected a boolean"). The value is read as what the schema of the
  tool asks for.
- On a machine whose data folder does not exist yet, a TLS certificate left
  without its key in `apollia.toml` was not reported.
- In plan mode, a local model could repeat itself inside a text field of the
  plan it was proposing and never finish: the conversation showed it thinking
  for as long as it was left. The fields of a plan step are described and
  bounded, and a short length bound of a tool schema now reaches the grammar of
  the local engine, which ends the string.
- A turn that ends by proposing a plan no longer reads as an answer that could
  not be produced.
- When a step of a plan failed, the request for a new plan carried a system
  message alone, which the chat template of the Qwen models refuses: the
  engine answered 500 on every attempt and the task ended on `REPLAN_FAILED`.
  It now carries a user turn.
- A plan answer had no token limit, and a reasoning that looped ran until the
  engine gave up at 300 seconds. Plans are capped, so a loop ends in about a
  minute and the planner tries again.
- A server error from an engine was retried for fifteen minutes before it was
  reported. It is retried for one minute at most.
- The cause of a failed plan step, and an MLX engine line that reports a
  failure, were written nowhere. Both reach the log.
- An engine that closes an answer without generating anything is reported as
  a failure instead of an empty answer said to have finished normally.
- A structured answer from a reasoning model behind a server with no
  template-aware parser failed with "the answer is not JSON": the reasoning
  block is now skipped before the answer is read.
- A `schema` sent to the LLM chat routes of the HTTP API was ignored, and a
  `seed` asked of the local engine or of an OpenAI-compatible server was
  dropped without a word.
- The free chat ignored the backend it stored as preferred.
- The model list missed models stored in a sub-folder, showed each part of a
  split GGUF and the vision projector as models of their own, and filed a
  dictation model saved as `.gguf` as a language model. A model downloaded
  into its own folder was not found at first launch, and wiring it copied the
  weights.
- Deleting a model a configuration still names says why it is refused, and
  deleting a model removes its card, projector, other parts and empty folder.
- An unreachable server is reported with its address, not as "HTTP error 0".
- Transcribing a file worked only after something had been dictated.
- "Install and restart" on an update did not restart the application.
- MCP OAuth on a server that publishes client metadata documents could fail in
  the browser with "Unknown OAuth client": dynamic client registration is tried
  first.
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
  The Apollia agent of the chat alone receives the profile of its
  conversation, under the "Use my preferences" switch of that conversation
  (on by default).
- The compatible server listens on the loopback interface unless the user
  opens it to the local network; agents and workflows called from the network
  require a token. The native API keeps refusing a non-loopback bind without a
  token.
- The organisation policy, not the user profile, forbids cloud connections.
- Images attached to a message never reach a trace or a log.
- On Windows, an organisation policy is trusted only when its folder and files
  belong to Administrators or LocalSystem and nobody else can write to them,
  and its signature date is a signed field, not the date of a file.
- A model file stored outside LFS on Hugging Face is checked against its
  published git blob identifier, and a variant file with neither digest is
  refused. An imported model is checked before it enters the models folder,
  and dictation models downloaded from the command line are checked against a
  digest.
- A `.apo` archive with an absolute path, a `..`, a symbolic link, a duplicate
  name, or entries that would inflate past their bounds is refused.
- The bundled `pypdf` moves to 6.20.1, which clears eight advisories, and the
  interface's `dompurify` to 3.4.17, which clears two.

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
  which answers only when the agent's profile source is checked: declare
  `sources = ["profile"]` under `[agents.defaults]` of the package, or let
  the user check the source on the agent sheet; `ctx.profile` is `None`
  otherwise.
- `@tool` no longer takes `dangerous`, and `action` is required: one of
  `read`, `read_personal`, `write_or_delete`, `send_external` or
  `execute_code`. Every family except `read` is sensitive, so a former
  `dangerous=True` becomes the family that says what the tool does, for
  example `action="write_or_delete"`.
- `ctx.secrets`, `ctx.datasources` and `ctx.templates` read nothing: the
  `secrets`, `datasources` and `templates` of the former `@agent` decorator
  have no counterpart in `apollia.toml`. `get` returns `None` on
  `ctx.secrets` and raises `FileNotFoundError` on the other two. Pass the
  value as an argument of the tool or the workflow instead.
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

#### Command line and API

- `apollia-os run` no longer takes `--alternatives`, `--allowed-tools` or
  `--disallowed-tools`: remove them, and grant tools on the agent's sheet.
- `apollia-os audit export` requires `--format` and `--output`, and exports the
  journal instead of the trail of tool invocations.
- `apollia-os tools approvals resolved` and the route of resolved approvals are
  removed; `task approvals` lists what waits.
- An agent no longer reaches the A2A skills of every loaded agent: grant
  `agent:<id>` or `workflow:<package>/<name>` on its sheet or in its package.
- An access list that names a bare connector operation stops granting it once
  the provider has two accounts connected: name the tool
  `conn:<provider>:<account>/<operation>`.

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

### Compatibility

- Published builds: the desktop for macOS arm64 (`.dmg`), Linux x86_64
  (`.deb`, `.AppImage`) and Windows x86_64 (`.msi`, `.exe`), and the
  `apollia-os` command line for the same systems, with Vulkan and Linux arm64
  variants built when their runners allow.
- The desktop runs on macOS 13 or later; the MLX engine needs macOS 14 or
  later on Apple Silicon, and GGUF models run on llama.cpp everywhere.
- The installers and the installed application keep the name Apollia OS, so an
  installation of 0.2.0 is updated in place rather than installed beside; the
  window and the interface say Apollia Desktop.
- The runtime embeds CPython 3.13.13. The SDK supports Python 3.10 and later
  for authoring and tests; building from source takes Rust 1.89 or later.
- The databases under `~/.apollia` are migrated at the first start of 0.3.0,
  and 0.2.0 refuses to open a database whose schema is newer than its own.
  Keep a copy of the folder before upgrading if you may go back to 0.2.0.
- New third-party components users get: `async-imap`, `mail-parser` and
  `rustls-native-certs` (mailboxes), `rust_xlsxwriter` (Excel export), `zip`
  (packages), `pdf_oxide` and `office_oxide` (documents) and vllm-mlx with MLX
  (the macOS arm64 engine). Each is stated in the decisions chapter of the
  documentation. `hf-hub` is no longer used.

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
