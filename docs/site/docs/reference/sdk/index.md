---
sidebar_position: 0
title: SDK / ctx contract
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# SDK / ctx contract

Runtime context passed to every agent handler.

Exposes 100% of the Apollia backend through 15 typed services.
Use type hints to get IDE autocomplete::

    @skill("foo.bar")
    async def bar(self, name: str, ctx: Ctx) -> dict:
        user = await ctx.memory.recall(f"user.{name}")
        ...

## Services

| Service | Type | Summary |
| --- | --- | --- |
| [`ctx.llm`](./llm.md) | `LlmProxy` | ``ctx.llm`` - LLM backend access. |
| [`ctx.memory`](./memory.md) | `MemoryInterface` | Tri-mode memory: episodic events, semantic key-value, procedural triggers. |
| [`ctx.tools`](./tools.md) | `ToolProxy` | Tool invocation surface - native registry + MCP routing. |
| [`ctx.a2a`](./a2a.md) | `A2AInterface` | Inter-agent invocation API. |
| [`ctx.mail`](./mail.md) | `MailInterface` | Durable, at-least-once inter-agent messaging surface. |
| [`ctx.datasources`](./datasources.md) | `DatasourcesInterface` | Runtime access to YAML datasources declared in ``@agent(datasources=(...))``. |
| [`ctx.templates`](./templates.md) | `TemplatesInterface` | Runtime Jinja2 template rendering. |
| [`ctx.secrets`](./secrets.md) | `SecretsInterface` | Read-only access to credentials declared in ``@agent(secrets=(...))``. |
| [`ctx.events`](./events.md) | `EventsInterface` | Public typed events for streaming, ReAct observability, error reporting. |
| [`ctx.logger`](./logger.md) | `Logger` |  |
| [`ctx.profile`](./profile.md) | `ProfileInterface` | User profile surface. |
| [`ctx.workspace`](./workspace.md) | `WorkspaceContext` | Snapshot of the workspace at task start. |
| [`ctx.stt`](./stt.md) | `SttInterface` | Audio transcription surface backed by ``apollia-stt``. |
| [`ctx.notify`](./notify.md) | `NotifyInterface` | Notification surface (desktop, webhook, future channels). |
| [`ctx.budget`](./budget.md) | `BudgetView` | Runtime step budget tracking, read-only from the agent's perspective. |

See also [Content types and helpers](./content-types.md) for the multi-modal message shapes.
