# Les outils

Dans `file-assistant`, vous avez utilisé `file_read` et `file_write` sans vraiment comprendre comment ils fonctionnent. Ce chapitre comble ce manque.

Apollia OS embarque **10 outils natifs** couvrant les opérations fichiers, l'exécution shell, le réseau, et la recherche mémoire. Chacun tourne dans un environnement isolé — sandbox Linux — et chaque appel est tracé dans l'audit trail.

---

## Vue d'ensemble des 10 outils

| Outil | Catégorie | Ce qu'il fait |
|---|---|---|
| `file_read` | Fichiers | Lire un fichier (lecture partielle incluse) |
| `file_write` | Fichiers | Créer ou remplacer un fichier |
| `file_edit` | Fichiers | Remplacer une chaîne exacte dans un fichier |
| `file_list` | Fichiers | Lister les entrées d'un répertoire |
| `file_glob` | Recherche | Trouver des fichiers par pattern (`**/*.txt`) |
| `file_grep` | Recherche | Chercher par regex dans des fichiers |
| `bash_executor` | Shell | Exécuter une commande shell |
| `python_executor` | Python | Exécuter du code Python dans un venv isolé |
| `http_fetch` | Réseau | Faire une requête HTTP |
| `memory_search` | Mémoire | Chercher dans la mémoire persistante de l'agent |

`http_fetch` et `memory_search` sont des **fonctionnalités optionnelles** (feature flags à la compilation). Ils ne sont disponibles que si le binaire a été compilé avec `--features http` et `--features memory-search` respectivement. Vérifiez avec `apollia-os tools list`.

---

## Ce que vous allez apprendre

- **Section 1 — Les 10 outils natifs** : paramètres, résultats, codes d'erreur — avec des exemples qui étendent `file-assistant`
- **Section 2 — Appeler un outil** : le pattern complet `ctx.tools.call()`, la gestion d'erreurs, le step_budget, et comment choisir entre plusieurs outils pour une même tâche
- **Section 3 — Sandbox et sécurité** : comment Linux namespaces isole chaque exécution, les profils sandbox, et ce qui est protégé (et ce qui ne l'est pas)
- **Section 4 — Outils MCP** : connecter un serveur MCP externe pour accéder à des milliers d'outils additionnels
