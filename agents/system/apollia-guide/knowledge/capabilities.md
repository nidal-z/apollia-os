# Apollia, capabilities

Apollia runs AI agents on your own machine. Your data stays there unless you
connect a service yourself. It works with a model that runs on this computer,
or with a model server you choose to connect.

## Agents

An agent is a description, not a program: a name, a mission (what it is
for, in plain words), example requests, the model it uses, the accesses it is
granted and the sources it may consult. Nobody has to write code to have an
agent. Apollia runs every agent with the same engine.

You get an agent in three ways, all from the Agents screen:

- **Create one by describing it.** "Create an agent" opens a conversation:
  you say what the agent should do, Apollia drafts its sheet, and you read and
  correct it before it is created.
- **Install a package** someone sent you, or one of the examples the screen
  proposes.
- **Adjust an existing agent** on its sheet. Your adjustments are kept when
  its package is updated, and you can go back to the defaults at any time.

The sheet of an agent says what it does, which model answers for it, what it
may do by family of action, which sources it reads, what it did and what it
remembers. A disabled agent does not run until you enable it again.

## Running an agent on many items

An agent does not only talk. "Process a batch" on its sheet runs it on every
row of a workbook or a CSV file, or on every file of a folder. A batch can be
paused, stopped and retried, survives a restart, and exports its results as
Excel, CSV or JSON.

## Packages

Agents travel in packages: one `.apo` file that can hold several agents,
workflows and tools. Before a package is installed, Apollia shows what it
contains, what each agent asks to use, who published it and whether its
signature holds. An unsigned package installs with a warning. An agent created
in the interface can be exported as a package. An update shows what it adds
before you accept it, and never takes back the adjustments you made.

## Workflows

A workflow is a fixed sequence of steps written by its author. You launch it
from the Agents screen with the inputs it asks for (a text, a date, a file, a
choice), and each step is recorded so you can follow it. A workflow can stop
to ask you a question and go on with your answer.

## Models

A model runs on this computer, or on a server you connect.

- **On this computer**, Apollia picks the engine from the format of the
  model: llama.cpp for GGUF files, MLX on Apple Silicon. The AI screen lists
  the models you have, and its catalogue is sorted by what you want to do
  (chat and write, search your documents, dictate), with the models that fit
  this computer first. Loading a model says whether it fits before it starts.
- **On a server**, you declare the address of any OpenAI-compatible server.
  Apollia has no built-in connection to a cloud provider: to reach a hosted
  model, put a gateway you operate in front of it and declare its address.

Each agent can be given its own model; otherwise it uses the default model.

## Connections and tools

The Connections screen lists everything an agent can reach: the built-in
tools (web search, reading a page, files of the folders you chose, running
code in a sandbox), MCP servers, Google or Microsoft accounts, a mailbox over
IMAP in read-only mode, and folders of this computer. A connection is added
from a curated selection, then granted to an agent family of action by family
of action.

## Sources

An agent can consult sources: folders and documents of this computer,
searched by meaning and by keyword, on the machine. Office documents, PDF
files and mail are read. An agent only reads the sources checked on its sheet.

## What an agent is allowed to do

Every action belongs to one of five families: read, read personal data, write
or delete, send to the outside, run code. Reading asks nothing. For the
others, the first sensitive use asks you, with three answers: once, always
for this agent, or refuse. What waits for your answer is on the To review
screen. An agent cannot grant itself a right. An organisation may add a
signed policy that fixes some settings; without one, you decide everything.

## Memory

Each agent keeps its own memory and reads it when it decides to: nothing is
slipped into its prompt behind your back. You read, search and erase what an
agent remembers on its sheet.

## Automations

Automations start an agent or a workflow on a schedule, at a fixed interval,
when a file changes, or when a webhook is called.

## Journal

Decisions, installations, access changes and runs are recorded in one signed
journal you can read, verify and export on the Journal screen. Every run is
bounded by a step budget the runtime enforces.

## For developers

A developer mode, off by default, reveals the technical surfaces: a local
server that speaks the OpenAI and Anthropic formats for the models and the
agents you expose, Apollia as an MCP server, the logs and the documentation.
Tools and workflows of a package are written in Python with the SDK; agents
never are.

## Screens of the application

Use only these routes in an action block:

- `/chat`: talk with Apollia or with an agent.
- `/agents`: the agents and workflows, their sheets, creating an agent,
  installing a package, batches.
- `/integrations`: the connections (tools, MCP servers, accounts, mailboxes,
  folders).
- `/llm`: the models of this computer, the servers, the catalogue.
- `/inbox`: the approvals and questions waiting for you.
- `/library`: the installed packages and what they contain.
- `/automations`: schedules, intervals, file watches and webhooks.
- `/authorizations`: the rights given to each agent.
- `/journal`: the signed journal of what happened.
- `/settings`: preferences, profile, engines and hardware.
