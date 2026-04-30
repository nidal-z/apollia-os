# code-review-multi

Code review multi-aspect : sécurité + style + performance. Règles métier externalisées dans APOLLIA.md (votre expertise, pas la nôtre).

**Niveau :** L2 (director + 3 workers, A2A)
**Format :** package
**Généré par :** apollia-agent-forge

## Architecture

```
┌──────────────────────┐
│ code-review-director │  (ReAct, agrégation)
└──────────┬───────────┘
           │ A2A
   ┌───────┼─────────────┐
   ▼       ▼             ▼
┌────────┐ ┌────────┐ ┌──────┐
│security│ │ style  │ │ perf │
│worker  │ │ worker │ │worker│
└────────┘ └────────┘ └──────┘
   │           │          │
   └─ ctx.workspace.get(...) ─┘
       (APOLLIA.md sections)
```

## Aperçu

| Aspect | Détail |
|---|---|
| Trigger | manuel |
| Outils director | `file_read`, `bash_executor` (pour git diff), `file_write` |
| Outils workers | `file_read`, `file_grep` |
| Mémoire | sémantique (cache `review:{hash}`), épisodique (audit) |
| Profil utilisateur | `user.tech.stack`, `user.constraints.compliance` |
| Notifications | desktop natif fin de review |
| Output | mémoire + `~/Documents/code-review-multi/{date}-{hash}.md` |
| HITL | aucun |
| Règles métier | **externalisées dans APOLLIA.md** (CRITIQUE) |

## Pourquoi ce design

Les règles de review changent d'une équipe à l'autre, d'un projet à l'autre. Hardcoder = forcer un style. Externaliser dans APOLLIA.md = livrer un agent **dont l'expertise reste sous votre contrôle**, modifiable sans toucher au code Python. C'est cette flexibilité qui transforme un agent générique en agent "sur mesure".

## Voir aussi
- `setup.md` — install + config
- `APOLLIA.md` — sections à coller dans votre workspace
- `examples/01-typical.md` — exemple input/output
- `datasources/example-target.py` — fichier de démo avec anti-patterns intentionnels
