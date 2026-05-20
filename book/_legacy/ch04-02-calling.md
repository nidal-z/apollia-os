# Appeler un outil depuis Python

La syntaxe d'appel est uniforme pour tous les outils — natifs ou MCP :

```python
result = await ctx.tools.call("nom_outil", {"param": "valeur"})
```

Cette section explique ce qui se passe autour de cet appel : comment les outils sont autorisés, comment les erreurs sont structurées, l'impact sur le step_budget, et comment choisir le bon outil pour une tâche.

---

## Déclaration préalable obligatoire

Vous ne pouvez appeler que les outils déclarés dans `tools_required` ou `tools_optional` de votre manifest. Tenter d'appeler un outil non déclaré retourne immédiatement une erreur — le runtime ne tente même pas d'exécuter l'outil.

```python
# manifest : "tools_required": ["file_read", "file_write"]
# ✔ autorisé
await ctx.tools.call("file_read", {"path": "..."})
# ✗ refusé — bash_executor non déclaré
await ctx.tools.call("bash_executor", {"command": "ls"})
# → {"error": "UNAUTHORIZED: bash_executor not declared in manifest"}
```

Cette contrainte est appliquée par le runtime **avant** d'invoquer l'outil — aucun code sandbox n'est exécuté pour un appel non autorisé.

Un outil peut aussi être **désactivé par l'opérateur** via la gouvernance (`apollia-os tools disable <nom>`). Dans ce cas, l'outil n'est pas enregistré dans le dispatcher et l'appel retourne :

```python
# → {"error": "unknown_tool: bash_executor"}
```

Le code d'erreur est identique à celui d'un outil inexistant — la désactivation est transparente du point de vue du code Python.

---

## Ce qui se passe à chaque appel

Chaque `ctx.tools.call` déclenche cette séquence dans le runtime :

```
1. Vérification d'autorisation (manifest)
2. Décompte du step_budget (si épuisé → PyRuntimeError)
3. Exécution de l'outil dans son sandbox
4. Trace dans l'audit trail (fire-and-forget)
5. Retour du résultat en Python
```

Les étapes 3 et 4 sont parallèles : l'audit trail est écrit de manière asynchrone et n'allonge pas la latence de l'appel.

---

## La structure du résultat

`ctx.tools.call` retourne **toujours** un `dict` Python. En cas de succès, les clés sont spécifiques à chaque outil (voir section précédente). En cas d'erreur, le dict contient un champ `"error"` :

```python
result = await ctx.tools.call("file_read", {"path": "/inexistant.txt"})

if result.get("error"):
    # result["error"] : str — message lisible
    # Exemple : "not_found: /inexistant.txt does not exist"
    print(result["error"])
```

Le message d'erreur contient toujours un **code machine** en préfixe (`not_found:`, `sandbox_violation:`, `timeout:`) suivi d'un message lisible. Pour les décisions programmatiques, extrayez le code :

```python
if result.get("error"):
    code = result["error"].split(":")[0]  # "not_found", "timeout", etc.
    if code == "not_found":
        # fichier absent — comportement spécifique
    elif code == "sandbox_violation":
        # chemin invalide — c'est un bug dans le code de l'agent
```

---

## Pattern défensif complet

Voici le pattern recommandé pour tous les appels d'outils :

```python
async def _read_file_safe(self, ctx, path: str) -> tuple[str | None, str | None]:
    """Lit un fichier. Retourne (contenu, None) ou (None, message_erreur)."""
    result = await ctx.tools.call("file_read", {"path": path})
    if result.get("error"):
        return None, f"Impossible de lire {path} : {result['error']}"
    return result["content"], None

async def run(self, task, ctx):
    content, error = await self._read_file_safe(ctx, "/data/rapport.txt")
    if error:
        return {
            "task_id": task["task_id"],
            "status": "failed",
            "error": {"code": "FILE_NOT_FOUND", "message": error},
        }
    # Continuer avec content...
```

Encapsuler les appels d'outils dans des méthodes d'aide rend `run()` plus lisible et les erreurs plus faciles à tester.

---

## L'impact sur le step_budget

Chaque `ctx.tools.call` consomme **1 step** du budget. Les appels `ctx.llm.chat` et `ctx.llm.complete` consomment également 1 step chacun. `ctx.llm.run_tools` consomme 1 step par itération de la boucle.

```python
# Budget initial : 10 steps
await ctx.tools.call("file_read", {"path": "..."})   # 9 restants
await ctx.tools.call("file_write", {"path": "..."})  # 8 restants
await ctx.llm.chat(system="...", user="...")          # 7 restants
```

Si le budget est épuisé, le runtime lève une `PyRuntimeError` lors du prochain appel — l'exception traverse `run()` et la tâche est marquée `failed`. Pour éviter ce comportement brutal :

```python
async def run(self, task, ctx):
    while True:
        # Vérifier avant chaque appel coûteux
        if ctx.step_budget.steps_remaining < 2:
            return {
                "task_id": task["task_id"],
                "status": "completed",
                "output": [{"type": "text", "text": "Résultat partiel (budget épuisé)"}],
            }
        # ... logique de l'agent
```

Propriétés disponibles sur `ctx.step_budget` (lecture seule) :

```python
ctx.step_budget.steps_remaining       # int — steps restants
ctx.step_budget.tool_calls_remaining  # int — appels d'outils restants
ctx.step_budget.elapsed_seconds       # float — temps écoulé depuis le début
```

---

## Inspecter les outils disponibles

```python
# Lister tous les outils accessibles à cet agent
tools = ctx.tools.list_tools()
# ["file_read", "file_write", "bash_executor", "mcp:notion/search"]

# Vérifier si un outil optionnel est disponible
if "mcp:notion/search" in ctx.tools.list_tools():
    result = await ctx.tools.call("mcp:notion/search", {"query": "..."})

# Obtenir le schéma complet d'un outil (introspection)
schema = await ctx.tools.describe("file_read")
# {"name": ..., "version": ..., "description": ...,
#  "input_schema": {...}, "output_schema": {...}, "tags": [...]}
```

`ctx.tools.describe` est la **source unique de vérité** pour les descripteurs d'outils — il interroge directement le Tool Registry Rust. C'est exactement ce que `BaseReActAgent.react()` utilise sous le capot pour bâtir le bloc *Available tools* du system prompt. Si vous écrivez un agent ReAct from scratch (sans hériter de `BaseReActAgent`), utilisez-le pour construire dynamiquement les descripteurs passés à `ctx.llm.run_tools` — vous n'avez ni à dupliquer le schéma, ni à le maintenir synchro.

---

## Choisir entre les outils pour une même tâche

Plusieurs outils peuvent accomplir la même tâche. Voici les règles de sélection :

| Tâche | Outil recommandé | Pourquoi |
|---|---|---|
| Lire un fichier texte | `file_read` | Atomiqu, sandbox, numéros de ligne |
| Modifier une ligne précise | `file_edit` | Moins risqué que réécrire tout le fichier |
| Réécrire entièrement | `file_write` | Quand `file_edit` est trop limité |
| Trouver des fichiers | `file_glob` | Plus rapide que `bash_executor` + `find` |
| Chercher dans des fichiers | `file_grep` | Plus sûr et parseable que `bash_executor` + `grep` |
| Calcul complexe | `python_executor` | Accès aux bibliothèques Python |
| Opération non couverte | `bash_executor` | En dernier recours — le plus puissant mais le moins prévisible |

**Règle générale :** utilisez l'outil le plus spécialisé disponible. `bash_executor` peut tout faire, mais `file_read` est plus rapide, plus prévisible, et retourne un résultat structuré directement parseable.

---

## Appels en séquence vs en parallèle

Apollia OS exécute les appels d'outils dans l'ordre où vous les faites — `run()` est une coroutine unique, sans parallélisme implicite. Si vous avez plusieurs opérations indépendantes, vous pouvez les paralléliser avec `asyncio.gather` :

```python
import asyncio

async def run(self, task, ctx):
    # Lire deux fichiers en parallèle
    read1, read2 = await asyncio.gather(
        ctx.tools.call("file_read", {"path": "/data/a.txt"}),
        ctx.tools.call("file_read", {"path": "/data/b.txt"}),
    )
    # Les deux appels comptent chacun 1 step
```

Attention : chaque appel parallèle consomme quand même 1 step. Avec `asyncio.gather`, les steps sont consommés simultanément — le step_budget peut s'épuiser plus vite qu'avec des appels séquentiels si vous n'y prenez pas garde.

---

## Patterns d'erreur avancés

La `ResilienceLayer` du runtime applique déjà un retry automatique sur certaines erreurs transitoires (timeout réseau, sandbox temporairement saturé). Mais quand votre logique métier exige un comportement spécifique — réessai sur un code d'erreur particulier, ou bascule vers un outil de secours — vous devez l'écrire explicitement dans `run()`.

### Retry custom avec backoff

```python
import asyncio

async def _read_with_retry(ctx, path: str, attempts: int = 3):
    for i in range(attempts):
        result = await ctx.tools.call("file_read", {"path": path})
        if not result.get("error"):
            return result
        code = result["error"].split(":")[0]
        # Ne réessayer que sur les erreurs transitoires
        if code not in ("timeout", "io_error"):
            return result
        await asyncio.sleep(0.5 * (2 ** i))   # backoff exponentiel
    return result   # dernier résultat, encore en erreur
```

Chaque tentative consomme 1 step — calibrez `attempts` en conséquence.

### Fallback entre 2 outils similaires

Quand un outil principal est indisponible (MCP DEGRADED) ou échoue, basculez sur une alternative :

```python
async def _search_web(ctx, query: str):
    # Préférer Brave si disponible, sinon DuckDuckGo natif
    if "mcp:brave-search/brave_web_search" in ctx.tools.list_tools():
        result = await ctx.tools.call("mcp:brave-search/brave_web_search", {"query": query})
        if not result.get("error"):
            return result
    # Fallback vers l'outil natif
    return await ctx.tools.call("web_search", {"query": query})
```

Ce pattern est typique des agents avec `tools_optional` : la disponibilité change selon la machine cible, et l'agent doit dégrader gracieusement plutôt que de planter.
