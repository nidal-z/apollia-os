# Apollia, capabilities

Apollia runs AI agents on your own machine. Your data stays there unless you
connect a service yourself. It works with a local model, or with a model
server you choose to connect.

## Agents

An agent is a description, not a program: a name, a mission (what it is
for, in plain words), example requests, the model it uses, and the accesses
it is granted (tools, connections, workflows). Apollia runs every agent with
the same engine. Each agent has a card on the Agents screen where you read
what it does and what it may use. You can adjust an agent on its card; your
adjustments are kept when its package is updated, and you can go back to the
package defaults at any time. A disabled agent does not run until you enable
it again.

## Packages

Agents travel in packages: one `.apo` file that can hold several agents,
workflows and Python tools. Before a package is installed, Apollia shows what
it contains, what each agent asks to use, who published it and whether its
signature holds. An unsigned package installs with a warning. An update shows
what it adds before you accept it, and never takes back the adjustments you
made. A package never installs a version older than the one you have unless
you allow it.

## Workflows

A workflow is a fixed sequence of steps written by its author: a model call,
a tool, an agent, or Python code. You run it with its inputs, and each step
is recorded so you can follow it.

## Models

Models run on your machine with llama.cpp (GGUF files, Metal or CUDA). You
can also connect a model server: Ollama, any OpenAI-compatible server,
Anthropic or OpenAI. Each agent can be given its own model; otherwise it uses
the default model.

## Tools and connections

Native tools search the web, read a page, fetch a URL, read and write files
in the folders you trust, and run commands in a sandbox. Connections add more:
MCP servers, and Google or Microsoft accounts. An agent only uses the tools it
is granted, and a sensitive action asks for your approval first.

## Memory

Each agent keeps its own memory and reads it when it decides to: nothing is
slipped into its prompt behind your back. You can read and erase what an
agent remembers on the Memory screen.

## Automations

Automations start an agent or a workflow on a schedule, at a fixed interval,
when a file changes, or when a webhook is called.

## Trust and audit

Every run is bounded by a step budget the runtime enforces. Installations,
access changes and runs are recorded in a tamper-evident journal you can
verify and export.

## Screens of the application

Use only these routes in an action block:

- `/dashboard`: what is running and what needs you.
- `/chat`: talk with Apollia or with an agent.
- `/agents`: the agents, their cards, installing a package.
- `/inbox`: the approvals and questions waiting for you.
- `/automations`: schedules, intervals, file watches and webhooks.
- `/integrations`: MCP servers and connected accounts.
- `/llm`: models, local and remote, and the default model.
- `/memory`: what each agent remembers.
- `/observability`: runs, costs and the audit journal.
- `/settings`: preferences, profile, notification channels and security.
