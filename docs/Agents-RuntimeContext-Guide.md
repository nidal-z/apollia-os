# Guide — RuntimeContext pour agents Python

> Ce document décrit le `RuntimeContext` (`ctx`) disponible dans les agents Python
> exécutés par Apollia OS. Le `ctx` est le point d'accès unique aux services du runtime.

---

## Vue d'ensemble

Tout agent Python reçoit un objet `ctx` de type `RuntimeContext` dans sa méthode `run()` :

```python
async def run(self, task, ctx):
    # ctx.tools — ToolProxy pour appeler les outils
    # ctx.llm — LlmProxy pour les appels LLM
    # ctx.memory — MemoryInterface pour la mémoire agent
    # ctx.user_context — Contexte utilisateur (mode chat uniquement)
    pass
```

---

## ctx.tools

Proxy vers le Tool Registry Rust. Permet d'appeler les outils natifs et MCP.

```python
result = await ctx.tools.call("bash_executor", {"command": "ls -la"})
tools = await ctx.tools.list_tools()
count = ctx.tools.tool_call_count
```

---

## ctx.llm

Proxy vers le `LlmRouter`. Permet les appels de complétion LLM.

```python
if ctx.llm is not None:
    response = await ctx.llm.complete("Résume ce texte : ...")
    backend = ctx.llm.default_backend
```

Vaut `None` si aucun backend LLM n'est configuré (un événement `AgentDegraded` est émis).

---

## ctx.memory

Interface vers le `MemoryManager` pour la mémoire persistante de l'agent.

```python
await ctx.memory.remember("semantic", "key", "value")
result = await ctx.memory.recall("semantic", "key")
results = await ctx.memory.search("query")
await ctx.memory.forget("semantic", "key")
```

---

## ctx.user_context

**Ajouté Sprint 22** — Contexte utilisateur injecté en mode chat.

### Type

```python
ctx.user_context  # dict[str, list[tuple[str, str]]] | None
```

### Comportement

| Mode | Valeur |
|---|---|
| **Chat (Libre ou Agent)** | `dict` avec les catégories `preferences`, `habits`, `context` |
| **Task** | `None` |

### Structure

```python
{
    "preferences": [("language", "français"), ("format", "markdown")],
    "habits": [("working_hours", "9h-18h")],
    "context": [("current_project", "apollia-os"), ("role", "CTO")]
}
```

### Utilisation

```python
async def run(self, task, ctx):
    user_ctx = ctx.user_context

    if user_ctx is not None:
        # Adapter le comportement selon les préférences
        for key, value in user_ctx.get("preferences", []):
            if key == "language":
                self.language = value

        # Utiliser le contexte
        for key, value in user_ctx.get("context", []):
            if key == "current_project":
                self.project = value
```

### Principe #6

L'agent **décide** quoi faire du contexte utilisateur. Le runtime ne contraint jamais le comportement de l'agent en fonction de ces données — elles sont purement informatives.

---

## Propriétés supplémentaires

| Propriété | Type | Description |
|---|---|---|
| `ctx.agent_name` | `str` | Nom de l'agent en cours d'exécution |
| `ctx.supports_a2a` | `bool` | Si l'agent supporte la communication agent-à-agent |
| `ctx.mailbox` | `AgentMailboxHandle \| None` | Boîte aux lettres pour communication inter-agents |
