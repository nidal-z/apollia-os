# Le manifest

La conception est faite. Traduisons-la en manifest.

---

## Le manifest de file-assistant

```python
def manifest(self):
    return {
        # --- Identité ---
        "name": "file-assistant",
        "version": "1.0.0",
        "description": "Lit un fichier, le résume via LLM, sauvegarde le résumé",

        # --- Outils requis ---
        # Le runtime vérifie leur existence au démarrage.
        # Si l'un d'eux est absent, l'agent refuse de démarrer (STOPPED).
        "tools_required": ["file_read", "file_write"],

        # --- Capacité ---
        # 1 tâche à la fois — les appels LLM ont une latence variable.
        "max_concurrent_tasks": 1,

        # --- Garde-fous ---
        # 10 étapes max par tâche (1 run() = 1 step ici, mais le LLM
        # pourrait faire plusieurs appels internes — on se protège).
        "step_budget": 10,
    }
```

---

## Décortiquer chaque champ

### name et version

```python
"name": "file-assistant",
"version": "1.0.0",
```

`name` est l'identifiant utilisé dans `apollia-os run file-assistant "..."`. Il doit être unique sur le runtime. Convention : kebab-case, sans espaces.

`version` suit semver. Pour un agent local, elle est surtout informative. Pour un agent publié dans le registre communautaire, elle détermine les mises à jour automatiques.

### tools_required

```python
"tools_required": ["file_read", "file_write"],
```

Ces deux outils sont **bloquants** : si l'un d'eux est absent du Tool Registry au moment du déploiement, l'agent passe en état `STOPPED` avec un message d'erreur clair.

```bash
$ apollia-os agent start ./file_assistant.py
  Validation AIP...
  Résolution des outils...
    ✔ file_read — OK
    ✗ file_write — INTROUVABLE dans le registry
  ✗ Démarrage impossible : outil requis manquant
```

Ce comportement **fail-fast** est intentionnel. Il est préférable de découvrir un problème de configuration au déploiement plutôt qu'en cours d'exécution, quand une tâche réelle échoue.

> **`tools_required` vs `tools_optional`** — si vous déclarez un outil dans `tools_optional`, son absence ne bloque pas le démarrage : l'agent passe en `DEGRADED` (warning dans les logs) et peut continuer à fonctionner partiellement. Utilisez `tools_optional` pour les capacités améliorées mais non essentielles.

### max_concurrent_tasks

```python
"max_concurrent_tasks": 1,
```

Combien de tâches cet agent peut-il traiter en parallèle ? Le runtime crée un sémaphore de cette capacité et met en file d'attente toute tâche soumise au-delà.

Pour `file-assistant`, 1 est correct : l'agent fait un appel LLM qui peut prendre plusieurs secondes, et les appels fichiers sont séquentiels. Avec plusieurs tâches en parallèle, l'agent risquerait de générer des résumés partiels si deux tâches lisent et écrivent le même fichier simultanément.

### step_budget

```python
"step_budget": 10,
```

Le nombre maximum d'étapes que cet agent peut effectuer par tâche. Une "étape" est comptabilisée à chaque appel d'outil via `ctx.tools` ou à chaque appel LLM via `ctx.llm`.

Pour notre agent, une tâche normale consomme 3 étapes (file_read + llm.chat + file_write). Un budget de 10 laisse une marge confortable pour les appels supplémentaires (par exemple, si l'agent doit relire le fichier après une erreur partielle).

Si le budget est épuisé, le runtime lève une `PyRuntimeError` dans le code Python de l'agent — il faut gérer ce cas. Nous verrons comment dans la section `run()`.

> **Le runtime plafonne toujours le budget** — si votre manifest demande `step_budget: 1000` mais que les paramètres globaux du runtime limitent à 100, le runtime applique la valeur la plus basse. C'est le principe #7 des garde-fous non-contournables.

---

## Ce qui manque intentionnellement

Vous avez peut-être remarqué que ce manifest ne déclare pas :

- `memory_namespace` — on n'utilise pas `ctx.memory` dans cette version
- `network_allowlist` — on n'utilise pas `http_fetch`
- `tools_optional` — tous nos outils sont requis

C'est voulu. Un manifest minimal est plus facile à comprendre et à déboguer. On ajoutera ces champs quand l'agent en aura vraiment besoin.

---

## Récapitulatif

```python
def manifest(self):
    return {
        "name": "file-assistant",
        "version": "1.0.0",
        "description": "Lit un fichier, le résume via LLM, sauvegarde le résumé",
        "tools_required": ["file_read", "file_write"],
        "max_concurrent_tasks": 1,
        "step_budget": 10,
    }
```

Manifest défini. Passons à l'implémentation de `run()`.
