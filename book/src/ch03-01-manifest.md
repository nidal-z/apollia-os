# manifest — déclarer ses capacités

`manifest()` est une méthode **synchrone** appelée une seule fois par le runtime au moment du déploiement (`apollia-os agent start`). Elle retourne un dictionnaire Python qui décrit ce que votre agent est et ce dont il a besoin.

---

## Les champs obligatoires

Tout agent, aussi simple soit-il, doit retourner ces quatre champs :

```python
def manifest(self):
    return {
        "name": "file-assistant",    # identifiant unique dans le runtime
        "version": "1.0.0",          # semver
        "description": "...",        # description lisible
        "tools_required": [],        # liste des outils requis (peut être vide)
    }
```

Si l'un de ces champs est absent ou malformé, le runtime refuse de démarrer l'agent avec un message d'erreur explicite :

```bash
$ apollia-os agent start ./mon_agent.py
  ✗ Validation AIP échouée : champ 'name' manquant dans manifest()
```

---

## Les outils : required vs optional

### tools_required — bloquant

```python
"tools_required": ["file_read", "file_write"],
```

Le runtime résout (vérifie l'existence dans le Tool Registry) chaque outil de cette liste au démarrage. Si un outil est absent, l'agent passe en `STOPPED` — il ne peut pas s'exécuter.

C'est le comportement **fail-fast** : mieux vaut une erreur claire au démarrage qu'un échec silencieux à la tâche numéro 47.

### tools_optional — dégradé

```python
"tools_optional": ["mcp:notion/search"],
```

Si un outil optionnel est absent, le démarrage continue mais l'agent passe en `DEGRADED`. Il peut fonctionner, mais ses capacités sont réduites. À vous de gérer ce cas dans `run()` :

```python
async def run(self, task, ctx):
    # Vérifier si l'outil optionnel est disponible avant de l'appeler
    if "mcp:notion/search" in ctx.tools.list_tools():
        results = await ctx.tools.call("mcp:notion/search", {"query": "..."})
    else:
        # Mode dégradé : fonctionner sans Notion
        results = []
```

---

## Concurrence et budget

### max_concurrent_tasks

```python
"max_concurrent_tasks": 1,   # défaut : 1
```

Nombre de tâches que l'agent peut traiter simultanément. Le runtime crée un sémaphore de cette capacité. Les tâches soumises au-delà sont mises en file d'attente.

Pour `file-assistant` : 1 est correct. Deux résumés LLM en parallèle pour le même fichier pourraient écraser le fichier `_summary.txt`.

Pour un agent qui fait des appels réseau indépendants : 4 ou 8 est raisonnable une fois que vous avez mesuré le besoin.

### step_budget — forme complète

Au chapitre 2, vous avez utilisé `"step_budget": 10` — une syntaxe raccourcie qui fixe uniquement `max_steps`. La forme complète expose les trois dimensions du budget :

```python
"step_budget": {
    "max_steps": 40,            # étapes (appels outils + appels LLM) — défaut runtime : 30
    "max_tool_calls": 80,       # appels d'outils uniquement — défaut runtime : 60
    "wall_clock_secs": 900,     # temps réel en secondes — défaut runtime : 600
},
```

Si `step_budget` est `None` ou absent, le runtime applique ses propres défauts. Ces défauts sont configurables dans `apollia.toml` — voir le chapitre 16.

> **Le runtime plafonne toujours** — si votre manifest demande `max_steps: 1000` et que le runtime est configuré avec un plafond de 100, la valeur effective est 100. Ce plafonnement est non-contournable (principe #7 des garde-fous).

---

## Mémoire persistante

```python
"memory_namespace": "file-assistant-memory",
```

Si ce champ est présent, le runtime ouvre un namespace SQLite dédié et l'expose via `ctx.memory`. Sans ce champ, `ctx.memory` est `None`.

Ajoutons la mémoire à `file-assistant` pour qu'il se souvienne des résumés déjà générés :

```python
def manifest(self):
    return {
        "name": "file-assistant",
        "version": "1.1.0",
        "description": "Lit un fichier, le résume via LLM, sauvegarde le résumé",
        "tools_required": ["file_read", "file_write"],
        "memory_namespace": "file-assistant-memory",  # ← nouveau
        "max_concurrent_tasks": 1,
        "step_budget": 10,
    }
```

Puis dans `run()`, avant d'appeler le LLM :

```python
# Vérifier si ce fichier a déjà été résumé récemment
if ctx.memory:
    past = await ctx.memory.search(f"résumé {file_path}", limit=1)
    if past and past[0]["score"] > 0.8:
        # Résumé récent trouvé — le retourner directement
        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": f"(Résumé en cache)\n\n{past[0]['content']}"}],
        }
```

Le chapitre 5 couvre la mémoire en profondeur : les trois types (épisodique, sémantique, procédurale), la recherche FTS5, et les namespaces partagés.

### shared_memory_namespaces — accès partagé en lecture

```python
"shared_memory_namespaces": ["crm-agent-memory", "catalog-agent-memory"],
```

Permet à cet agent de lire (jamais d'écrire) dans les namespaces d'autres agents. Utile dans un pipeline multi-agents où un orchestrateur accède aux mémoires des workers.

---

## Réseau

```python
"network_allowlist": ["api.openai.com", "*.anthropic.com"],
```

Liste des domaines autorisés pour `http_fetch`. Sans ce champ, toute tentative d'appel réseau via `http_fetch` est rejetée avec `DOMAIN_NOT_ALLOWED`.

```python
# Autoriser tous les domaines (avec avertissement au démarrage)
"network_allowlist": ["*"],
```

L'utilisation de `"*"` génère un `WARN` dans les logs du runtime et une entrée explicite dans l'audit trail pour chaque appel réseau.

---

## Approbation humaine

```python
"tools_requiring_approval": ["smtp", "file_write"],
```

En **mode orchestré** (chapitre 9), liste les outils qui nécessitent une confirmation humaine avant d'être exécutés. Quand ORIA planifie une action avec un de ces outils, il suspend la tâche et attend une décision.

Ce champ n'a aucun effet en mode direct (l'agent Python gère lui-même la logique d'approbation).

---

## Exposition A2A

```python
"supports_a2a": True,
"skills": [
    {
        "id": "summarize-file",
        "name": "Résumé de fichier",
        "description": "Lit un fichier et retourne un résumé structuré",
        "input_modes": ["text"],
        "output_modes": ["text"],
    }
],
```

Quand `supports_a2a: True`, le runtime génère automatiquement une **AgentCard** et expose l'agent via le protocole A2A. D'autres agents peuvent alors le découvrir et lui déléguer des tâches. Le chapitre 11 couvre A2A en profondeur.

---

## Backend LLM spécifique

```python
"llm_backend": "anthropic",   # None = backend par défaut du runtime
```

Si plusieurs backends sont configurés (par exemple, un backend local rapide pour les tâches simples et un backend cloud puissant pour les résumés), vous pouvez forcer l'utilisation d'un backend spécifique pour cet agent. Ce champ est ignoré si le backend nommé n'existe pas — l'agent utilise le défaut.

---

## Tableau récapitulatif

### Champs obligatoires

| Champ | Type | Effet si absent |
|---|---|---|
| `name` | `str` | Erreur au démarrage |
| `version` | `str` (semver) | Erreur au démarrage |
| `description` | `str` | Erreur au démarrage |
| `tools_required` | `list[str]` (peut être `[]`) | Erreur au démarrage |

### Champs optionnels

| Champ | Défaut | Effet si absent |
|---|---|---|
| `tools_optional` | `[]` | Ignoré |
| `memory_namespace` | `None` | `ctx.memory` est `None` |
| `shared_memory_namespaces` | `[]` | Aucun accès partagé |
| `max_concurrent_tasks` | `1` | 1 tâche à la fois |
| `step_budget` | `None` | Défauts runtime appliqués |
| `network_allowlist` | `None` | Aucun accès réseau |
| `tools_requiring_approval` | `[]` | Aucune approbation |
| `supports_a2a` | `False` | Pas d'AgentCard |
| `skills` | `[]` | Requis si `supports_a2a: True` |
| `llm_backend` | `None` | Backend par défaut |

---

## La validation duck typing

Avant d'activer l'agent, le runtime effectue ces vérifications dans l'ordre :

1. La variable `agent` existe au niveau module
2. `agent` a une méthode `manifest()`
3. `manifest()` retourne un dict JSON-sérialisable avec les 4 champs obligatoires
4. `agent` a une méthode `run()`
5. `run()` est une coroutine async (`asyncio.iscoroutinefunction`)
6. Résolution des outils listés dans `tools_required`

Si une étape échoue, l'agent s'arrête en `STOPPED` avec un message précis pointant la cause.
