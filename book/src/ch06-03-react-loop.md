# La boucle ReAct

`chat()` est puissant, mais il a une limite : vous devez savoir à l'avance quels outils appeler et dans quel ordre. Si le fichier n'existe pas, vous codez le cas d'erreur. Si le résumé est trop long, vous le tronquez manuellement. Vous orchestrez.

La boucle **ReAct** inverse cette relation. Vous donnez des outils au LLM et lui décrivez l'objectif — c'est lui qui décide quels outils appeler, dans quel ordre, en fonction de ce qu'il observe à chaque étape.

---

## Le concept : Thought → Action → Observe

ReAct (Reasoning + Acting) est un pattern de raisonnement pour les agents LLM :

```
┌─────────────────────────────────────────────────────────┐
│  1. Thought  : "Je dois d'abord lire le fichier"        │
│  2. Action   : appeler file_read("/data/rapport.txt")   │
│  3. Observe  : "Fichier lu — 342 lignes, 3 sections"    │
│                                                         │
│  4. Thought  : "Je peux maintenant générer le résumé"   │
│  5. Action   : (aucun outil — générer la réponse)       │
│  6. Observe  : fin de la boucle → réponse finale        │
└─────────────────────────────────────────────────────────┘
```

À chaque itération, le LLM décide soit d'appeler un outil (Action), soit de conclure (réponse finale). Cette boucle continue jusqu'à ce que le LLM s'arrête ou que `max_iterations` soit atteint.

---

## ctx.llm.run_tools()

`run_tools()` implémente cette boucle automatiquement :

```python
result = await ctx.llm.run_tools(
    messages=[
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user",   "content": user_request},
    ],
    tools=[TOOL_SPEC_1, TOOL_SPEC_2, ...],
    max_iterations=5,
)

print(result.content)          # réponse finale du LLM
print(result.usage.cost_usd)   # coût total (cumul de toutes les itérations)
```

**Ce que fait `run_tools()` en interne :**

```
1. Appelle le LLM avec les outils disponibles
2. Le LLM répond avec finish_reason = "tool_calls"
   → Apollia OS exécute les outils demandés
   → Ajoute les résultats comme messages role: "tool"
   → Revient à l'étape 1
3. Le LLM répond avec finish_reason = "stop"
   → Retourne la réponse finale
4. Si max_iterations atteint → PyRuntimeError
5. Si step_budget épuisé → PyRuntimeError
```

Les erreurs d'outil sont **absorbées** : si un outil échoue, son message d'erreur devient un résultat `role: "tool"` que le LLM peut lire. La boucle continue — jamais d'interruption fatale pour une erreur d'outil individuelle.

---

## Décrire les outils pour le LLM

`tools` est une liste de descripteurs JSON Schema. Le LLM utilise ces descriptions pour décider quand et comment appeler chaque outil.

```python
FILE_READ_SPEC = {
    "name": "file_read",
    "description": "Lit le contenu d'un fichier texte. "
                   "Utilise offset et limit pour les fichiers volumineux.",
    "parameters": {
        "type": "object",
        "properties": {
            "path":   {"type": "string",  "description": "Chemin absolu du fichier"},
            "offset": {"type": "integer", "description": "Ligne de départ (1-based, optionnel)"},
            "limit":  {"type": "integer", "description": "Nombre maximum de lignes (optionnel)"},
        },
        "required": ["path"],
    },
}

FILE_WRITE_SPEC = {
    "name": "file_write",
    "description": "Écrit ou remplace un fichier. Crée les répertoires parents si nécessaire.",
    "parameters": {
        "type": "object",
        "properties": {
            "path":    {"type": "string", "description": "Chemin du fichier à écrire"},
            "content": {"type": "string", "description": "Contenu complet à écrire"},
        },
        "required": ["path", "content"],
    },
}
```

> **La qualité de la description compte.** Un LLM utilise le champ `description` pour décider quand appeler l'outil. Une description vague produit des appels incorrects. Soyez précis sur ce que l'outil fait et ce qu'il retourne.

Astuce : utilisez `ctx.tools.describe()` pour récupérer le schéma directement depuis le Tool Registry — vous n'avez pas à le dupliquer manuellement :

```python
file_read_schema = await ctx.tools.describe("file_read")
# file_read_schema : dict avec name, description, input_schema
```

---

## File-assistant autonome avec ReAct

Voici comment réécrire `file-assistant` en agent ReAct. Au lieu de parser manuellement le chemin du fichier et d'orchestrer chaque appel, on laisse le LLM gérer tout cela :

```python
import re
from datetime import datetime

SYSTEM_PROMPT = """Tu es un assistant spécialisé dans la lecture et la synthèse de fichiers.

Quand l'utilisateur te demande de résumer un fichier :
1. Utilise file_read pour lire le fichier
2. Génère un résumé clair en 5 à 10 phrases
3. Utilise file_write pour sauvegarder le résumé dans un fichier <nom_original>_summary.<ext>
4. Indique à l'utilisateur le chemin du fichier résumé

Si le fichier est volumineux (> 200 lignes), lis-le en plusieurs tranches avec offset et limit.
Si le fichier n'existe pas, dis-le clairement à l'utilisateur."""


class FileAssistantReAct:
    """Agent file-assistant propulsé par la boucle ReAct."""

    def manifest(self):
        return {
            "name": "file-assistant-react",
            "version": "2.0.0",
            "description": "Lit et résume des fichiers — version autonome ReAct",
            "tools_required": ["file_read", "file_write"],
            "max_concurrent_tasks": 1,
            "step_budget": 20,
        }

    async def run(self, task, ctx):
        if ctx.llm is None:
            return AIPResult.failed("LLM_UNAVAILABLE",
                                    "Ce agent nécessite un backend LLM configuré.")

        user_request = task["input"]["parts"][0].get("text", "")
        if not user_request:
            return AIPResult.failed("MISSING_INPUT", "Une requête texte est requise.")

        # Décrire les outils au LLM
        tools = [
            {
                "name": "file_read",
                "description": "Lit le contenu d'un fichier texte. "
                               "Utilise offset et limit pour les grands fichiers.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path":   {"type": "string"},
                        "offset": {"type": "integer"},
                        "limit":  {"type": "integer"},
                    },
                    "required": ["path"],
                },
            },
            {
                "name": "file_write",
                "description": "Écrit ou remplace un fichier texte.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path":    {"type": "string"},
                        "content": {"type": "string"},
                    },
                    "required": ["path", "content"],
                },
            },
        ]

        try:
            result = await ctx.llm.run_tools(
                messages=[
                    {"role": "system", "content": SYSTEM_PROMPT},
                    {"role": "user",   "content": user_request},
                ],
                tools=tools,
                max_iterations=8,
            )
        except Exception as e:
            return AIPResult.failed("REACT_ERROR", str(e))

        return {
            "task_id": task["task_id"],
            "status":  "completed",
            "output":  [{"type": "text", "text": result.content}],
        }


agent = FileAssistantReAct()
```

**Ce que cet agent peut faire que la V1 ne pouvait pas :**

```bash
# V1 : instruction explicite avec chemin exact
apollia-os run file-assistant "Résume /data/rapport.txt"

# V2 ReAct : instructions en langage naturel
apollia-os run file-assistant-react "Résume le rapport du T3 dans /data/"
apollia-os run file-assistant-react "Lis /data/rapport.txt et liste les 5 points clés"
apollia-os run file-assistant-react "Compare /data/q2.txt et /data/q3.txt"
apollia-os run file-assistant-react "Que contient le fichier le plus récent dans /data/ ?"
```

Le LLM peut appeler `file_read` plusieurs fois (pour lire par tranches), `file_write` une fois pour sauvegarder, et adapter son plan selon ce qu'il trouve.

---

## Contrôle et garde-fous

### max_iterations

```python
result = await ctx.llm.run_tools(
    messages=[...],
    tools=[...],
    max_iterations=8,   # si atteint → PyRuntimeError("MaxIterationsReached")
)
```

`max_iterations` limite le nombre d'aller-retours LLM ↔ outils. Pour un agent de résumé de fichier, 5–8 est généralement suffisant. Pour un agent qui peut explorer une arborescence complexe, 15–20 peut être nécessaire.

### step_budget

Chaque itération de `run_tools()` consomme 1 step. Avec `max_iterations=8`, une tâche peut consommer jusqu'à 8 steps pour les appels LLM — plus les appels d'outils individuels. Ajustez `step_budget` dans le manifest en conséquence.

```python
# Manifest pour un agent ReAct avec 8 max_iterations et ~8 appels d'outils
"step_budget": 20,  # 8 LLM + 8 outils + marge
```

### Gérer MaxIterationsReached

Si le LLM n'arrive pas à conclure en `max_iterations` itérations, `run_tools()` lève une `PyRuntimeError`. Pour éviter un `status: "failed"` brutal, gérez cette exception :

```python
try:
    result = await ctx.llm.run_tools(messages=messages, tools=tools, max_iterations=8)
    return AIPResult.completed(result.content)
except Exception as e:
    error_msg = str(e)
    if "MaxIterationsReached" in error_msg:
        # Le LLM a dépassé la limite — retourner ce qui a été produit ou un message partiel
        return AIPResult.failed("INCOMPLETE",
                                "L'agent n'a pas pu terminer dans la limite d'itérations.")
    return AIPResult.failed("REACT_ERROR", error_msg)
```

---

## ReAct vs orchestration manuelle — quand utiliser quoi

| Situation | Pattern recommandé |
|---|---|
| Flux déterministe connu à l'avance | `chat()` + code Python |
| Instructions en langage naturel imprévisibles | `run_tools()` |
| Agents qui doivent explorer avant d'agir | `run_tools()` |
| Coût strict à maîtriser (facturation) | `chat()` — nombre d'appels prévisible |
| Débogage facile requis | `chat()` — flux déterministe |
| Agent autonome cœur de métier | `run_tools()` |

La règle pratique : commencez avec `chat()` et une orchestration manuelle. Passez à `run_tools()` quand vous constatez que les instructions des utilisateurs sont trop variées pour être toutes gérées manuellement, ou quand l'agent doit adapter son plan selon des résultats intermédiaires.

---

## run_tools() et la mémoire

Pour un agent ReAct qui utilise aussi la mémoire, ajoutez `memory_search` à la liste des outils :

```python
tools = [
    {
        "name": "file_read",
        "description": "...",
        "parameters": {...},
    },
    {
        "name": "memory_search",
        "description": "Cherche dans la mémoire persistante de l'agent. "
                       "Utilise quand tu as besoin de contexte sur des tâches passées.",
        "parameters": {
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Requête de recherche"},
                "limit": {"type": "integer", "description": "Max résultats (défaut: 5)"},
            },
            "required": ["query"],
        },
    },
]
```

Le LLM peut maintenant décider seul quand consulter sa mémoire — sans que vous ayez à coder explicitement `if ctx.memory: results = await ctx.memory.search(...)`. C'est la forme la plus autonome d'un agent Apollia OS.
