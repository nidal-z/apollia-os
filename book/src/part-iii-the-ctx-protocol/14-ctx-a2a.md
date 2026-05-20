# `ctx.a2a` et `apollia.react`

`ctx.a2a` (Protocol `A2AInterface`) est l'unique surface pour appeler d'autres agents. Quatre méthodes : `invoke`, `discover`, `list_skills`, `skill_as_tool`.

`apollia.react` est une **fonction libre** (pas une méthode de `ctx`) qui pilote une boucle ReAct : LLM → tool calls → LLM → … → réponse. C'est l'outil principal d'un agent director.

---

## `ctx.a2a.invoke` : appeler une skill d'un autre agent

```python
result = await ctx.a2a.invoke(
    "pdf.read_text",
    input={"path": "/tmp/report.pdf", "page_range": "1-3"},
    timeout_secs=60,
)
text = result["text"]
```

Signature :

```python
async def invoke(
    self,
    skill_id: str,
    input: dict[str, Any] | None = None,
    *,
    timeout_secs: int = 120,
    **kwargs: Any,
) -> dict[str, Any]: ...
```

Le `skill_id` est l'identifiant `dot.snake_case` déclaré par le worker cible (cf. [chapitre 7](../part-ii-the-decorators/07-skill-decorator.md)). Le dict de retour est le payload retourné par la skill, exactement comme si vous l'aviez appelée localement.

Si la skill cible lève une `DomainError("CODE", "message")`, `invoke` propage l'erreur. C'est cohérent : votre agent traite les erreurs A2A comme les erreurs locales.

---

## `discover` : lire une SkillCard

```python
card = await ctx.a2a.discover("pdf.read_text")
# card : dict | None, keys : skill_id, name, description, agent_name,
#        input_schema, output_schema
```

Utile pour décider dynamiquement si une skill existe avant de l'appeler, ou pour afficher sa description dans une UI d'agent director.

---

## `list_skills` : inventaire

```python
skills = await ctx.a2a.list_skills()
# skills : list[dict] de SkillCards, agrégeant toutes les skills installées
```

Retourne toutes les skills exposées par les agents installés et activés sur la machine. Utile pour scaffolder dynamiquement la liste d'outils d'un director.

---

## `skill_as_tool` : convertir en tool descriptor LLM

```python
descriptor = ctx.a2a.skill_as_tool("pdf.read_text")
# descriptor : dict au format Anthropic / OpenAI tool-use
#   {"name": "pdf.read_text", "description": "...",
#    "input_schema": {...}, "examples": [...]}
```

C'est la méthode **synchrone** qui transforme une skill A2A en descripteur compatible avec les `tools=[...]` que prennent les LLM. Cas d'usage principal : nourrir `apollia.react` (voir plus bas).

---

## `apollia.react` : boucle ReAct prête à l'emploi

`apollia.react` est importée directement, pas via `ctx` :

```python
from apollia import react
```

Elle pilote une boucle Reason+Act : appel LLM avec system+user et une liste de tools, exécution des tool calls demandés par le LLM, ré-appel LLM avec les résultats, jusqu'à une réponse finale ou épuisement du `max_steps`.

```python
from apollia import agent, on_message, react
from apollia.types import Ctx, Message

@agent(name="research-director", version="0.1.0", description="Investigate questions.")
class ResearchDirector:
    SYSTEM_PROMPT = "You orchestrate research workers to answer questions thoroughly."

    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        return await react(
            ctx,
            system=self.SYSTEM_PROMPT,
            user=message,
            tools=[
                ctx.a2a.skill_as_tool("web.search_and_extract"),
                ctx.a2a.skill_as_tool("web.summarize"),
                ctx.a2a.skill_as_tool("research.synthesize"),
            ],
            max_steps=15,
        )
```

Signature :

```python
async def react(
    ctx: Ctx,
    system: str,
    user: str,
    *,
    tools: list[dict[str, Any]] | None = None,
    max_steps: int = 15,
    temperature: float = 0.3,
    stream: bool = True,
) -> str: ...
```

- `tools` est une liste de descripteurs au format LLM tool-use. Vous l'assemblez avec `ctx.a2a.skill_as_tool(skill_id)` pour les workers A2A, ou avec `await ctx.tools.describe(name)` pour les outils natifs.
- `max_steps` est le plafond de tours LLM. Si dépassé, le runtime LLM lève une erreur.
- `temperature` est informationnelle aujourd'hui (`run_tools` utilise le default du backend).
- `stream=True` émet un `emit_thought` à l'entrée de la boucle pour les dashboards d'observabilité.

Le retour est la **string finale** produite par le LLM. C'est ce que vous renvoyez à l'utilisateur dans un `@on_message`.

---

## `apollia.react` vs `@orchestrated`

Deux voies pour faire raisonner un agent :

- **`apollia.react`** : vous gardez le contrôle. Le code Python décide quand entrer dans la boucle, quels outils exposer, quoi faire de la sortie. Vous pouvez chaîner plusieurs `react`, brancher selon la réponse, faire de la pré et post-procédure.
- **`@orchestrated`** : vous décrivez l'intention en `system_prompt` et laissez le moteur ORIA (côté Rust) piloter la boucle. Moins de code, moins de contrôle.

Choix typique :

- Director qui exécute un workflow connu en quelques étapes ⇒ `apollia.react`.
- Agent qui doit décider en autonomie complète quelle séquence d'outils utiliser ⇒ `@orchestrated`.

---

## Erreurs `apollia.react`

`react` lève une `DomainError("REACT_MAX_STEPS", ...)` si `max_steps <= 0` à l'appel. La boucle interne du LLM peut aussi lever une erreur si elle épuise `max_steps` sans converger ; cette erreur remonte telle quelle.

L'agent peut catcher localement pour tomber dans un fallback :

```python
try:
    answer = await react(ctx, system="...", user=msg, tools=[...], max_steps=5)
except DomainError as exc:
    if exc.code == "REACT_MAX_STEPS":
        return "Désolé, je n'ai pas pu finir cette analyse dans les temps."
    raise
```

---

## Anti-patterns

**Ne pas** appeler `await ctx.a2a.skill_as_tool(...)`. La méthode est synchrone. Le `await` lèvera `TypeError`.

**Ne pas** boucler manuellement sur `ctx.llm.complete` avec des prompts qui simulent du tool calling. Utilisez `apollia.react` qui parse les tool calls correctement et gère le step budget.

**Ne pas** mélanger `@orchestrated` et `apollia.react` dans la même classe. Si la classe est décorée `@orchestrated`, le runtime ne lui injecte pas le dispatcher d'invocation A2A en direct. Choisissez l'un ou l'autre.

**Ne pas** lister explicitement les skills dans un `list_skills` côté director si vous savez quelles skills vous utilisez. Codez les `skill_as_tool` en dur : plus rapide, plus lisible, et le fail-fast au boot signalera une skill manquante.

---

## ADRs

- `ADR-049` : A2A routing inter-agents
- `ADR-101` : Ctx exhaustif
- `ADR-102` : SDK A2A API unifiée
- `ADR-108` : SDK mailbox A2A suppression (alignement sur `ctx.a2a`)

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
