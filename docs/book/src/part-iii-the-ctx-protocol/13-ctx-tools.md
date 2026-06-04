# `ctx.tools`

`ctx.tools` (Protocol `ToolProxy`) est la surface d'appel des outils natifs Apollia (`file_read`, `bash_executor`, `file_grep`, `web_read`, …) et des outils MCP exposés par des serveurs externes connectés.

Du point de vue de l'agent, il n'y a pas de différence : tout outil s'appelle par `ctx.tools.call(name, input)`. Le routage (natif vs. MCP) est transparent.

---

## Appeler un outil : `call`

```python
result = await ctx.tools.call(
    "file_read",
    input={"path": "/tmp/report.txt", "max_bytes": 50_000},
)
content = result["content"]
```

Signature :

```python
async def call(
    self,
    tool_name: str,
    input: dict[str, Any],
) -> dict[str, Any]: ...
```

L'argument `input` est un dict (pas des keyword args éparpillés). C'est le payload validé contre l'`input_schema` de l'outil côté runtime. Le retour est un dict conforme à l'`output_schema`.

---

## Lister et décrire

```python
names = ctx.tools.list_tools()
# names : list[str], par exemple ["file_read", "file_write", "bash_executor", ...]

descriptor = await ctx.tools.describe("file_read")
# descriptor : dict | None, métadonnées : description, input_schema, output_schema, tags
```

`list_tools` ne renvoie que les outils résolus (présents au catalogue, et non bloqués par les permissions de la session). C'est utile pour un agent director qui veut construire dynamiquement la liste d'outils à passer à `apollia.react`.

---

## Outils natifs et outils MCP

Le nom de l'outil porte le routage :

- **Outil natif Apollia** : `"file_read"`, `"bash_executor"`, `"file_grep"`, etc. Résolu par le ToolRegistry Rust.
- **Outil MCP** : préfixé par `mcp:<server>/<name>`. Exemple : `"mcp:github/list_issues"`. Le runtime route vers le serveur MCP connecté correspondant.

Du code de l'agent, ça donne :

```python
issues = await ctx.tools.call("mcp:github/list_issues", input={"repo": "owner/name"})
```

La liste des outils MCP disponibles dépend des serveurs configurés au niveau de la session. L'opérateur les active dans l'app Desktop ou via `apollia-os mcp enable <server>`.

> **Référence technique :** la page wiki `Briques-Tool-Registry` détaillera le catalogue complet des outils natifs, leurs `input_schema`, et la procédure d'ajout d'un outil natif. La page `Briques-MCP-Client` couvrira le client MCP *(wiki disponible prochainement)*. En attendant, `apollia-os tools list` affiche le catalogue installé.

---

## Permissions et HITL

Tous les outils ne sont pas appelables en silence. Trois niveaux de gating au runtime :

1. **Disponibilité** : l'outil est-il dans le catalogue ? Si non, `call` lève une erreur `UnknownTool`.
2. **Permission de session** : le moteur de permissions à 3 couches (session / project / global) peut bloquer un outil. L'erreur retournée est `tool not allowed for this session`.
3. **HITL** : un outil marqué `requires_approval` déclenche une pause pour validation humaine. Le runtime suspend la tâche et la reprend après accord.

Du point de vue de l'agent, ces gates sont **transparents** : vous appelez, et soit ça passe, soit ça lève. Pas besoin d'interroger les permissions à l'avance.

Bonne pratique : déclarez les outils que votre agent utilise dans `@agent(tools_required=("file_read", "bash_executor"))`. Cela force la validation au boot (fail-fast si un outil manque) et rend votre manifeste lisible (cf. [chapitre 6](../part-ii-the-decorators/06-agent-decorator.md)).

---

## Compteur d'appels

```python
count = ctx.tools.tool_call_count()
# int : nombre d'appels d'outils émis par l'agent dans cette tâche
```

Utile pour des décisions adaptatives (`si déjà 20 appels, arrêter de fouiller`), même si le runtime applique son propre StepBudget en parallèle (cf. [chapitre 17](17-ctx-events-logger-budget.md)).

---

## Exemple complet : lecture + filtrage

```python
@skill("audit.list_large_files", description="List files larger than a threshold.")
async def list_large_files(self, root: str, min_kb: int, ctx: Ctx) -> dict:
    out = await ctx.tools.call(
        "bash_executor",
        input={"cmd": f"find {root} -type f -size +{min_kb}k"},
    )
    paths = out["stdout"].splitlines()
    descriptors = []
    for p in paths[:50]:
        meta = await ctx.tools.call("file_read", input={"path": p, "max_bytes": 0})
        descriptors.append({"path": p, "bytes": meta["size_bytes"]})
    return {"files": descriptors, "truncated": len(paths) > 50}
```

L'agent déclare bien `tools_required=("bash_executor", "file_read")` dans `@agent` pour bénéficier du fail-fast au boot.

---

## Anti-patterns

**Ne pas** appeler `subprocess.run`, `open()` ou `requests` en direct depuis un agent. Ces appels contournent la sandbox, le budget, et l'audit trail. Utilisez `ctx.tools.call("bash_executor", ...)`, `ctx.tools.call("file_read", ...)`, `ctx.tools.call("web_read", ...)`.

**Ne pas** boucler en `for x in big_list: await ctx.tools.call(...)` sans surveiller `ctx.budget`. Chaque appel consomme du step budget. Préférez un outil qui traite un batch (`file_grep` plutôt qu'une boucle de `file_read`).

**Ne pas** capturer une exception sur `ctx.tools.call` et la transformer en réponse "vide". L'erreur est de l'information : laissez-la remonter, ou re-levez une `DomainError` typée (cf. [chapitre 22](../part-v-error-handling/22-domain-errors.md)).

**Ne pas** parser le `name` d'un outil pour décider du routage. Le préfixe `mcp:` est une convention, mais le routage est interne au runtime. Ne réinventez pas la dispatch.

---

## ADRs

- `ADR-002` : ToolExecutor trait abstraction
- `ADR-006` : Décomposition atomique des outils
- `ADR-017` : Client MCP natif
- `ADR-015` : Permission engine 3 layers
- `ADR-015` : Tool governance unifiée
- `ADR-006` : Tool execution paths convergence

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
