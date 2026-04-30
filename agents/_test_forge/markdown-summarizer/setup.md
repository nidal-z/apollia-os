# Setup — markdown-summarizer

## Prérequis
- Apollia OS v0.1.0+ (`apollia --version`)
- Backend LLM configuré (`apollia llm config`)
- Outils `web_read` et `file_write` activés

## Installation
```bash
apollia agent install ./
apollia agent list | grep markdown-summarizer
```

## Configuration

### 1. Section APOLLIA.md (optionnelle)
Copier le contenu de `APOLLIA.md` (fourni dans ce package) dans le `APOLLIA.md` à la racine de votre workspace pour personnaliser le style des résumés.

### 2. Profil utilisateur (recommandé)
Si vous avez déjà fait l'onboarding, l'agent lit automatiquement :
- `user.tech.stack` → adapte la profondeur technique
- `user.tech.languages` → adapte le vocabulaire

Pour vérifier : `apollia memory list --namespace __user__`.

### 3. Dossier de sortie
L'agent écrit dans `~/Documents/markdown-summarizer/`. Créez ce dossier si vous voulez activer l'output fichier (sinon l'agent log un warning et continue).

## Premier run
```bash
apollia agent run markdown-summarizer --input "Résume cette URL : https://example.com/article"
```

## Customisation avancée

| Quoi changer | Où | Comment |
|---|---|---|
| Style de résumé | `APOLLIA.md` section `Markdown Summary Style` | Texte libre injecté dans le prompt |
| Profondeur (max_steps) | `agent.toml` `step_budget.max_steps` | Défaut 12 |
| Output fichier | `markdown_summarizer.py` `_post_run()` | Modifier le path |
| Désactiver le cache | `markdown_summarizer.py` méthode `run` | Commenter le bloc `if cached: return` |

## Troubleshooting

| Erreur | Cause probable | Solution |
|---|---|---|
| `NO_LLM` | Backend LLM non configuré | `apollia llm config` |
| `NO_URL` | Pas d'URL dans l'input | Format : URL en clair dans le texte |
| Cache hit infini | Modification du contenu cible | `apollia memory forget summary:{hash}` |
| `web_read` échoue | SSRF guard bloque | Vérifier l'URL, ajouter à `network_allowlist` si nécessaire |

## FAQ

**Q: Comment forcer un nouveau résumé sur une URL déjà en cache ?**
R: `apollia memory forget summary:{hash}` puis relancer.

**Q: Comment lister les résumés en cache ?**
R: `apollia memory search "summary:" --namespace markdown-summarizer`.

**Q: Pourquoi pas de support PDF ?**
R: `web_read` fait du HTML extraction. Pour PDF, soit on extend l'agent (voir customisation), soit on utilise un agent dédié.
