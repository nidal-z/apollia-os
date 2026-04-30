# markdown-summarizer

Résume une URL en markdown structuré, adapte le ton au profil de l'utilisateur, met les résumés en cache.

**Niveau :** L1 (agent solo standalone)
**Format :** package
**Généré par :** apollia-agent-forge

## Aperçu

| Aspect | Détail |
|---|---|
| Trigger | manuel (`apollia agent run markdown-summarizer --input "..."`) |
| Outils | `web_read` (requis), `file_write` + `http_fetch` + `memory_search` (optionnels) |
| Mémoire | sémantique (cache `summary:{hash}`), épisodique (audit) |
| Profil utilisateur | `user.tech.stack`, `user.tech.languages` |
| Notifications | desktop natif fin de run |
| Output | mémoire (cache) + fichier `~/Documents/markdown-summarizer/{date}-{hash}.md` |
| HITL | aucun |

## Installation rapide
```bash
apollia agent install ./
apollia agent run markdown-summarizer --input "Résume cette URL : https://exemple.fr"
```

Voir `setup.md` pour le détail.

## Fichiers du package
| Fichier | Rôle |
|---|---|
| `agent.toml` | Manifest package |
| `markdown_summarizer.py` | Code de l'agent |
| `APOLLIA.md` | Section à coller dans le workspace utilisateur (style maison) |
| `templates/summary.md` | Modèle de sortie (Handlebars-like) |
| `datasources/example-urls.txt` | URLs d'exemple pour tester |
| `examples/01-typical.md` | Paire input/output canonique |
| `setup.md` | Guide install client final |
| `CHANGELOG.md` | Historique versions |
