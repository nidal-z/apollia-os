# Les outils

Dans `file-assistant`, vous avez utilisé `file_read` et `file_write` sans vraiment comprendre comment ils fonctionnent. Ce chapitre comble ce manque.

Apollia OS embarque **13 outils natifs** couvrant les opérations fichiers, l'exécution shell, le réseau, la recherche mémoire, et l'interaction utilisateur. Chacun tourne dans un environnement isolé — sandbox Linux — et chaque appel est tracé dans l'audit trail.

---

## Vue d'ensemble des 13 outils

- `file_read` — lire un fichier (lecture partielle possible)
- `file_write` — créer ou remplacer un fichier (atomique)
- `file_edit` — remplacer chirurgicalement une chaîne exacte
- `file_list` — lister les entrées d'un répertoire
- `file_glob` — trouver des fichiers par pattern (`**/*.txt`)
- `file_grep` — recherche regex dans des fichiers
- `bash_executor` — exécuter une commande shell isolée
- `python_executor` — code Python dans un venv par agent
- `http_fetch` — requête HTTP (whitelist de domaines)
- `memory_search` — recherche FTS5 dans la mémoire persistante
- `ask_user` — demander une information à l'utilisateur (HITL léger)
- `notebook_read` — lire un notebook Jupyter (.ipynb) cellule par cellule
- `notebook_edit` — modifier une cellule de notebook Jupyter

> Pour les paramètres, formes de retour et codes d'erreur de chacun, voir la **section 1 — [Les outils natifs](./ch04-01-native-tools.md)**.

`http_fetch` et `memory_search` sont des **fonctionnalités optionnelles** (feature flags à la compilation). Ils ne sont disponibles que si le binaire a été compilé avec `--features http` et `--features memory-search` respectivement. Vérifiez avec `apollia-os tools list`.

---

## Ce que vous allez apprendre

- **Section 1 — Les 13 outils natifs** : paramètres, résultats, codes d'erreur — avec des exemples qui étendent `file-assistant`
- **Section 2 — Appeler un outil** : le pattern complet `ctx.tools.call`, la gestion d'erreurs, le step_budget, et comment choisir entre plusieurs outils pour une même tâche
- **Section 3 — Sandbox et sécurité** : comment Linux namespaces isole chaque exécution, les profils sandbox, et ce qui est protégé (et ce qui ne l'est pas)
- **Section 4 — Outils MCP** : connecter un serveur MCP externe pour accéder à des milliers d'outils additionnels
