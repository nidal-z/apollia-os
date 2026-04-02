# Concevoir l'agent

Avant d'écrire une ligne de code, prenons 5 minutes pour concevoir l'agent. Cette étape est souvent négligée, mais elle évite de se retrouver à réécrire le manifest à mi-chemin.

---

## Ce que l'agent doit faire

L'utilisateur envoie une instruction en texte libre :

```
"Résume /data/rapport.txt"
```

L'agent doit :

1. **Extraire le chemin du fichier** depuis le texte d'entrée
2. **Lire le fichier** pour récupérer son contenu
3. **Résumer le contenu** via LLM
4. **Sauvegarder le résumé** dans un fichier `_summary.txt` à côté du fichier original
5. **Retourner le résumé** à l'appelant

---

## Quels outils ?

Pour accomplir ces étapes, l'agent a besoin de :

| Étape | Outil | Pourquoi |
|---|---|---|
| Lire le fichier | `file_read` | Lit un fichier avec protection path traversal |
| Sauvegarder le résumé | `file_write` | Écrit un fichier (crée ou remplace) |
| Résumer | `ctx.llm` | Appel LLM — pas un outil Tool Registry, c'est un service `ctx` |

Pas de `bash_executor`. Pas de `http_fetch`. Pas de mémoire persistante. On commence simple.

---

## Les cas d'erreur à anticiper

Un bon agent gère les erreurs à la source, dans `run()`, plutôt que de laisser le runtime attraper des exceptions Python. Voici les cas à couvrir :

| Cas | Comportement attendu |
|---|---|
| Aucun texte dans l'entrée | `status: "failed"`, code `MISSING_INPUT` |
| Aucun chemin trouvé dans le texte | `status: "failed"`, code `NO_FILE_PATH` |
| Fichier introuvable | `status: "failed"`, code `FILE_NOT_FOUND` |
| LLM non disponible | `status: "failed"`, code `LLM_UNAVAILABLE` |
| Fichier trop volumineux | Résumé par tranches (optionnel — version simple : erreur `FILE_TOO_LARGE`) |

Pour cette première version, on traitera les 4 premiers cas. Le découpage par tranches est laissé en exercice.

---

## Architecture de run()

```
run(task, ctx)
│
├── 1. Extraire le chemin depuis task["input"]["parts"][0]["text"]
│       └── Erreur → status: "failed", code: NO_FILE_PATH
│
├── 2. ctx.tools.call("file_read", {"path": chemin})
│       └── Erreur → status: "failed", code: FILE_NOT_FOUND
│
├── 3. ctx.llm.chat(system=SYSTEM_PROMPT, user=contenu_fichier)
│       └── ctx.llm is None → status: "failed", code: LLM_UNAVAILABLE
│
├── 4. ctx.tools.call("file_write", {"path": chemin_summary, "content": résumé})
│
└── 5. return { status: "completed", output: [résumé + chemin sauvegardé] }
```

---

## Décisions de conception

**Pourquoi pas de mémoire persistante ?**
L'agent sauvegarde le résumé dans un fichier — c'est suffisant pour cette version. La mémoire persistante (via `ctx.memory`) est utile quand on veut retrouver des résumés antérieurs en recherche plein texte. On l'ajoutera au chapitre 5.

**Pourquoi `tools_required` et non `tools_optional` ?**
L'agent ne peut pas fonctionner sans lire et écrire des fichiers. Si ces outils sont absents, le runtime doit refuser de démarrer plutôt que de lancer un agent qui échouera à chaque tâche.

**Pourquoi une seule tâche concurrente ?**
Les appels LLM ont une latence variable. Démarrer avec `max_concurrent_tasks: 1` est toujours plus sage — on augmente cette valeur quand on a mesuré un besoin.

**Comment extraire le chemin du texte ?**
On utilise une heuristique simple : le premier token qui ressemble à un chemin de fichier (commence par `/`, `./`, `~/`, ou se termine par une extension connue). Pour une version production, on utiliserait le LLM lui-même pour extraire le chemin — c'est exactement ce que montre le chapitre 2 avancé.

---

## Résultat de la conception

À l'issue de cette étape, on sait :

- **Nom de l'agent :** `file-assistant`
- **Outils requis :** `file_read`, `file_write`
- **Services ctx :** `ctx.llm` (requis)
- **Tâches concurrentes :** 1
- **Cas d'erreur :** 4 cas explicites

On a tout ce qu'il faut pour écrire le manifest.
