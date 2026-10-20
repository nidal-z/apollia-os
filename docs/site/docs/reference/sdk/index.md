---
sidebar_position: 0
title: SDK / ctx contract
---
<!-- GENERATED FILE. Do not edit; regenerate with docs/site/regen.sh. -->

# SDK / ctx contract

Runtime context passed to every tool and every workflow.

The attributes below are the whole surface the code sees, and their reach
is not uniform: ``ctx.workspace``, ``ctx.notify`` and ``ctx.stt`` are
``None`` on every binary this project ships, and ``ctx.mail`` raises
outside the desktop application. Each service page states its own reach.
Use type hints to get IDE autocomplete::

    @tool("greet", "Greet a known user.", action="read_personal")
    async def greet(ctx: Ctx, name: str) -> dict:
        user = await ctx.memory.recall(f"user.{name}")
        ...

## Services

| Service | Type | Summary |
| --- | --- | --- |
| [`ctx.llm`](./llm.md) | `LlmProxy` | ``ctx.llm`` - LLM backend access. |
| [`ctx.memory`](./memory.md) | `MemoryInterface` | Tri-mode memory: episodic events, semantic key-value, procedural triggers. |
| [`ctx.tools`](./tools.md) | `ToolProxy` | Tool invocation surface - native registry + MCP routing. |
| [`ctx.mail`](./mail.md) | `MailInterface` | Durable, at-least-once inter-agent messaging surface. |
| [`ctx.datasources`](./datasources.md) | `DatasourcesInterface` | Runtime access to the YAML datasources the context was given. |
| [`ctx.templates`](./templates.md) | `TemplatesInterface` | Runtime Jinja2 template rendering. |
| [`ctx.secrets`](./secrets.md) | `SecretsInterface` | Read-only access to the credentials the context was granted. |
| [`ctx.events`](./events.md) | `EventsInterface` | Public typed events for streaming, ReAct observability, error reporting. |
| [`ctx.logger`](./logger.md) | `Logger` | ctx.logger - structured logging via stdlib ``logging``. |
| [`ctx.profile`](./profile.md) | `ProfileInterface \| None` | User profile surface. |
| [`ctx.workspace`](./workspace.md) | `WorkspaceContext` | The sections collected from a workspace, by title. |
| [`ctx.stt`](./stt.md) | `SttInterface` | Audio transcription surface backed by ``apollia-stt``. |
| [`ctx.notify`](./notify.md) | `NotifyInterface` | Notification surface (desktop, webhook, future channels). |
| [`ctx.budget`](./budget.md) | `BudgetView` | Runtime step budget tracking, read-only from the agent's perspective. |
| [`ctx.is_resumed`](./is_resumed.md) | `bool` | ``True`` when this run resumes a task that paused on a human. |
| [`ctx.input_response`](./input_response.md) | `InputResponse \| None` | The operator's response to the pause being resumed, ``None`` on a first run. See `apollia.hitl`. |

See also [Content types and helpers](./content-types.md) for the multi-modal message shapes.
