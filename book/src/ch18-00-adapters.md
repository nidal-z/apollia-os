# Adapter LangGraph / CrewAI

Vous avez déjà un agent LangGraph, une Crew CrewAI, ou un workflow AutoGen qui fonctionne. Vous ne voulez pas tout réécrire — vous voulez juste l'exécuter dans Apollia OS pour bénéficier de la persistance, des triggers, des pipelines, et du HITL.

Bonne nouvelle : le contrat AIP est minimaliste. Deux méthodes suffisent — `manifest()` et `async run`. Tout framework Python peut être enveloppé en quelques dizaines de lignes.

---

## Le pattern d'adaptation universel

Quelle que soit la source (LangGraph, CrewAI, AutoGen, ou votre propre code), la structure est toujours la même :

```python
class AgentAdapter:
    def __init__(self):
        self._underlying_agent = None

    def manifest(self):
        return {
            "name": "mon-agent-adapte",
            "version": "1.0.0",
            "description": "Description de ce que fait l'agent",
            "tools_required": [],
        }

    async def on_start(self, ctx):
        # Initialisation lazy — appelé quand l'agent passe à ACTIVE
        # ctx.llm, ctx.memory sont disponibles ici
        self._underlying_agent = creer_mon_agent()

    async def run(self, task, ctx):
        parts = task["input"]["parts"]
        user_input = parts[0]["text"] if parts else ""

        response = await self._underlying_agent.arun(user_input)

        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": str(response)}],
        }

    async def on_stop(self):
        self._underlying_agent = None

agent = AgentAdapter()
```

`on_start()` et `on_stop()` sont **optionnels** — seuls `manifest()` et `run()` sont requis par le contrat AIP.

---

## LangGraph

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
            "description": "Agent ReAct LangGraph avec outils natifs LangChain",
            "tools_required": [],
            "memory_namespace": "langgraph-agent",
            "step_budget": {
                "max_steps": 20,
                "wall_clock_timeout_secs": 120
            }
        }

    async def on_start(self, ctx):
        llm = ChatOpenAI(model="gpt-4o-mini")
        tools = [...]  # vos outils LangChain
        self._agent = create_react_agent(llm, tools)

    async def run(self, task, ctx):
        parts = task["input"]["parts"]
        user_input = parts[0]["text"] if parts else ""

        # Enrichir avec la mémoire Apollia (optionnel)
        context = []
        if ctx.memory:
            results = await ctx.memory.search(user_input, limit=3)
            context = [r["content"] for r in results]

        messages = [("user", user_input)]
        if context:
            messages = [("system", "Contexte : " + "\n".join(context))] + messages

        try:
            result = await self._agent.ainvoke({"messages": messages})
            final_message = result["messages"][-1].content

            # Mémoriser pour les prochaines tâches (optionnel)
            if ctx.memory:
                await ctx.memory.record(
                    f"Q: {user_input[:100]} → R: {final_message[:100]}",
                    importance=0.6,
                    task_id=task["task_id"],
                )

            return {
                "task_id": task["task_id"],
                "status": "completed",
                "output": [{"type": "text", "text": final_message}],
            }
        except Exception as exc:
            return {
                "task_id": task["task_id"],
                "status": "failed",
                "error": {"code": "AGENT_ERROR", "message": str(exc)},
            }

    async def on_stop(self):
        self._agent = None

agent = LangGraphAdapter()
```

---

## CrewAI

CrewAI est synchrone — il faut l'exécuter dans un thread séparé pour ne pas bloquer le runtime Tokio.

```python
# crewai_adapter.py
from crewai import Agent, Task, Crew
import asyncio


class CrewAIAdapter:
    def manifest(self):
        return {
            "name": "crewai-research-writer",
            "version": "1.0.0",
            "description": "Crew de recherche et rédaction CrewAI (researcher + writer)",
            "tools_required": [],
            "max_concurrent_tasks": 1,  # CrewAI n'est pas thread-safe
            "step_budget": {
                "wall_clock_timeout_secs": 300
            }
        }

    async def run(self, task, ctx):
        parts = task["input"]["parts"]
        topic = parts[0]["text"] if parts else ""

        researcher = Agent(
            role="Researcher",
            goal=f"Rechercher des informations sur : {topic}",
            backstory="Expert en recherche documentaire",
        )
        writer = Agent(
            role="Writer",
            goal="Rédiger un résumé clair et structuré",
            backstory="Rédacteur technique expérimenté",
        )
        research_task = Task(description=f"Rechercher sur : {topic}", agent=researcher)
        write_task   = Task(description="Rédiger un résumé basé sur la recherche", agent=writer)

        crew = Crew(agents=[researcher, writer], tasks=[research_task, write_task])

        # run_in_executor évite de bloquer le runtime Tokio (event loop Python)
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
            "description": "Conversation multi-agents AutoGen (assistant + user proxy)",
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
            llm_config={"config_list": config_list},
        )
        user_proxy = autogen.UserProxyAgent(
            name="user",
            human_input_mode="NEVER",
            max_consecutive_auto_reply=5,
        )

        result = await asyncio.get_event_loop().run_in_executor(
            None,
            lambda: user_proxy.initiate_chat(
                assistant,
                message=user_input,
                summary_method="last_msg",
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

**Initialisation dans `on_start()`, pas dans `__init__()`**

Les modèles LLM lourds à charger (LangGraph avec modèle local, etc.) doivent être initialisés dans `on_start()`. `__init__()` est appelé à l'import du module — trop tôt, le contexte runtime n'est pas encore disponible.

**`max_concurrent_tasks: 1` pour les frameworks non thread-safe**

CrewAI et AutoGen maintiennent un état global. Si deux tâches s'exécutent en parallèle, elles s'interfèrent. `max_concurrent_tasks: 1` dans le manifest force la séquentialité.

**Toujours catcher les exceptions du framework sous-jacent**

```python
try:
    result = await self._agent.ainvoke(...)
    return {"status": "completed", "output": [...]}
except Exception as exc:
    return {"status": "failed", "error": {"code": "AGENT_ERROR", "message": str(exc)}}
```

Une exception non catchée dans `run()` est convertie en `AIPBridgeError::PythonException` par le bridge Rust — le runtime ne plante pas, mais la tâche est marquée `failed` avec un message peu informatif.

**Frameworks synchrones → `run_in_executor`**

CrewAI, AutoGen, et de nombreux agents custom sont synchrones. Utiliser `asyncio.get_event_loop.run_in_executor(None, lambda:...)` pour les exécuter sans bloquer la boucle d'événements Python partagée avec le bridge Rust.
