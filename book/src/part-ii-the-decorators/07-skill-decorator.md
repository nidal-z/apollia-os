# Le décorateur `@skill`

Un **skill** est une capacité atomique qu'un agent expose à l'extérieur. Il est appelable par la CLI, l'API REST, l'app Desktop, ou un autre agent via A2A. `@skill` marque une méthode async d'une classe `@agent` pour la déclarer comme telle.

La signature de la méthode **est** le schéma I/O. Pas de manifeste à écrire à la main : le SDK introspecte les annotations et produit le JSON Schema (cf. [Partie IV](../part-iv-llm-friendly-design/19-annotated-descriptions.md)).

---

## Exemple minimal

```python
from apollia import agent, skill
from apollia.types import Ctx

@agent(name="echo", version="0.1.0", description="Echo agent.")
class Echo:
    @skill("echo.say", description="Echo a value back to the caller.")
    async def say(self, value: str, ctx: Ctx) -> dict:
        return {"echoed": value}
```

Trois exigences :

- la méthode est `async def` ;
- elle prend `ctx: Ctx` en argument ;
- elle retourne un `dict` JSON-sérialisable, ou lève une exception typée (cf. [Partie V](../part-v-error-handling/22-domain-errors.md)).

---

## Signature

```python
def skill(
    skill_id: str,
    *,
    description: str = "",
    requires_approval: bool = False,
    dangerous: bool = False,
    examples: list[dict] | None = None,
) -> Callable[[F], F]:
```

### `skill_id`

Identifiant unique de la skill, en `dot.snake_case` minuscule. La regex appliquée est `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`. Quelques exemples valides :

- `"greet"`
- `"pdf.read_text"`
- `"web.search_and_extract"`
- `"file.list_directory"`

Et quelques invalides qui lèvent `AgentConfigError` au load :

- `"Pdf.ReadText"` (majuscules)
- `"pdf-read"` (tirets)
- `"pdf..read"` (segment vide)
- `""` (chaîne vide)

Le `dot.namespace` permet de regrouper plusieurs skills d'un même domaine. Convention recommandée : préfixer par un nom court qui rappelle l'agent ou la famille (`pdf.*`, `web.*`, `chart.*`).

### `description`

Phrase courte affichée à l'invocant (autre agent, LLM, opérateur humain). Si elle est absente, le manifeste retombe sur la première ligne du docstring de la méthode. Toujours fournir l'une des deux : c'est ce que voit le LLM quand il décide d'appeler la skill.

### `examples`

Liste optionnelle de **payloads d'exemple**. Chaque entrée est un dict JSON-sérialisable représentant un appel réaliste. Le tool descriptor LLM les exposera tel quel (cf. [chapitre 20](../part-iv-llm-friendly-design/20-examples-payloads.md)).

```python
@skill(
    "pdf.read_text",
    description="Extract text from a PDF, optionally limited to a page range.",
    examples=[
        {"path": "/tmp/report.pdf"},
        {"path": "/tmp/report.pdf", "page_range": "1-10"},
    ],
)
async def read_text(
    self,
    path: str,
    page_range: str | None = None,
    ctx: Ctx,
) -> dict: ...
```

Le SDK **ne valide pas** les exemples contre le schéma inféré : cela créerait une dépendance circulaire au moment de la construction du manifeste. L'auteur est responsable de la cohérence.

### `requires_approval` et `dangerous`

- `requires_approval=True` : déclenche une pause HITL avant l'invocation. Le runtime suspend, demande à l'opérateur de valider, puis reprend (cf. [chapitre 23](../part-v-error-handling/23-need-human-input.md)).
- `dangerous=True` : marque la skill comme potentiellement destructive ; affiche un bandeau d'avertissement dans l'UI Desktop et la CLI interactive.

Les deux flags sont indépendants. Un `pdf.delete_pages` typique est à la fois `dangerous=True` et `requires_approval=True`.

---

## Plusieurs skills sur le même agent

Une classe `@agent` peut exposer autant de skills que de méthodes décorées. C'est le pattern standard d'un **worker** (cf. [chapitre 3](../part-i-getting-started/03-quickstart-worker.md)).

```python
@agent(name="pdf-worker", version="1.0.0", description="PDF utilities.", agent_type="worker")
class PdfWorker:
    @skill("pdf.read_text", description="Extract text from a PDF.")
    async def read_text(self, path: str, ctx: Ctx) -> dict: ...

    @skill("pdf.count_pages", description="Count the pages of a PDF.")
    async def count_pages(self, path: str, ctx: Ctx) -> dict: ...

    @skill("pdf.extract_metadata", description="Read PDF metadata (author, title).")
    async def metadata(self, path: str, ctx: Ctx) -> dict: ...
```

`apollia inspect` listera les trois skills avec leur schéma inféré et leurs exemples.

---

## Contraintes validées au load

`@skill` lève `AgentConfigError` à l'import si :

- `skill_id` n'est pas une chaîne non vide ;
- `skill_id` ne respecte pas la regex `dot.snake_case` ;
- `description` n'est pas une chaîne ;
- `examples` n'est pas une `list[dict]` ;
- la méthode décorée n'est pas `async def` ;
- `@skill` est appliqué deux fois sur la même méthode.

Le `@agent` parent enforce en plus :

- pas de doublon de `skill_id` à travers les méthodes (deux `@skill("pdf.read_text")` ⇒ erreur) ;
- mutuelle exclusion avec `@orchestrated` (cf. [chapitre 9](09-orchestrated-decorator.md)).

---

## Anti-patterns

**Ne pas** rendre la méthode non-async :

```python
# CASSE au load
@skill("foo.bar")
def bar(self, x: int, ctx): ...   # ⇒ AgentConfigError "must be 'async def'"
```

**Ne pas** retourner un `AIPResult` à la main :

```python
# OK : retournez juste un dict
async def bar(self, x: int, ctx) -> dict:
    return {"value": x * 2}

# NON : AIPResult est interne au SDK
async def bar(self, x: int, ctx):
    return AIPResult.completed({"value": x * 2}).to_dict()  # ⇒ erreur runtime
```

Le boundary du dispatcher emballe automatiquement votre `dict` en `AIPResult.completed` et trappe vos exceptions en `AIPResult.failed` (cf. [chapitre 22](../part-v-error-handling/22-domain-errors.md)). Vous n'avez jamais à manipuler `AIPResult` directement.

**Ne pas** réutiliser le même `skill_id` pour plusieurs méthodes : `AgentConfigError` au load.

---

## ADRs

- `ADR-098` : Decorator-first
- `ADR-099` : Signature inference comme schéma I/O
- `ADR-109` : `AIPResult` interne au SDK

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
