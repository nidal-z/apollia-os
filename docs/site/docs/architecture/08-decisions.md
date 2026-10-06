---
sidebar_position: 8
title: 8. Design decisions in force
---

# 8. Design decisions in force

This page states the structural decisions that hold today, and why. It is
written in the present tense: it describes the system as it is built, not the
route that led there. Where a decision has a cost, the cost is named.

## Stack and runtime {#stack-and-runtime}

The runtime is Rust on Tokio. Agents are Python, loaded in-process through a
PyO3 bridge rather than spawned as subprocesses: the bridge translates Rust
futures to Python coroutines directly, so an agent call costs no process
boundary and no serialization round-trip.

Every subsystem that owns mutable state is a Tokio actor with a bounded channel
and a clonable handle. No state is shared between actors behind a lock. The cost
is indirection, and it buys the property that matters at this size: a deadlock
cannot form between two subsystems that only exchange messages.

Persistence is SQLite in WAL mode, vendored into the binary. Nothing on the
default path requires a server, a daemon, or a network.

## The agent contract {#agent-contract}

An agent is a Python class carrying `@agent`, with at least one `@skill` or one
`@on_message` async method. That is the whole contract. The bridge refuses an
object that does not expose the dispatch entry point the decorators install;
there is no escape hatch that accepts an arbitrary callable.

The runtime hands each call a `ctx` object exposing the services an agent may
use: the LLM router, memory, tools, agent-to-agent messaging, notifications, and
a logger. `ctx` is the whole runtime surface. An agent reaching around it is
using something the runtime does not guarantee.

Agent payload schemas are `TypedDict`, read at registration time to build skill
schemas. This is why deferred annotation evaluation is refused in those modules:
it turns annotations into strings, and the schema comes out empty with no error.

The Python SDK has no third-party dependencies. Every one would become a
dependency of every agent, on every machine, forever.

## Execution model {#execution-model}

The autonomous engine observes, reasons, and acts in a loop. It runs in two
modes: direct, where a skill answers, and orchestrated, where a plan is built
and its steps executed.

On the orchestrated path, step arguments are filled at plan time under a
grammar, with just-in-time extraction as a fallback at execution. Filling them
at plan time is what lets a plan drive real tools with structured arguments
instead of re-parsing prose at each step.

A completed orchestrated run is verified by a critic from the `supervised` tier
upward; the `assisted` tier runs no verification pass at all. The verdict is
recorded as a runtime event, and a failing verdict triggers bounded re-planning
under the same budget that bounded the original run.

## Budget and safeguards {#budget-and-safeguards}

Every run carries a step budget with three dimensions: a maximum number of
steps, a maximum number of tool calls, and a wall-clock ceiling. The runtime
enforces it. Agent code cannot raise it, extend it, or opt out of it, because
the counter lives on the runtime side of the bridge and the agent never holds a
reference to it.

This is the safeguard the design rests on: an autonomous loop that can be
stopped is a product, and one that cannot is a liability.

## Supervision {#supervision}

<!-- claim:supervisor-has-no-restart-machinery -->
<!-- claim:supervisor-has-no-child-spec -->
<!-- claim:supervisor-has-no-restart-tracker -->
The supervisor is fail-fast then degrade. An actor that dies is not restarted.
The runtime reports the loss, degrades the capability that actor served, and
keeps serving everything else.

Restart-on-crash was considered and rejected: an actor that crashes has already
lost the state it owned, and restarting it produces a subsystem that answers
requests with a plausible but empty view. A capability that is honestly absent
is easier to operate than one that quietly lies.

## Tools and confinement {#tools-and-sandbox}

Tools are resolved at startup, not at first call, so a missing or misconfigured
tool is a startup error rather than a failure in the middle of a run.

The native tool set covers shell execution, path-confined file reads and writes,
Python execution in a per-agent virtual environment, HTTP fetch, web search, and
memory search. File tools are confined to a resolved root: the path is
canonicalized and re-checked against the root after resolution, so a symlink
cannot walk out of it.

The root of an agent's file tools is the union of the folders granted to that
agent, the built-in chat agent included, on the chat path and on the task path
alike. A folder of the machine is a connection, chosen with the system picker,
and its tools are granted to an agent one by one: the reading tools reach every
folder granted to it, the writing tools only those granted for writing. The
first folder granted, by the date it was added, anchors a relative path. An
agent with no folder granted has file tools that refuse with a message saying
how to grant one. Nothing else is a root: not the home folder, not the data
directory, not the volume, not the folder of a project. The former
`[filesystem] trusted_paths` and `[chat] default_workspace` keys are withdrawn;
the folders they named, the home folder excepted, were added once as
connections, granted to no agent.

Confinement is not uniform across platforms, and the difference is not
cosmetic. On Linux, shell execution runs under PID and mount namespaces. On
macOS the confinement is weaker. On Windows there is none. This is stated
plainly rather than smoothed over, because an operator choosing a platform is
choosing a threat model. See [the agent trust
model](/explanation/agent-trust-model) for what that means in practice.

## Outbound HTTP {#outbound-http}

Every outbound HTTP call goes through one module, `apollia_core::net`, and
`scripts/check_http_clients.py` refuses a client built anywhere else or a
response body consumed by any other means.

The module carries two rules. A destination must resolve outside the loopback,
private, link-local, multicast and unique-local ranges, and outside the internal
domain suffixes, checked on the first URL and again on every redirect hop, so a
public URL cannot redirect into the local network. A response body is read
through a cap that abandons the stream once the budget is crossed, rather than
buffering the whole answer and measuring it afterwards, by which point the
memory is already spent.

One exception is named rather than left implicit. A destination the operator
configured may legitimately be internal: a local MCP server, the runner, a
self-hosted model endpoint. Those clients are built through a separate
constructor that keeps the body cap and drops the address check.

Holding both rules in `apollia-core` is what puts `reqwest` and `url` in the
crate every other crate depends on. They sit behind an optional `net` feature.
Nine of the crates that depend on `apollia-core` turn it on; the others keep the
dependency closure they had, and neither crate is new to the tree: both were
already workspace dependencies, and the lockfile gained no package. The cost is
that the foundation crate is no longer dependency-free in every configuration.
That is worth paying, because the address policy previously existed in two
copies and the capped read in six, and each rule was simply absent wherever its
copy had not been made.

## Permission model {#permission-model}

<!-- claim:prefix-rules-evaluated-per-invocation -->
On the chat path a tool call is decided in a fixed order. The operator's
persisted rules are evaluated first, per invocation, against this call's own
first argument: project rules, then rules scoped to the chat agent, then global
rules, longest argument prefix first. A matching deny refuses the call outright,
even one the turn had already authorized by name. Otherwise the call runs when
the tool's name is in the turn's authorization set or a rule allows it, and a
rule-based allow is not remembered, so the next call is evaluated again with its
own argument. Anything left raises an approval request and waits for a person; a
refusal, or five minutes without an answer, runs nothing.

A persisted rule carries one of three scopes: project, agent, or global. The
session scope is refused at write time and lives in memory only, and the chat
path passes no in-memory session rule.

Code executors are never blanket-authorized. A rule that would grant every tool
does not grant shell or Python execution; those require their own explicit
grant. A blanket grant is usually a convenience decision about reading files,
and it must not silently become permission to run arbitrary code.

<!-- claim:executor-guard-blocks-command-chaining -->
A rule only lets a shell command skip approval when that command is a single
simple command, with no chaining, pipe, redirection, substitution, or
backgrounding. That guard decides whether a call may skip approval; it is not a
filter placed in front of execution, and a code executor with no matching rule
asks a person on every invocation.

<!-- claim:permission-decision-is-not-recorded -->
The permission decision itself is not written anywhere. Nothing in a shipped
binary writes to the `permission_audit` table; it is read by
`apollia-os permissions audit` and by the desktop audit view, and by nothing else.
What is recorded is the invocation: `tool_invocations` holds what ran, not who
allowed it.

## Human in the loop {#human-in-the-loop}

Any tool can require human approval before it executes. When one does, the
runtime suspends the run, emits an event carrying what is being asked, and
resumes on the answer. Suspension is a first-class state, not a blocking wait:
the process is free, and a run can stay parked across a restart.

## Memory {#memory-model}

Memory has three layers: episodic events with an importance score, semantic
facts with a confidence score, and procedures with triggers. Each agent has its
own namespaced store, searchable full-text through SQLite FTS5 with BM25
ranking.

Memory is read at the agent's initiative. The runtime never injects memory
content into an agent's prompt. An agent that wants context asks for it, which
keeps the prompt something the agent author controls and can reason about.

Three exceptions exist, and none of them is reachable from an agent execution
path. Two are inside the built-in conversational assistant: a brief of the
profile of the conversation, and past session summaries on the first message of
a free chat. The third is outside the assistant, in the desktop prompt-rewrite
command, which carries the role, the function and the tools of that profile into
its own one-shot prompt and returns text to the composer rather than to a run.

The profile brief is appended on every turn of the assistant, at every autonomy
tier, while the profile switch of the conversation is on. The switch is on by
default, stored with the conversation, and the conversation gauge reports
whether the prompt carries the profile. The brief was previously gated by the
longest autonomy tier, which free chat never runs at, so a profile the user
filled in never reached the assistant. A setting with no effect is worse than an
injection the user sees and can turn off, so the gate moved from the tier to the
conversation. The rule for agents is unchanged: an installed agent reads the
profile only when it asks for it.

## Local inference {#local-inference}

Local models run through an embedded `llama-server`, the upstream llama.cpp
server, which the daemon spawns and supervises over its OpenAI-compatible HTTP
API, with native tool calling and continuous batching.

An in-tree binding was maintained previously and has been dropped. Keeping one
meant tracking a fast-moving upstream through a foreign-function layer, and
every capability that landed upstream landed here late, or not at all. Speaking
the upstream HTTP API instead costs one local process and buys the upstream's
release cadence.

A packaged build stages the engine binary. A source build expects
`llama-server` on the `PATH`. When a local backend is configured and no engine
is reachable, calls fail with an explicit unavailable reason rather than
silently falling back to a cloud provider.

On macOS Apple Silicon, models in the MLX format run through a second embedded
engine, vllm-mlx (Apache 2.0), an OpenAI-compatible server built on MLX with
continuous batching, prefix caching and native structured output. One version
is pinned, and the engine is installed by hash into an environment of its own
inside the bundled Python tree, apart from the packages agents import: the
engine starts with the bundled interpreter in a mode that keeps the agents'
packages off its path. Only the dependencies that serving a model imports are
installed, without resolving the rest of the closure the engine declares.
Two exclusions follow from that. mlx-embeddings is GPL-3.0 and only backs the
engine's embeddings endpoint, so it never ships and that endpoint is not
served; the build and the release check both refuse a bundle that carries it.
PyTorch, OpenCV, Gradio, SciPy and the audio stack are not imported to serve
a model, so they are left out. The daemon supervises the engine the way it
supervises `llama-server`, behind an engine interface that keeps the engine
replaceable, and it listens on the loopback interface only. The cost is
weight: the engine and its dependencies add about 150 MB to the macOS disk
image, and they need macOS 14 or later, the floor of the MLX wheels at the
pinned version. The Windows and Linux bundles do not carry it.

Engines have an update channel of their own and do not follow the version of
Apollia. The embedded engine serves every machine from the first launch; an
accelerated build, CUDA on a machine with an NVIDIA card, is downloaded on
demand, or imported from a disk without network, and the launcher then takes the
newest installed build of that variant over the embedded one. Which engine
serves each model format is a setting, automatic by default; a chosen variant
that is no longer installed falls back to the automatic choice. The followed
channel is checked when the runtime starts and when the channel changes, and an
update it announces for a chosen engine is installed unasked, unless the setting
that governs it is off, in which case the update is only announced. Every
download or import of an engine is preceded by the verification of a digest manifest
signed in the minisign format with a dedicated distribution key. Each channel,
stable or preview, publishes one such manifest, listing every engine archive
with its SHA-256. The public half of that key is compiled into the binary and
is the only key accepted. No archive is requested before the signature holds,
an archive whose digest differs from the manifest is deleted, and a manifest
older than the last one accepted on its channel is refused. A mirror can replace
the address manifests and archives are fetched from; it cannot replace the key.
Engine archives are read as zip only, the format already read for agent
packages, and no other archive format is parsed. Upstream builds published as
`.tar.gz`, those for macOS and Linux among them, are repackaged as zip by the
release pipeline that publishes the manifest, and the manifest carries the
digest of that zip. The MLX engine environment is listed in the same manifest
and installed through the same verification.

There is no dedicated cloud provider. A remote model is reached through one
generic connection to an OpenAI-compatible server, at the address the
organization designates: a self-hosted vLLM or LM Studio, or a gateway. Any
OpenAI-compatible address is accepted, but a cloud API is neither tested nor
promoted. Where a cloud model is needed, LiteLLM is the recommended gateway: it
runs on infrastructure the organization controls and speaks the OpenAI protocol
to Apollia. When `system.db` is migrated, backends that pointed at the OpenAI,
Anthropic or Mistral APIs are deleted, and backends that point at a self-hosted
server are kept unchanged.

### Checking a downloaded model {#model-download-integrity}

Every file of a model downloaded from HuggingFace is checked against the
identifier the repository publishes for it before the model is recorded. A
file stored through LFS (weights, projector, large tokenizer) is checked
against the SHA-256 of its content. A small file committed outside LFS
(`config.json`, `tokenizer_config.json` and the like in an MLX repository) has
no published SHA-256: HuggingFace publishes only its git blob identifier, the
SHA-1 of `blob <size>\0` followed by the content. Such a file is checked
against that identifier, which is why `sha1` is a direct dependency of
`apollia-llm`. It comes from the same RustCrypto family as `sha2`, adds no
transitive crate, and serves to compare a file with what its repository
declares, not to resist a forger: the TLS connection to the repository carries
that guarantee, and a file that matches neither identifier is deleted.

### The model registry {#model-registry}

The catalogue of models reads one online index of model cards. A card is a
short text written for this project, in French and in English: what the model
is good at, the capabilities its chat template declares, the memory it asks
for, the sampling values its publisher recommends, the switches its template
exposes, its licence and the page of its original repository. Every value a
card repeats from a model repository cites the address it was read from; no
README text is copied into a card, and a README is read from the repository
only when the user asks for it, as text, never written to disk. The cards are
written in this repository, so that a card is added or corrected without a new
version of the application.

The registry is read under the local-first principle. The index is a static
file, requested when the catalogue opens and at no other time, by a request
that carries no body, no query, no cookie and no identifier: the registry
learns nothing about who reads it, and no user data is sent. It is signed in
the minisign format by the dedicated distribution key described under
[Signatures](#signing-schemes), whose public half is compiled into the binary
and is the only key accepted; an index whose signature does not hold is
refused, never used. The index is served from object storage operated in
France, behind a domain of the project, with access logs kept for a short
period only. When that address does not answer, the same signed index is read
from the release mirror on GitHub, under the same verification. The last index
accepted is kept on disk, and the card of a model is copied next to it when it
is downloaded, so an installed model reads its card offline.

An organisation replaces the address by an internal mirror, or turns the
registry off, through the `[models]` section of `apollia.toml`.
A mirror changes where the index comes from, never who may sign it. Without
network and without a kept copy, the catalogue is empty and says so, nothing is
recommended, and a model can still be imported from a disk.

The cost is a network source the catalogue depends on: no index ships with the
application, so a first launch without network has no recommendation.

## Speech to text {#speech-to-text}

Transcription runs out of process, in a sidecar built on whisper, so a model
crash cannot take the daemon with it. It is optional: a build without it loses
dictation and nothing else.

## Model Context Protocol {#mcp}

Apollia is an MCP client over stdio and HTTP transports. It is also an MCP
server, for the agents and workflows the user chooses and nothing else:
`apollia-os mcp server` is a stdio bridge to the running daemon, which lists
the agents and workflows marked exposed and runs one to its end when a client
calls it. An agent takes one message, under the tool name `agent__<id>`; a
workflow takes the inputs its schema declares, under
`workflow__<package>_<workflow>`. A name keeps to `[A-Za-z0-9_-]` and 64
characters, as the model APIs require, and two items whose names would
collide are both left out. The bridge is off by default: the daemon refuses
it until `[mcp] server_enabled` is turned on, a setting it reads again each
time a bridge opens. The native tools (shell, files, agent installation) are
no longer served to an MCP client.

Every MCP response is treated as untrusted input. Responses are capped per
server, tool names and descriptions are validated at ingestion against a
character set and a length limit, and the number of tools a single server may
contribute is bounded. A server that misbehaves degrades itself, not the
runtime.

Servers requiring OAuth go through a dedicated flow with its own, smaller
response cap.

Node.js is not shipped with Apollia, which keeps principle 2 (zero external
dependency): nothing has to be installed on the machine beforehand. A pinned
LTS release of Node.js is published in the signed digest manifest of the engine
channel, the manifest signed in the minisign format with Apollia's dedicated
distribution key (see [Local inference](#local-inference)), and it is
downloaded the first time a server launched by `node` or `npx` needs it: the
manifest signature is verified before any archive is requested, then the
archive digest, then the archive is extracted into the engine store. Such a
server then runs on that Node.js by its absolute path, never through the
`PATH`; while none is installed, its launch fails with a typed "Node.js
required" error. A machine without network imports Node.js from a disk,
through the same verification. The npm packages of the servers install from a
lockfile carrying an integrity digest for each package.

## Connectors {#connectors}

Google and Microsoft connectors authenticate through OAuth2 with PKCE. No
aggregator sits in the path: a paid third-party relay between an operator and
their own mailbox contradicts the point of the product.

The binary embeds a Microsoft OAuth client and no Google one. Google's
restricted scopes require a verification process the project has not completed,
so a Google connection asks the operator to supply their own client credentials.
The asymmetry is deliberate, and it is stated where an operator meets it.

## Documents and sources {#documents-and-sources}

Workbooks and CSV files are read in Rust, without the embedded Python
interpreter. `calamine` (MIT) reads `.xlsx`, `.xlsm`, `.xlsb`, `.xls` and
`.ods`; `csv` (MIT or Unlicense) reads CSV files, with the separator detected
among the comma, the semicolon and the tabulation. Both are pure Rust, read
only, link no system library and open no connection. Each cell is read as text
with its sheet, row and column: a number without a zero fraction, a date in
ISO 8601, a formula by the value the file last computed for it (no formula is
evaluated), an error by its code. A workbook protected by a password, or
damaged, is a typed error, never a crash. A CSV file that is not UTF-8 is read
with its invalid bytes replaced, and the result says so: no encoding is
guessed.

The cost is named: `calamine` depends on `zip` 8 while the workspace carries
`zip` 4 for the package archive and the batch export, so two major versions of
`zip` enter every build, with five small crates under `calamine` (`atoi_simd`,
`codepage`, `debug_unsafe`, `fast-float2`, `typed-path`) and `csv-core` under
`csv`. `calamine` is pinned to 0.36.1, so that a new release, and the version of
`zip` it brings, is a decision rather than an update.

The text of a PDF document is extracted in Rust by `pdf_oxide` (MIT or
Apache-2.0), page by page, each page with its number. It is pure Rust, reads
only, links no system library and opens no connection; its default features are
off, so neither its colour-profile engine nor its rendering, OCR or signature
modules enter the build, and only `legacy-crypto` is kept, so that a document
locked with an owner password alone still opens. A page that draws no letter
and no digit is marked as a page without text, the page that optical character
recognition has to read. A document that needs a password to be opened, or a
damaged one, is a typed error, and a defect of the parser on a malformed file
is caught and reported as a damaged file, never a crash. It was measured before
it was adopted, on twenty real French documents published under the Licence
Ouverte (contract documents and terms of use, budgets and price lists,
methodological reports), with five accented witness phrases per document taken
from an independent extractor: nineteen documents of twenty yield all five
phrases with every accent intact; the twentieth loses one phrase whose words a
table cell spreads over two lines. The corpus is versioned with the tests that
read it, and PDFium, the native fallback considered, is not shipped.

The cost is named: `pdf_oxide` brings fifty-three crate versions the lock file
did not hold, among them a font stack (`skrifa`, `read-fonts`, `write-fonts`,
`ttf-parser`, `subsetter`), a layout engine (`taffy`) and second majors of
`quick-xml`, `brotli`, `sha2` and `aes`. `pdf_oxide` is pinned to 0.3.78, and
`office_oxide`, which it depends on, is held at 0.1.11 in the lock file: its
0.1.12 release no longer compiles under `pdf_oxide` 0.3.78, so an update of
either is a decision rather than a refresh.

One crate of that font stack carries an advisory: `ttf-parser` 0.25.1, which
`pdf_oxide` alone uses to parse the fonts a document embeds, is flagged
unmaintained (RUSTSEC-2026-0192). No vulnerability is known in it and no newer
release exists to move to, so the advisory is exempted in `deny.toml`, dated
2026-10-04, with that reason beside it, and it is revisited at the next minor
release. It lifts when `pdf_oxide` drops `ttf-parser` or a maintained release
of it appears.

Word documents and PowerPoint presentations are read in Rust, by code of our
own over two crates: `zip`, the one that opens the package archive (see
[Package archive](#package-archive)), opens the file, and `quick-xml` (MIT), a
streaming XML reader, reads its parts. A Word document yields the paragraphs
and the table rows of its body, one row per segment, in the order of the
document and each with its rank; a presentation yields the text and the speaker
notes of each slide, slides in numeric order, each with its number. Headers,
footers, comments and floating text boxes are not read. A file in the legacy
binary formats, `.doc` or `.ppt`, is a typed error naming the formats that are
read. The archive comes from elsewhere, so reading it is bounded: more than
10,000 entries, or more than 64 MiB decompressed across the parts read, and the
file is refused, the bytes being counted as they are inflated rather than as
the archive declares them. A dedicated crate was not taken: `docx-rs`, the
one weighed for Word, is oriented to writing and does not read PowerPoint, and
`office_oxide`, which the PDF reader brings, is held at an old release (see
above) and is not called directly.

The cost is named: no crate enters the lock file. `quick-xml` 0.41 was already
built for the spreadsheet reader and the desktop bundle, and is now a direct
dependency with its default features off; `zip` was already a direct
dependency. What these crates do not cover is ours to maintain: the structure
of the two formats, read here only as far as their text.

Scanned pages and pictures are read by optical character recognition, with an
engine compiled into the binary. A page of a PDF document that draws no text
and is a single picture is taken as the document embeds it, without rendering
the page, and recognised; the `.png`, `.jpg` and `.tiff` files of a source are
read the same way. The engine is Tesseract, over Leptonica, both built from
source by `tesseract-rs` (MIT; Tesseract is Apache-2.0, Leptonica BSD-2-Clause)
and linked statically: the machine that runs Apollia installs nothing, and the
binary depends on no Tesseract or Leptonica library. Leptonica is built without
its image codecs, so pictures are decoded in Rust by `image` (MIT or
Apache-2.0), with its PNG, JPEG and TIFF decoders only. The recognised text
lives in the index of the source and nowhere else: no file is written beside
the documents.

The engine reads with one data file per language. French and English, from the
`tessdata_fast` set (Apache-2.0), are entries of the model registry, each
pinned by its SHA-256. They are fetched the first time a source holds a scanned
page, once per run, by the downloader that checks every model against its
digest; a machine without the network takes the same files from a disk through
the model import, which accepts a language data file only when its digest is a
pinned one. A file whose digest is not pinned is never handed to the engine.
Until the data is there, each page that waits is listed in the state of its
source as waiting for recognition, and is read once it arrives; a page without
text that is not a single picture is listed as not read. No rendering engine is
shipped: `hayro` and PDFium, the ones weighed, stay out until documents show
the need.

The cost is named: the C++ toolchain enters the build. Compiling the engine
needs cmake, a C++ compiler and the network, takes one to two minutes (83
seconds on macOS Apple Silicon, 133 seconds on Linux x64 under emulation), and
adds 2.8 MB to the binary on macOS and 4.4 MB on Linux. The build script of
`tesseract-rs` downloads the sources of Tesseract 5.5.2 and Leptonica 1.87.0
from GitHub without checking a digest, keeps them with the compiled libraries
in a folder of the home directory rather than under `target/`, and fetches two
language files there that the product does not read. Recognition therefore sits
behind the `ocr` feature, off by default: an everyday build compiles none of
it, and a build that ships recognition turns it on. `tesseract-rs` is pinned to 0.4.0, since a release changes the
sources it fetches; three crates enter the lock file, `tesseract-rs` itself and
`aws-lc-rs` with `aws-lc-sys`, which the HTTP client of its build script brings
and which run at build time only. `image` was already built for the PDF reader
and adds none. The static build is measured on macOS and on Linux; it is not
measured on Windows, where the feature stays off until it is.

The results of a batch are written back to the table it read, by default into
a copy next to the original, `<name> (results YYYY-MM-DD).xlsx`, named in the
language of the interface, so that the original is read and never opened for
writing. On explicit request, and for `.xlsx` and `.xlsm` only, they go into the
original itself, after the dated backup copy the batch contract requires; when
the backup fails, nothing is written. In both cases the workbook is edited in
place, by code of our own over `zip` and `quick-xml`: the cells of the added
column, after the last column of the sheet, are inserted into the XML of that
sheet as inline strings, the calculation chain is removed so that the
spreadsheet recomputes on opening, and every other part of the archive is
copied as it is stored, byte for byte. Formatting, charts, pivot tables,
comments and macros are therefore kept, which a library that loads a workbook
and saves it again does not guarantee: `umya-spreadsheet`, the one weighed,
documents charts corrupted, comments lost and pivot tables not read. The edit is
tested against a versioned corpus of real workbooks (conditional formatting, a
chart, pivot tables, comments, a file saved by LibreOffice, a workbook with
macros). A CSV file gets a copy with its column added, never an edit of the
original; a `.xls`, `.xlsb` or `.ods` workbook gets a new `.xlsx` copy written by
`rust_xlsxwriter` (see [Batch export](#batch-export)), its values only.

The cost is named: no crate enters the lock file, `rust_xlsxwriter` becoming a
direct dependency of the readers. What is ours to maintain is the edit of the
sheet, a small part of a format whose other parts are never rewritten.

A mailbox connects over IMAP with an application password, so any provider is
reachable, Gmail included, without an OAuth client and without a relay. The
client is `async-imap` (MIT or Apache-2.0), on the Tokio runtime, over the
`tokio-rustls` TLS stack the runtime already carries; it brings no OpenSSL.
Messages are decoded by `mail-parser` (Apache-2.0 or MIT), which has no
dependency outside Rust. The server certificate is checked against the root
certificates of the system, read by `rustls-native-certs` (Apache-2.0, ISC or
MIT), already present in the build through the HTTP client and now a direct
dependency.

The access is read-only, and it is enforced in the client rather than promised:
folders open with `EXAMINE`, never `SELECT`, messages are fetched with
`BODY.PEEK` so they stay unread, and no command that writes (`STORE`,
`EXPUNGE`, `COPY`, `MOVE`, `APPEND`) is ever sent. Sending, moving or deleting
mail is out of reach. The password lives in the configured secret store, never
in the connection list, a log or the journal; because `async-imap` traces the
commands it sends through the `log` facade, a login included, every binary
turns its log target off.

The cost is named: two crates and their small trees (`imap-proto`, `hashify`,
`stop-token`, `async-channel`) enter every build, for a protocol whose
servers vary in how strictly they follow it. A server that misreads a command
answers a typed protocol error rather than a crash.

## Batch export {#batch-export}

The result of a batch is exported as an Excel workbook, as CSV or as JSON, one
row per item. CSV and JSON are written without a dependency. The workbook is
written by `rust_xlsxwriter` (MIT or Apache-2.0), a writer only, with no
network access, pinned to 0.92.0: it is the last release that depends on the
major version of `zip` the workspace already carries, and a later one would
put two versions of `zip` in every build. It adds three crates to the lock
file, itself, `zopfli` and `zlib-rs` (see [Package archive](#package-archive)).

`rust_xlsxwriter` cannot change a workbook that already exists. Writing results
back into the file a batch was read from is decided with the reader of
spreadsheets (see [Documents and sources](#documents-and-sources)).

The desktop application writes one more workbook with it: the demonstration
dataset an example agent can be tried on, built from a table the application
carries when the user asks for a copy. No crate enters the lock file for it,
`rust_xlsxwriter` becoming a direct dependency of the desktop.

## Agent-to-agent messaging {#agent-messaging}

A director agent delegates to worker agents by skill identifier, over a durable
mailbox backed by SQLite. Delivery is leased: a consumer takes a lease, and
acknowledgement is fenced on the run holding it, so a stale consumer whose lease
was reassigned cannot delete or requeue a message the new owner is processing.

Dispatch propagates the full skill identifier. A shorter dispatch key was used
once and produced ambiguity as soon as two workers exposed similarly named
skills.

## Audit and evidence {#audit-and-evidence}

There are two registers, and the guarantee is not the same on each.

The tool-invocation trail holds what ran, a hash of its inputs, whether it
succeeded, and how long it took. A call that failed is persisted as failed. It is
written fire-and-forget, so recording never blocks execution, and a record is
dropped when the channel is saturated: the trail is a best-effort record, not a
complete one. Its rows are flat: the input hash each row carries is not chained
to the row before it, and no row is signed.

The hash-chained journal is the register that carries the evidence. Its entries
are chained and signed, and the chain is anchored globally so that truncation is
detectable: removing the tail of the journal breaks a verification a reader can
run themselves.

Both registers cover the agent path. A tool call made in a chat session reaches
neither.

Replaying a run and comparing it against its record is deliberately not built.
Re-execution proves that a second run behaved a certain way, not that the first
one did, and accountability already rests on the signed chain. It is named here
so its absence reads as a choice rather than a gap.

## Secrets and API authentication {#secrets-and-api-auth}

Secrets live in the OS keychain, or in an age-encrypted file where no keychain
exists. They are never written to configuration files.

<!-- claim:daemon-binds-tcp-by-default -->
The HTTP API listens on a Unix socket and on a TCP port; the daemon binds both on
every start, and the port number is the only thing `--port` decides. The Unix
socket is local-trust and relies on filesystem permissions. TCP requires a
bearer token on every `/api/v1` path but one, and binding it to a non-loopback
address without a token is a startup error rather than a warning: an
unauthenticated remote bind is the one mistake that cannot be undone once
traffic has flowed. TLS is optional: with `[api] tls_cert` and `tls_key` set,
the TCP listener serves HTTPS, and a certificate that cannot be loaded fails the
start instead of falling back to cleartext.

The exemption is `GET /api/v1/health`, which answers without a token and reveals
the product marker and the runtime version, nothing else. A process that finds
the port taken holds no token of this installation, and what it needs to know is
whether an Apollia runtime is the holder and in which version. Every other
`/api/v1` route keeps the token.

### The compatible server and the local network {#local-server}

The runtime has two listeners. The native API, port 7771 and the local socket,
drives the runtime; the compatible server, port 7772 by default, serves the
OpenAI and Anthropic formats to other applications. The native API stays on the
machine: the embedded runtime of the desktop binds it to `127.0.0.1` whatever
the file says, the daemon to the loopback `[api] bind` names by default (another
address needs a token, as above), and no setting of the compatible server moves
it. The compatible server is the only surface the user opens to the local
network, by `[server] serve_on_lan`, which is off by default and turned on only
by an explicit action of the user.

The network and the tokens are independent switches for models. A device of the
local network can call the models without a token when `[server] require_auth`
is off, as other local inference servers allow; that choice belongs to the
user. Agents and workflows are not models: they reach the user's connections,
documents and mail, so a call to one from the local network is admitted only
with a token of the server in use, whatever `require_auth` says. Each request
carries its origin, the machine or the local network, read from the peer
address when its connection is accepted. The organisation policy is where an
organisation forbids the local network or imposes tokens, overriding the user's
switches.

Read against the local-first principle: nothing leaves the machine until the
user opens the network, and what then answers without a token is inference on
the prompt the caller sent itself. The user's data, which only agents and
workflows reach, never answers the network without a token the user created and
can revoke. The native API, which holds every other capability, is never
reachable from the network through this switch. Traffic on the network is
cleartext HTTP: the compatible server does not terminate TLS.

## Signatures {#signing-schemes}

Two signature schemes coexist, one per kind of signer, and neither needs a
network to verify.

Agent packages and organisation policies are signed by their author, with a key
the application generates. The signature is a detached Ed25519 signature over a
canonical list of `(path, SHA-256)` entries sorted by the bytes of the path, so
the order in which files were collected never changes it, and adding, removing
or altering a single file breaks it. It travels in a versioned `SIGNATURE` file
that names the format version, the fingerprint of the key (the SHA-256 of the
public key), the publisher name the author declared, and the signature. The
private key is stored through the secret store and never written in clear to
disk. The publisher name is a declaration and is not covered by the signature:
what identifies an author is the key fingerprint.

Distribution metadata published by the Apollia release pipeline (the digest
manifest of the inference engines and of Node.js, the model registry index) is
signed in the minisign format, with a dedicated distribution key. It is the same
format as the desktop updater key and a distinct key, held in its own pipeline
secrets (`APOLLIA_DIST_SIGNING_KEY`, `APOLLIA_DIST_SIGNING_KEY_PASSWORD`).
Signing happens in the pipeline with the Tauri signing tool; the application
only verifies, and the daemon verifies without Tauri, which is why the verifier
is a direct dependency rather than one reached through the updater plugin.

minisign was not taken for packages and policies: it signs one file, not a list
of entries, and the key generation and storage the application needs would have
to be written anyway. Sigstore and X.509 were rejected for both uses: one needs a
transparency log and an identity provider reached over the network, the other a
certificate authority, and either makes an outside party a condition of
verification, against the local-first principle.

The cost is a new dependency surface and no revocation. `ed25519-dalek` 2 is
compiled without default features (no precomputed tables), and brings `ed25519`
and `signature`; its curve arithmetic, `curve25519-dalek`, was already in the
tree. `minisign-verify` has no dependency of its own and was already in the tree
through the updater. A key cannot be revoked other than by changing the set of
keys a reader trusts.

## Organisation policy {#organisation-policy}

An organisation imposes its rules on a machine by dropping a signed policy in a
system folder: `/Library/Application Support/Apollia/policy` on macOS,
`C:\ProgramData\Apollia\policy` on Windows, `/etc/apollia/policy` on Linux. The
folder holds `policy.toml`, a `SIGNATURE` in the package format above over a
list of one entry, and `org.pub`, the organisation public key. The runtime reads
it once, at startup. No folder means no policy: the user is free.

The key sits next to the policy, so the policy is only as authentic as the
rights on its folder. A folder or file that someone other than the system
administrator can modify is refused: on Unix the owner must be root and neither
the others nor a group other than root's may write; on Windows the owner must be
the Administrators group or the LocalSystem account, and no entry of the access
list may grant a write to anyone else. A symbolic link is refused on every
platform. The Windows path is written out rather than read from `%ProgramData%`,
which the user sets.

The date of signature is a `signed_at` field of `policy.toml`, a date, a time
and an offset. The signature covers the whole file, so the date is signed and
the `SIGNATURE` format did not change for it. A file system date was rejected:
anyone can touch a file.

Reading a Windows owner and access list takes the Win32 security API, which the
standard library does not expose. `windows-sys` is a direct dependency of the
permissions crate on Windows only, with the security, authorization and file
system features. It is the raw binding Microsoft publishes, it has no
dependency beyond its own linker shim, and the same version was already in the
tree through other crates, so the lockfile gained no package. The cost is a
block of FFI code with its own `unsafe` allowance, confined to one module.

## The configuration file {#configuration-file}

`apollia.toml` has one writer, `apollia_core::config::writer`. The settings
screens of the desktop application, the command line, the routes under
`/api/v1/config` and the backends the runtime mirrors into the file all go
through it, so the same four checks hold whoever writes: the text validates
against the strict schema, no line gives a key the organisation policy fixes
another value, the file on disk is still the one the caller read, compared by
its SHA-256 fingerprint, and the text replaces the file through a sibling file
renamed over it. A write by key keeps the comments and the order of the file.
The command line calls the writer directly, so it works with the daemon
stopped, and a refusal reads the same sentence on every surface.

Holding the writer in the foundation crate puts `toml`, `toml_edit` and `sha2`
among the direct dependencies of `apollia-core`. All three were already
workspace dependencies and the lockfile gained no package.

## Command line {#cli}

The CLI is noun-verb. Daily operations are bare verbs; everything else is a noun
carrying subcommands. `--json` is global and a terminal is auto-detected, so one
command serves a human and a script.

Exit codes are a contract: 0 success, 1 usage error, 2 runtime error, 3 task
failed, 4 timeout, 5 canceled. A caller branches on them without parsing output.

## Desktop application {#desktop}

The desktop app is Tauri with a Svelte frontend, sharing the same runtime and
the same embedded Python interpreter as the CLI. It is a second front end over
one runtime, not a second implementation.

All user-facing text goes through the internationalization layer with parallel
English and French entries. Color, spacing, and typography come from design
tokens; a hardcoded value duplicating a token is a value that will not follow a
theme change.

### The runtime in the window or beside it {#desktop-runtime-modes}

The runtime runs in one of two places: inside the desktop process, or as a
separate daemon, `apollia-os start` installed as a service of the user, to which
the desktop attaches. The daemon is what lets agents and scheduled work go on
with the window closed, and it is the same runtime, not a second one.

The TCP port decides. At launch, before the Python interpreter, any database or
any listener, the desktop binds the runtime port. Winning it means no runtime
serves the machine: the desktop starts one in its own process, on that very
listener, which is never released between the decision and the serve. Losing it
means something holds the port, and the desktop asks it `GET /api/v1/health`,
whose body carries a product marker and a version: a compatible Apollia runtime
is attached to, anything else is refused. A probe followed by a bind was
rejected, because the moment between the two belongs to whichever process takes
the port first.

Refused: two runtimes on one machine, attaching to a listener that does not say
it is Apollia or to a runtime whose major or minor version differs, and a launch
that ends without a word. A refusal opens a window that names what holds the
port and the gesture to make, and appends the reason to `startup-refused.log` in
the data directory.

Attached, the desktop builds nothing of a runtime of its own. It reaches the
daemon over the local HTTP API with the token of the data directory, and
receives the whole event bus as one stream, `GET /api/v1/events`. It asks the
health route every two seconds rather than assuming the daemon is there, so an
unanswered probe shows the runtime as disconnected at once. A command without a
route yet answers a typed error that names it, instead of reaching for state
the process never built; `scripts/check_attached_coverage.py` counts the routed
commands against the registered ones, on a descending ratchet. A command that
belongs to the window rather than to the runtime (quitting, restarting,
checking for an update, listing the microphones) needs no route: it is named
in a list beside the routed ones, with the reason it reads no runtime state,
and the same script refuses an entry without one. Quitting the attached
desktop leaves the daemon serving.

Version 0.3.0 delivers both modes.

### One application, and a package file that opens it {#desktop-single-instance}

The `.apo` extension is associated with the desktop application
(`bundle.fileAssociations` in `tauri.conf.json`), so a package opens the way a
document does. Opening a package never installs it: whatever brings the file,
the application shows the preview of the package, and the package is installed
or updated only from that preview. Three gestures lead there: opening the file
from the system, dropping it on the window, and the Install button, which opens
a file chooser.

A file opened from the system has to reach the application that already runs,
not start a second one beside it. macOS keeps one process per application on its
own and delivers the file as an event, at launch and afterwards. Windows and
Linux start a new process with the path among its arguments; there
`tauri-plugin-single-instance` makes that second process hand its arguments to
the first and exit, and the first shows its window. The plugin is registered
before any other, in both runtime modes, and on Windows and Linux only: on macOS
it would add a second mechanism where the system already provides one. The
process keeps the paths until the interface takes them, since a file that starts
the application arrives before the interface is loaded. A file dropped on the
window comes through the drag-and-drop event of the webview, with no plugin.

The cost is one more Tauri plugin to follow, which brings no library the tree
did not already hold, and a code path that differs between macOS and the two
other systems, so each has to be checked on its own machine. On Windows and
Linux a second launch can no longer open a second window.

Refused: a deep link (`apollia://install`), which does not answer a double-click
on a file, and offering the button and the drop alone, which leaves a package
unable to open like a document.

## Agent distribution {#agent-distribution}

An agent is installed from a local path or a Git URL. Publishing agents to a
package index was considered and rejected: it would make the index an
availability dependency of a product whose first promise is running without one.

Third-party Python dependencies declared by an agent are installed into that
agent's own virtual environment, after explicit consent, and the operator sees
the list before it happens.

## Package archive {#package-archive}

An agent package travels as one `.apo` file: a zip archive with the
`apollia.toml` manifest at its root, the Python modules and resources of the
package at their relative paths, and, when the author signed it, a `SIGNATURE`
file (see [Signatures](#signing-schemes)) and a `SIGNATURE.key` file holding the
public key that made it. The signature covers every other entry. Building the
archive validates the manifest and refuses a folder whose modules define tools
or workflows that the manifest does not describe; it runs no Python.

Zip was taken over a `tar.gz`: any operating system opens it without a tool,
and its central directory lets the manifest be read without decompressing the
rest, which is what an install preview needs. A `tar.gz` has no index, so
reading one file means decompressing everything before it, and it would have
cost the same two direct dependencies (`tar` and `flate2`) for none of that.
Reading zip with code of
our own was rejected: an archive parser is an attack surface, and an unaudited
one is a worse surface than a maintained one.

An archive comes from elsewhere, so reading it is bounded. An entry with an
absolute path, a `..` component, a backslash or a drive prefix is refused, as is
a symbolic link and a name that appears twice. The archive is capped at 256 MiB,
4096 entries, 64 MiB per entry decompressed and 512 MiB decompressed in all, and
the manifest at 1 MiB. The caps apply to the bytes actually decompressed, not to
the sizes the archive declares, so a lying header does not lift them. A package
over any cap is refused at build time too, so what Apollia builds it can read.

The cost is two direct dependencies of the runtime, `zip` and `flate2`, both
already in the tree through the desktop updater; no crate entered the lock
file. `zip` is compiled with deflate only: no encryption, no bzip2, lzma, zstd
or deflate64. `flate2` is declared beside it for one reason, to choose the
pure-Rust backend (`miniz_oxide`) of that deflate: `zip` leaves the backend to
the caller, and `flate2` refuses to compile without one.

The Excel export of a batch (see [Batch export](#batch-export)) changed one
part of this. Its writer turns on the `deflate` feature of `zip`, which brings
two pure-Rust crates, `zopfli` (Apache-2.0) and `zlib-rs` (Zlib), and makes
`zlib-rs` the deflate backend of `flate2` across the build. Both were accepted
rather than writing the workbook by hand over `zip` alone. The read bounds
above are unchanged.

The packages Apollia ships inside its own binary, the in-app guide first, are
not signed. The desktop assembles each of them on start from files embedded at
compile time and installs it through the same path as any package. The
installation knows them as embedded because the desktop says so, never because
an archive says so: nothing inside a file can claim that origin, and the same
archive handed over from elsewhere is a plain unsigned package. An embedded
package installs with no notice about its signature, and an organisation policy
that requires signed packages does not refuse it. The exemption is bounded to
that origin. Its reason is that the binary carrying the package is what the
distribution signature covers, so signing the package again would prove
nothing more. Like any package, it is recorded in the audit journal, a newer
installed version is never downgraded, and the user's overrides and the
disabled state of its agents survive an update.

The example packages follow the same rule, with one difference in what happens
after. They are declarative packages embedded in the runtime at compile time,
from one folder of the source tree. The application proposes them and installs
one only when the user chooses it, after the preview every package gets. An
example is named to the runtime by its package name, never by a path, so only
what the binary carries is treated as an example; it is unsigned, and neither
flagged nor refused for it, for the reason above. Once installed it belongs to
the user: unlike the guide, it is uninstalled like any package.

A package tool runs with a bare context: no memory, no user profile. One
exception is decided by the host, never by a package. The onboarding of
Apollia ships as a declarative agent with one tool that records the profile
facts of the first conversation, the right the onboarding agent held when it
ran code. The runtime grants that tool the user profile, writable, and the
memory of the onboarding, which the desktop reads to learn that the
onboarding is complete. The grant names the package, the module and the
source the binary carries: a tool whose module on disk is not that source byte
for byte, whatever its package is called, gets nothing.

## Host integration {#host-integration}

The versioned HTTP API is a driving contract, not an implementation detail. It
carries a generated OpenAPI schema and generated TypeScript and Python clients,
so a host product drives the runtime without reverse-engineering route modules.

Breaking changes go through a new version prefix, never through a silent
mutation of the current one.

## Public error surface {#public-error-enums}

Every public error enum in the workspace carries `#[non_exhaustive]`, and
`scripts/check_rust_rules.py` refuses one that does not. Adding a variant to an
error type is then an ordinary change rather than a breaking one for every crate
that matches on it, which is what the attribute exists to absorb.

`apollia-core` is sealed like the rest, and it is the crate where the attribute
earns the most: every other crate depends on it, so an unabsorbed variant there
would break the whole workspace at once.

The cost lands on the matching side, and only across a crate boundary. A match
on one of these enums from another crate needs a fallback arm, and the compiler
stops asking that match to be exhaustive, so a variant added later reaches the
fallback instead of failing the build. Three cross-crate matches were exhaustive
when the attribute went on, all three mapping a runtime error to an HTTP status;
each now answers an unmatched variant with a 500 carrying the error's own
message rather than with a panic.

## Platforms and release {#platforms-and-release}

Linux, macOS, and Windows are supported targets. Tool confinement differs
between them, as stated above.

Releases are published on GitHub Releases. The desktop app checks that feed only
when the operator asks, never in the background, and reports an empty feed as an
empty feed rather than as an error.

## Verification {#verification}

Beyond the test suite, two classes of property are checked by machine rather
than by review: concurrency interleavings of the actor algorithms, and the two
cardinal invariants, the non-bypassable step budget and the mailbox lease fence,
proven under bounded symbolic execution.

These run on a schedule rather than on every change, because they are slow. Each
has an in-tree counterpart running with the ordinary test suite, so a regression
surfaces at the usual moment and the scheduled job confirms it.

## Documentation {#documentation}

This site is the documentation, organized by what a reader is trying to do:
tutorials to learn, how-to guides to accomplish, reference to look up,
explanation to understand.

The command-line and SDK references are generated from the code and committed,
with a check that fails when the committed pages drift from what the code would
produce. A reference that can drift silently is worse than no reference.
