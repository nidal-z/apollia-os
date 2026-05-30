# `ctx.datasources` et `ctx.templates`

Deux services pour découpler le **contenu modifiable** (YAML, templates) du **code** de l'agent. L'opérateur ou un autre agent peut ajuster les datasources et les templates sans toucher au Python.

Tous les deux suivent le même principe : déclarés dans le manifeste, résolus au boot, accédés via `ctx.<service>.get(...)` ou `.render(...)`.

---

## `ctx.datasources` : YAML déclarés

Un fichier YAML par datasource, placé dans `datasources/` du package de l'agent. Le runtime parse au boot et garde la valeur en mémoire.

**Structure projet :**

```
agents/veille-ia/
├── agent.py
├── datasources/
│   ├── topics.yaml
│   ├── sources.yaml
│   └── competitors.yaml
└── ...
```

**Déclaration dans `@agent` :**

```python
from apollia import agent, skill
from apollia.types import Ctx

@agent(
    name="veille-ia",
    version="0.1.0",
    description="Competitive intelligence cycles.",
    datasources=("topics", "sources", "competitors"),
)
class VeilleIA:
    @skill("veille.run_cycle", description="Run one intelligence cycle.")
    async def run_cycle(self, ctx: Ctx) -> dict:
        topics = ctx.datasources.get("topics")
        sources = ctx.datasources.get("sources")
        return {"covered": len(topics["entries"]), "feeds": len(sources["rss"])}
```

L'API du Protocol :

```python
def get(self, name: str) -> Any: ...
def list_names(self) -> list[str]: ...
```

`get` retourne le YAML parsé : un `dict`, une `list`, un scalaire, selon la racine du document.

### Gating strict

Si vous appelez `ctx.datasources.get("foo")` sans avoir déclaré `"foo"` dans `@agent(datasources=(...))`, l'appel lève `FileNotFoundError`. Le manifeste est la liste exhaustive de ce que l'agent peut lire.

C'est volontaire. À tout instant, `apollia inspect` (cf. [chapitre 27](../part-vii-tooling/27-apollia-inspect.md)) peut afficher l'inventaire complet : « cet agent lit `topics.yaml`, `sources.yaml`, `competitors.yaml` et rien d'autre ».

### Le workspace override

Si l'opérateur a placé un `APOLLIA.md` avec une section `# topics` au niveau du workspace, `ctx.workspace.get("topics")` retourne cette section. La convention est que le code prend la valeur du workspace en priorité, puis retombe sur le datasource local :

```python
topics = ctx.workspace.get("topics") or ctx.datasources.get("topics")
```

Cf. [chapitre 18](18-ctx-other-services.md) pour `ctx.workspace`.

---

## `ctx.templates` : Jinja2 servi par le runtime

Les templates Jinja2 vivent dans `templates/` du package. Ils sont rendus côté runtime (moteur `minijinja` en Rust), pas en Python.

**Structure projet :**

```
agents/veille-ia/
├── agent.py
├── templates/
│   ├── weekly-digest.j2
│   └── alert.j2
└── ...
```

**Déclaration :**

```python
@agent(
    name="veille-ia",
    version="0.1.0",
    description="…",
    templates=("weekly-digest", "alert"),
)
class VeilleIA:
    @skill("veille.format_digest", description="Render the weekly digest.")
    async def format_digest(self, items: list, week: int, ctx: Ctx) -> dict:
        body = ctx.templates.render(
            "weekly-digest",
            items=items,
            week=week,
            generated_at=datetime.now().isoformat(),
        )
        return {"markdown": body}
```

API :

```python
def render(self, name: str, **context: Any) -> str: ...
def list_names(self) -> list[str]: ...
```

Le nom du template **n'inclut pas l'extension** (`weekly-digest`, pas `weekly-digest.j2`).

### Sandbox du moteur

Le runtime utilise `minijinja` qui n'expose pas `{% include %}`, `{% import %}`, ni les filtres dangereux par défaut. Utilisez les templates comme de simples *formatters* :

- Boucles `{% for item in items %}`
- Conditions `{% if condition %}`
- Filtres natifs courants (`{{ value | upper }}`, `{{ value | length }}`)

Les besoins plus avancés (sous-templates) se font en composition au niveau Python (rendre deux templates et concaténer).

### Gating identique

Comme pour les datasources, un `render("foo")` sans `"foo"` dans `templates=(...)` lève une erreur. Le manifeste reste la définition exhaustive.

---

## Pattern composite : datasource + template

Cas classique de la veille : assembler un digest à partir d'une source YAML et d'un template :

```python
@skill("veille.format_alert", description="Format a single alert.")
async def format_alert(self, entry: dict, ctx: Ctx) -> dict:
    competitors = ctx.datasources.get("competitors")
    matched = next((c for c in competitors["names"] if c in entry["title"]), None)
    body = ctx.templates.render(
        "alert",
        entry=entry,
        competitor=matched,
        urgency="high" if matched else "normal",
    )
    return {"markdown": body, "urgency": "high" if matched else "normal"}
```

Le code reste très court. Toute la logique de formatting et de configuration vit hors du Python.

---

## Anti-patterns

**Ne pas** ouvrir les fichiers à la main (`open("datasources/topics.yaml")`). Passez par `ctx.datasources.get(...)` qui résout depuis le bon répertoire selon l'installation (dev local vs. agent installé dans `~/.apollia/agents/`).

**Ne pas** sérialiser des objets Python dans les datasources (`yaml.dump(custom_obj)`). Restez en types primitifs YAML (dict, list, str, int, float, bool). C'est ce que l'opérateur doit pouvoir lire et modifier sans devoir interpréter du Python.

**Ne pas** rendre des templates avec du code Python à l'intérieur. La sandbox `minijinja` ne l'exécutera pas et lèvera une erreur. Les templates sont des *formatters*, pas du scripting.

**Ne pas** muter les datasources depuis l'agent. La surface est en lecture seule par contrat. Pour persister du state, c'est `ctx.memory` (cf. [chapitre 12](12-ctx-memory.md)).

---

## ADRs

- `ADR-056` : Workspace context assembly (APOLLIA.md)
- `ADR-103` : SDK datasources et templates runtime

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
