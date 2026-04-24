# Apollia OS — Tutorials

## Install and run your first agent

```bash
# Install a package (director + workers + triggers)
apollia agent install ./agents/veille-ia

# List installed agents
apollia agent list

# Start an agent
apollia agent start veille-ia-agent

# Run a task
apollia task run veille-ia-agent '{"text": "Generate today report"}'
```

## Configure a trigger

```toml
# In agent.toml
[[triggers]]
id             = "daily-run"
agent          = "my-agent"
enabled        = true
on_busy        = "skip"
input_template = "Run daily task"

[triggers.source]
type     = "cron"
schedule = "0 8 * * 1-5"
```

## Use memory in an agent

```python
# Store a value
await ctx.memory.remember("my-key", "my-value", namespace="my-agent")

# Recall a value
value = await ctx.memory.recall("my-key", namespace="my-agent")
```

## Delegate to a worker (A2A)

```python
result = await ctx.a2a_invoke("my-skill", {"text": "process this"})
```
