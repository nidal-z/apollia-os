# email-triage

Triage automatique de l'inbox Gmail : classification + actions proposées + HITL avant tout envoi.

**Niveau :** L3 (orchestré ORIA)
**Format :** package
**Généré par :** apollia-agent-forge

## Architecture

```
ORIA Observer  → ContextBundle (mémoire pertinente, règles APOLLIA.md via system_prompt)
ORIA Reasoner  → ExecutionPlan (steps : fetch inbox, classer, draft, label, archive)
ORIA ActorLoop → exécute step-by-step
                 ↓ AVANT CHAQUE http_fetch : HITL approval (tools_requiring_approval)
EmailTriageAgent.on_plan_complete → synthèse markdown
```

## Aperçu

| Aspect | Détail |
|---|---|
| Triggers | cron `0 8 * * 1-5` (matinal), manuel |
| Outils | `http_fetch` (Gmail API), `memory_search`, `file_write` |
| HITL | `tools_requiring_approval=["http_fetch"]` → approbation avant chaque appel sortant |
| Mémoire | namespace `email-triage`, credentials Gmail stockés ici |
| Profil utilisateur | `user.agents.hitl` documenté mais pas lu dynamiquement (gap v0.1) |
| Output | synthèse markdown via `on_plan_complete` |

## Pourquoi L3 (orchestré) ?

Triage email = tâche **non-déterministe** : le plan dépend du contenu (4 emails ≠ 40 emails, urgents ≠ newsletters). ORIA génère un plan adapté à chaque inbox plutôt que de coder une boucle ReAct rigide. Le coût LLM supplémentaire (Reasoner + Actor) est compensé par l'auditabilité (plan visible) et la flexibilité.

## Limitations majeures

- **Pas d'outil gmail natif Apollia v0.1** → wrappers http_fetch + auth manuelle. Voir `setup.md`.
- **`user.agents.hitl` non lu dynamiquement** → comportement HITL fixé au manifest.
- **Triage séquentiel** (pas de fan-out parallèle dans ORIA actuel).

Ces limitations sont des stories candidates documentées dans `setup.md` § Roadmap.

## Voir aussi
- `setup.md` — install + auth Gmail + config APOLLIA.md
- `APOLLIA.md` — sections classification + VIP list + templates
- `examples/01-typical.md` — exemple plan ORIA + output
- `datasources/example-inbox.json` — format Gmail simplifié pour démo
