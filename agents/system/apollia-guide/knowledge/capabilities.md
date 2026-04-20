# Apollia OS — product capabilities (apollia-guide knowledge base)

> Source-of-truth for the Apollia Guide coach. Every feature listed here is
> shipped in the current version. **Never invent capabilities that are not
> in this file.** If a user asks about something absent, redirect to docs.

## Core concepts

- **Agents** — autonomous Python programs that run locally in the Apollia
  runtime. Two categories: *assistants* (conversational, operator-facing)
  and *workers* (headless, called by pipelines / triggers).
- **Tools** — typed capabilities an agent may call (filesystem, web, MCP
  servers, notifications, memory…). Always audited and gated by permission
  rules.
- **Automations** — an agent + a trigger (cron, interval, filewatch,
  webhook) wired together. Operator builds these through the wizard.
- **Pipelines** — DAG of agents/tools with fan-out/fan-in, HITL gates,
  fallback branches. Powered by the Pipeline Engine.
- **Memory** — per-agent and per-user semantic store (SQLite + FTS5).
  Episodic / semantic / procedural slots. Always on-device.
- **MCP** — Model Context Protocol servers (local stdio, HTTP, SSE). Apollia
  supports mDNS discovery, hot reload, and HITL gating of dangerous tools.

## Runtime

- Local-first. Everything runs on the user's machine by default.
- The LLM backend is **user-configured** (llama.cpp embedded, Ollama,
  Anthropic, OpenAI, Bedrock, Vertex…). Local backends → fully offline.
- StepBudget: a non-negotiable guard-rail the runtime enforces on every
  agent step. Agents cannot opt out.

## Routes (operator mode deep-links)

- `/dashboard` — daily digest + running tasks.
- `/agents` — installed assistants, install/uninstall/update.
- `/projects` — project scopes (memory + chat isolated per project).
- `/tasks` — running tasks timeline + HITL approvals.
- `/chat` — conversational surface with any assistant.
- `/automations` — schedule-backed agents + wizard (`?wizard=open`).
- `/integrations` — connections (OAuth, MCP servers, webhooks).
- `/inbox` — pending approvals + notifications triage.
- `/onboarding` — resume or replay the onboarding flow.

## Routes (builder mode deep-links)

- `/llm` — configure LLM backends, defaults, cost alerts.
- `/triggers` — cron, interval, filewatch, webhook management.
- `/pipelines` — DAG editor + run history.
- `/memory` — namespaces explorer + FTS5 search.
- `/observability` — global timeline, per-tool audit trail, cost stats.
- `/notifications` — channels (desktop, webhook) + event routing.

## Safety rules

- No action the user hasn't consented to is performed on their behalf.
- Every destructive tool (filesystem writes, shell, network POST) goes
  through HITL unless the user pre-approved a permission rule.
- Apollia Guide itself can **only** navigate and read — never write, never
  delete, never exfiltrate.
