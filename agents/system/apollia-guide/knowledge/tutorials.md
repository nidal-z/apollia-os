# Apollia OS, tutorials

## Write the smallest agent

An agent is a decorated Python class. `@on_message` handles a chat turn and
returns the reply string.

```python
from apollia import agent, on_message
from apollia.types import Ctx


@agent(name="hello", version="0.1.0", description="Echoes back whatever you send.")
class Hello:
    @on_message
    async def handle(self, message: str, history: list, ctx: Ctx) -> str:
        return f"You said: {message}"


agent = Hello()
```

## Install and run an agent from the CLI

```bash
# Install an agent package from a directory
apollia-os agent install ./agents/examples/hello

# List installed agents
apollia-os agent list

# Start an agent, then submit a task
apollia-os agent start hello
apollia-os task run hello '{"message": "hi"}'
```

## Use memory

Memory is read and written at the agent's initiative through `ctx.memory`. The
runtime never injects it into your agent's prompt.

```python
# Store a value (source and confidence are optional)
await ctx.memory.remember("user.city", "Lyon")

# Recall it later, in this or a future session
city = await ctx.memory.recall("user.city")   # -> "Lyon" or None
```

## Expose a callable skill

```python
from apollia import agent, skill
from apollia.types import Ctx


@agent(name="notes", version="0.1.0", description="Keeps short notes.")
class Notes:
    @skill("notes.add", description="Append a note.")
    async def add(self, text: str, ctx: Ctx) -> dict:
        await ctx.memory.record("note", text)
        return {"stored": text}


agent = Notes()
```

## Delegate to another agent (A2A)

```python
result = await ctx.a2a.invoke("notes.add", {"text": "buy milk"})
```
