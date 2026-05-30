# Adapter LangGraph et CrewAI

Vous avez déjà un agent LangGraph, une Crew CrewAI, ou un workflow AutoGen qui fonctionne. Vous ne voulez pas tout réécrire, vous voulez juste l'exécuter dans Apollia OS pour bénéficier de la persistance, des triggers, du HITL, et de l'observabilité.

Bonne nouvelle : le contrat decorator-first est minimaliste. Une classe décorée par `@agent`, un handler (`@skill` ou `@on_message`), et l'instance est appelable depuis le runtime. Tout framework Python peut être enveloppé en quelques dizaines de lignes.

---

## Le pattern d'adaptation

Quelle que soit la source (LangGraph, CrewAI, AutoGen, code custom), la structure est toujours la même :

```python
from apollia import agent, on_message
from apollia.types import Ctx, Message


@agent(
    name="my-adapted-agent",
    version="0.1.0",
    description="Wrap an existing framework agent.",
    packages=("langgraph>=0.2", "langchain-openai>=0.2"),
)
class AdaptedAgent:
    def __init__(self):
        self._underlying = None

    def _lazy_init(self):
        # Initialisation à la première invocation, pas à l'import.
        if self._underlying is None:
            from langgraph.prebuilt import create_react_agent
            from langchain_openai import ChatOpenAI
            llm = ChatOpenAI(model="gpt-4o-mini")
            self._underlying = create_react_agent(llm, tools=[])

    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        self._lazy_init()
        result = await self._underlying.ainvoke({"messages": [("user", message)]})
        return result["messages"][-1].content
```

Trois principes :

1. **L'instance vit dans `self._underlying`.** Le décorateur `@agent` instancie la classe une fois. Initialisez l'agent sous-jacent en lazy, à la première invocation.
2. **Le handler retourne une string** (`@on_message`) ou un dict (`@skill`). Le boundary fait le reste.
3. **Les imports lourds sont dans la méthode**, pas en haut du fichier. Ça évite de charger LangGraph quand un autre agent du runtime ne l'utilise pas.

---

## LangGraph

```python
from apollia import agent, on_message
from apollia.types import Ctx, Message


@agent(
    name="langgraph-react-agent",
    version="0.1.0",
    description="ReAct agent backed by LangGraph + LangChain tools.",
    packages=("langgraph>=0.2", "langchain-openai>=0.2"),
    memory_namespace="langgraph-agent",
    step_budget={"max_steps": 20, "wall_clock_secs": 120},
)
class LangGraphAgent:
    def __init__(self):
        self._agent = None

    def _lazy_init(self):
        if self._agent is None:
            from langgraph.prebuilt import create_react_agent
            from langchain_openai import ChatOpenAI
            llm = ChatOpenAI(model="gpt-4o-mini")
            self._agent = create_react_agent(llm, tools=[])

    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        self._lazy_init()
        # Enrichir avec la mémoire Apollia (optionnel)
        context_hits = await ctx.memory.search(message, limit=3)
        context_text = "\n".join(h["content"] for h in context_hits)
        messages = []
        if context_text:
            messages.append(("system", f"Contexte:\n{context_text}"))
        messages.append(("user", message))

        result = await self._agent.ainvoke({"messages": messages})
        final = result["messages"][-1].content

        # Persister pour la prochaine tâche
        await ctx.memory.record(f"Q: {message[:120]} → A: {final[:200]}", importance=0.4)
        return final
```

Note : la clé API OpenAI doit être déclarée dans `secrets=("openai_api_key",)` et lue via `ctx.secrets.get("openai_api_key")`. L'exemple ci-dessus suppose que `OPENAI_API_KEY` est dans l'environnement (méthode legacy).

---

## CrewAI

CrewAI est synchrone. Il faut l'exécuter dans un thread séparé pour ne pas bloquer le runtime Tokio.

```python
import asyncio

from apollia import agent, on_message
from apollia.types import Ctx, Message


@agent(
    name="crewai-research-writer",
    version="0.1.0",
    description="Research + writer crew (synchronous, runs in executor).",
    packages=("crewai>=0.50",),
    step_budget={"wall_clock_secs": 300},
)
class CrewAIAgent:
    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        from crewai import Agent, Task, Crew

        researcher = Agent(
            role="Researcher",
            goal=f"Rechercher des informations sur : {message}",
            backstory="Expert en recherche documentaire",
        )
        writer = Agent(
            role="Writer",
            goal="Rédiger un résumé clair et structuré",
            backstory="Rédacteur technique expérimenté",
        )
        research_task = Task(description=f"Rechercher sur : {message}", agent=researcher)
        write_task   = Task(description="Rédiger un résumé basé sur la recherche", agent=writer)

        crew = Crew(agents=[researcher, writer], tasks=[research_task, write_task])

        # run_in_executor pour ne pas bloquer le runtime Tokio
        loop = asyncio.get_event_loop()
        result = await loop.run_in_executor(
            None,
            lambda: crew.kickoff(inputs={"topic": message}),
        )
        return str(result)
```

Bonne pratique : déclarer `agent_type="worker"` et exposer une `@skill` au lieu d'un `@on_message` si le worflow est appelé en A2A par un autre agent (pas un humain).

---

## AutoGen

```python
import asyncio

from apollia import agent, on_message
from apollia.types import Ctx, Message


@agent(
    name="autogen-conversation",
    version="0.1.0",
    description="Multi-agent conversation backed by AutoGen.",
    packages=("autogen-agentchat>=0.2",),
    step_budget={"max_steps": 30, "wall_clock_secs": 180},
)
class AutoGenAgent:
    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        import autogen

        config_list = [{"model": "gpt-4o-mini"}]

        assistant = autogen.AssistantAgent(name="assistant", llm_config={"config_list": config_list})
        user_proxy = autogen.UserProxyAgent(
            name="user",
            human_input_mode="NEVER",
            max_consecutive_auto_reply=5,
        )

        loop = asyncio.get_event_loop()
        result = await loop.run_in_executor(
            None,
            lambda: user_proxy.initiate_chat(
                assistant,
                message=message,
                summary_method="last_msg",
            ),
        )

        return user_proxy.last_message(assistant)["content"]
```

---

## Points d'attention communs

**Initialisation lazy, pas dans `__init__`.** Le décorateur `@agent` appelle `__init__` à l'import du module pour exposer `module.agent`. Les frameworks lourds (modèles locaux, chargement de données) doivent être initialisés à la première invocation, pas à l'import.

**Frameworks synchrones → `run_in_executor`.** CrewAI, AutoGen, et beaucoup d'agents custom sont synchrones. Utilisez `asyncio.get_event_loop().run_in_executor(None, lambda: ...)` pour les exécuter sans bloquer la boucle d'événements Python partagée avec le bridge Rust.

**Catcher les exceptions du framework sous-jacent.** Le boundary trappe et formate, mais pour des erreurs métier exploitables, levez explicitement :

```python
try:
    result = await self._agent.ainvoke(...)
except SomeFrameworkError as exc:
    raise DomainError("UNDERLYING_FRAMEWORK_FAILED", str(exc)) from exc
```

**Step budget conservatif.** Apollia enforce `step_budget` côté Rust. Un agent LangGraph qui boucle peut consommer le budget en quelques secondes. Définissez `max_steps` réaliste dans `@agent(step_budget=...)`.

**Pas de threading non sandboxé.** CrewAI et AutoGen lancent des threads en interne. Tant que vous restez dans le venv isolé de l'agent (déclaré via `packages=(...)`), c'est sans risque. N'ajoutez pas de `subprocess.Popen` à côté qui contournerait la sandbox.

---

## Quand adapter, quand réécrire

| Cas | Choix |
|---|---|
| Workflow LangGraph stable, en prod | Adapter via `@on_message` |
| Crew CrewAI proof-of-concept | Réécrire directement en `apollia.react` + workers `@skill` |
| Agent custom Python, < 200 lignes de logique | Réécrire en decorator-first natif, gain en observabilité |
| Agent custom Python, > 1000 lignes de logique | Adapter, migrer progressivement |

L'adapter est un raccourci. À terme, réécrire en patterns natifs Apollia donne accès à toute l'observabilité (`ctx.events`), à la mémoire structurée (`ctx.memory`), aux outils sandboxés (`ctx.tools`), et au HITL natif (`raise NeedHumanInput(...)`).

---

## ADRs

- `ADR-003` : Duck typing AIP (le contrat minimal qui permet l'adaptation)
- `ADR-098` : Decorator-first

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
