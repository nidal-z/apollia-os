# Agents — Adapter des Agents Existants — Apollia OS

> Comment intégrer un agent LangGraph, CrewAI ou AutoGen existant dans Apollia OS avec un minimum d'adaptation.
> Public cible : développeur Python intermédiaire avec un agent existant

---

## Vue d'ensemble

Apollia OS ne requiert que deux méthodes : `manifest()` et `async run`. Tout agent existant peut être adapté en créant une mince couche d'adaptation qui :
1. Expose un `manifest()` décrivant les capacités de l'agent
2. Traduit l'`AIPTask` vers le format d'entrée de l'agent existant
3. Traduit la réponse de l'agent vers un `AIPResult`

---

## Pattern d'adaptation universel

```python
# adapter.py

class AgentAdapter:
    """Couche d'adaptation AIP autour d'un agent existant."""

    def __init__(self):
        # Initialisation lazy — l'agent sous-jacent est créé dans on_start()
        self._underlying_agent = None

    def manifest(self):
        return {
            "name": "mon-agent-adapte",
            "version": "1.0.0",
            "description": "Agent XYZ adapté pour Apollia OS",
            "tools_required": [],
        }

    async def on_start(self, ctx):
        # Initialiser l'agent sous-jacent ici, pas dans __init__
        # (on_start est appelé avec le contexte runtime disponible)
        self._underlying_agent = creer_mon_agent()

    async def run(self, task, ctx):
        # 1. Extraire l'entrée de l'AIPTask
        parts = task["input"]["parts"]
        user_input = parts[0]["text"] if parts else ""

        # 2. Appeler l'agent sous-jacent
        response = await self._underlying_agent.arun(user_input)

        # 3. Retourner un AIPResult
        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": str(response)}],
        }

    async def on_stop(self):
        # Libérer les ressources si nécessaire
        self._underlying_agent = None

agent = AgentAdapter()
```

---

## LangGraph

### Agent ReAct LangGraph existant

```python
# Avant adaptation — agent LangGraph standalone
from langgraph.prebuilt import create_react_agent
from langchain_openai import ChatOpenAI

llm = ChatOpenAI(model="gpt-4o-mini")
tools = [...]  # vos outils LangChain
langgraph_agent = create_react_agent(llm, tools)

# Appel direct (sans Apollia)
result = await langgraph_agent.ainvoke({"messages": [("user", "...")]})
```

### Adaptation AIP

```python
# langgraph_adapter.py
from langgraph.prebuilt import create_react_agent
from langchain_openai import ChatOpenAI


class LangGraphAdapter:
    def __init__(self):
        self._agent = None

    def manifest(self):
        return {
            "name": "langgraph-react-agent",
            "version": "1.0.0",
            "description": "Agent ReAct LangGraph avec outils natifs",
            "tools_required": [],   # pas d'outils Apollia si LangChain gère les siens
            "memory_namespace": "langgraph-agent",  # mémoire persistante optionnelle
            "step_budget": {
                "max_steps": 20,
                "max_tool_calls": 40,
                "wall_clock_timeout_secs": 120
            }
        }

    async def on_start(self, ctx):
        llm = ChatOpenAI(model="gpt-4o-mini")
        tools = [...]
        self._agent = create_react_agent(llm, tools)

    async def run(self, task, ctx):
        parts = task["input"]["parts"]
        user_input = parts[0]["text"] if parts else ""

        # Contexte mémoriel (optionnel)
        context = []
        if ctx.memory:
            results = await ctx.memory.search(user_input, limit=3)
            context = [r["content"] for r in results]

        messages = [("user", user_input)]
        if context:
            system_msg = "Contexte pertinent :\n" + "\n".join(context)
            messages = [("system", system_msg)] + messages

        try:
            result = await self._agent.ainvoke({"messages": messages})
            final_message = result["messages"][-1].content

            # Mémoriser le résultat
            if ctx.memory:
                await ctx.memory.record(
                    f"Q: {user_input[:100]} → R: {final_message[:100]}",
                    importance=0.6,
                    task_id=task["task_id"]
                )

            return {
                "task_id": task["task_id"],
                "status": "completed",
                "output": [{"type": "text", "text": final_message}],
            }
        except Exception as e:
            return {
                "task_id": task["task_id"],
                "status": "failed",
                "error": {"code": "AGENT_ERROR", "message": str(e)}
            }

    async def on_stop(self):
        self._agent = None

agent = LangGraphAdapter()
```

---

## CrewAI

### Crew existante

```python
# Avant adaptation — CrewAI standalone
from crewai import Agent, Task, Crew

researcher = Agent(role="Researcher", ...)
writer = Agent(role="Writer", ...)
crew = Crew(agents=[researcher, writer], tasks=[...])
result = crew.kickoff(inputs={"topic": "..."})
```

### Adaptation AIP

```python
# crewai_adapter.py
from crewai import Agent, Task, Crew


class CrewAIAdapter:
    def manifest(self):
        return {
            "name": "crewai-research-writer",
            "version": "1.0.0",
            "description": "Crew de recherche et rédaction CrewAI",
            "tools_required": [],
            "max_concurrent_tasks": 1,  # CrewAI n'est pas thread-safe
            "step_budget": {
                "wall_clock_timeout_secs": 300
            }
        }

    async def run(self, task, ctx):
        import asyncio

        parts = task["input"]["parts"]
        topic = parts[0]["text"] if parts else ""

        researcher = Agent(
            role="Researcher",
            goal=f"Rechercher des informations sur : {topic}",
            backstory="Expert en recherche documentaire",
        )
        writer = Agent(
            role="Writer",
            goal="Rédiger un résumé clair",
            backstory="Rédacteur technique expérimenté",
        )

        research_task = Task(
            description=f"Rechercher sur : {topic}",
            agent=researcher
        )
        write_task = Task(
            description="Rédiger un résumé basé sur la recherche",
            agent=writer
        )

        crew = Crew(
            agents=[researcher, writer],
            tasks=[research_task, write_task]
        )

        # CrewAI est synchrone — exécuter dans un thread séparé
        result = await asyncio.get_event_loop().run_in_executor(
            None,
            lambda: crew.kickoff(inputs={"topic": topic})
        )

        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": str(result)}],
        }

agent = CrewAIAdapter()
```

---

## AutoGen

```python
# autogen_adapter.py
import autogen
import asyncio


class AutoGenAdapter:
    def manifest(self):
        return {
            "name": "autogen-conversation",
            "version": "1.0.0",
            "description": "Conversation multi-agents AutoGen",
            "tools_required": [],
            "max_concurrent_tasks": 1,
            "step_budget": {
                "max_steps": 30,
                "wall_clock_timeout_secs": 180
            }
        }

    async def run(self, task, ctx):
        parts = task["input"]["parts"]
        user_input = parts[0]["text"] if parts else ""

        config_list = [{"model": "gpt-4o-mini"}]
        assistant = autogen.AssistantAgent(
            name="assistant",
            llm_config={"config_list": config_list}
        )
        user_proxy = autogen.UserProxyAgent(
            name="user",
            human_input_mode="NEVER",
            max_consecutive_auto_reply=5,
        )

        # AutoGen est synchrone
        result = await asyncio.get_event_loop().run_in_executor(
            None,
            lambda: user_proxy.initiate_chat(
                assistant,
                message=user_input,
                summary_method="last_msg"
            )
        )

        summary = user_proxy.last_message(assistant)["content"]
        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": summary}],
        }

agent = AutoGenAdapter()
```

---

## Points d'attention communs

**Initialisation lazy dans `on_start()` (optionnel)**

> `on_start(ctx)` et `on_stop()` sont des hooks **optionnels** — seuls `manifest()` et `run(task, ctx)` sont requis par le contrat AIP (voir ADR-003). Le runtime appelle `on_start()` si la méthode existe, sinon il passe directement à `ACTIVE`.

Les agents avec des modèles LLM lourds à charger doivent être initialisés dans `on_start()`, pas dans `__init__()`. `on_start()` reçoit le `RuntimeContext` complet et est appelé quand l'agent passe à `ACTIVE`.

**Agents synchrones**

CrewAI, AutoGen et certains frameworks sont synchrones. Utiliser `asyncio.get_event_loop.run_in_executor(None, lambda:...)` pour les exécuter sans bloquer le runtime Tokio.

**`max_concurrent_tasks: 1` pour les frameworks non thread-safe**

Si le framework sous-jacent maintient un état global (souvent le cas pour CrewAI), forcer `max_concurrent_tasks: 1` dans le manifest.

**Gestion des exceptions**

Toujours catcher les exceptions du framework sous-jacent et retourner un `AIPResult` avec `"status": "failed"` plutôt que de laisser propager l'exception.

---

## Voir aussi

- [Briques AIP Specification](./Briques-AIP-Specification) — contrat complet AIPTask, AIPResult
- [Agents RuntimeContext Guide](./Agents-RuntimeContext-Guide) — `ctx.memory`, `ctx.tools`
- [Agents Bonnes Pratiques](./Agents-Bonnes-Pratiques) — StepBudget avec agents LLM
- [ADR-003](../adr/ADR-003-duck-typing-aip) — pourquoi duck typing rend l'adaptation facile
