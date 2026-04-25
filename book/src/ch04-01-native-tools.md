# Les outils natifs

Apollia OS embarque 10 outils natifs disponibles sans configuration : 6 outils fichiers, 1 shell, 1 Python, 1 HTTP, 1 mémoire. Tous s'appellent de la même façon depuis `run()` :

```python
result = await ctx.tools.call("nom_outil", { ...paramètres... })
```

L'appel est asynchrone — ORIA l'intercepte, applique le `StepBudget` et la `ResilienceLayer`, puis retourne le résultat structuré.

> **Référence technique :** [Référence des Outils Natifs](https://github.com/nidal-z/apollia-os/wiki/Outils-Reference) — paramètres complets, structures de retour, codes d'erreur pour chaque outil.

---

## Les 10 outils en un coup d'œil

| Catégorie | Outil | Usage typique |
|---|---|---|
| **Fichiers** | `file_read` | Lire le contenu d'un fichier (lecture partielle possible) |
| | `file_write` | Créer ou remplacer un fichier (écriture atomique) |
| | `file_edit` | Remplacer chirurgicalement une chaîne exacte |
| | `file_list` | Lister les entrées d'un répertoire |
| | `file_glob` | Chercher des fichiers par pattern (`**/*.txt`) |
| | `file_grep` | Recherche regex dans les fichiers (avec contexte) |
| **Shell** | `bash_executor` | Commande shell dans un environnement isolé |
| **Python** | `python_executor` | Code Python dans un venv par agent |
| **Réseau** | `http_fetch` | Requête HTTP (whitelist de domaines) |
| **Mémoire** | `memory_search` | Recherche FTS5/BM25 dans la mémoire persistante |

Tous les outils fichiers valident le chemin contre la `SandboxRoot` de l'agent avant toute opération disque.

---

## Patterns avec file-assistant

Les exemples ci-dessous s'appuient sur le file-assistant introduit au chapitre 2.

### Lister les fichiers disponibles

Quand l'utilisateur envoie "liste", l'agent répond avec les fichiers dans son sandbox :

```python
async def run(self, task, ctx):
    command = task["input"]["parts"][0]["text"].strip()

    if command == "liste":
        result = await ctx.tools.call("file_list", {"path": "/data/"})
        files = [e["name"] for e in result["entries"] if not e["is_dir"]]
        return AIPResult.completed("\n".join(files))

    # ... logique de résumé
```

`file_list` retourne les entrées avec `name`, `is_dir`, `size`, et `modified`. Le paramètre `depth` contrôle la récursion (défaut : 1 niveau).

### Résumer le rapport le plus récent

Les résultats de `file_glob` sont triés par date de modification, le plus récent en premier — ce qui simplifie ce pattern courant :

```python
async def run(self, task, ctx):
    # Trouver le rapport le plus récent
    glob_result = await ctx.tools.call("file_glob", {
        "pattern": "**/rapport*.txt",
        "path":    "/data/",
    })

    if not glob_result["matches"]:
        return AIPResult.failed("NO_REPORTS", "Aucun rapport trouvé dans /data/")

    # Le plus récent est toujours en première position
    latest = glob_result["matches"][0]

    # Lire et résumer
    content = await ctx.tools.call("file_read", {"path": latest})
    # ... appel LLM pour résumer content["content"]
```

### Modifier une valeur de configuration

`file_edit` opère chirurgicalement : il échoue si la chaîne est absente ou apparaît plusieurs fois. C'est une contrainte voulue — ambiguïté = refus :

```python
await ctx.tools.call("file_edit", {
    "path":    "/data/config.toml",
    "old_str": 'log_level = "info"',
    "new_str": 'log_level = "debug"',
})
```

Si `old_str` n'est pas unique, fournissez plus de contexte (une ligne avant ou après) pour désambiguïser.

---

## bash_executor : avec modération

`bash_executor` est le plus puissant des outils — et le plus risqué. Préférez les outils fichiers atomiques quand c'est possible :

```python
# ❌ Éviter si un outil fichier suffit
await ctx.tools.call("bash_executor", {"command": "cat /data/notes.txt", "timeout": 10})

# ✓ Préférer
await ctx.tools.call("file_read", {"path": "/data/notes.txt"})
```

Réservez `bash_executor` pour ce que les outils fichiers ne couvrent pas : compression, conversion de format, exécution de scripts existants.

---

## memory_search : outil vs `ctx.memory.search`

Les deux font la même recherche FTS5. La différence est le contexte d'appel :

- **`ctx.memory.search`** : appelé directement dans `run()` par le code Python de l'agent
- **`memory_search` comme outil** : utilisable depuis la boucle ReAct LLM (`run_tools()`), où le LLM décide lui-même quand chercher en mémoire

Le chapitre 5 explique quand utiliser l'un ou l'autre.

---

## Récapitulatif des appels

```python
await ctx.tools.call("file_read",        {"path": "..."})
await ctx.tools.call("file_write",       {"path": "...", "content": "..."})
await ctx.tools.call("file_edit",        {"path": "...", "old_str": "...", "new_str": "..."})
await ctx.tools.call("file_list",        {"path": "..."})
await ctx.tools.call("file_glob",        {"pattern": "**/*.txt"})
await ctx.tools.call("file_grep",        {"pattern": "regex", "path": "..."})
await ctx.tools.call("bash_executor",    {"command": "...", "timeout": 30})
await ctx.tools.call("python_executor",  {"code": "..."})
await ctx.tools.call("http_fetch",       {"url": "https://..."})
await ctx.tools.call("memory_search",    {"query": "..."})
```

> **Référence complète :** [Outils-Reference](https://github.com/nidal-z/apollia-os/wiki/Outils-Reference) — pour chaque outil : signature complète, tous les paramètres optionnels, structure de retour JSON, codes d'erreur, et contraintes sandbox.
