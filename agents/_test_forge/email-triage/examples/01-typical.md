# Exemple — Triage matinal (4 emails)

## Input
```
Triage inbox depuis hier 18h
```

## Plan attendu (généré par ORIA Reasoner)
```
step-1: http_fetch GET gmail/messages?q=is:unread (HITL approval)
step-2: pour chaque message, http_fetch GET gmail/messages/{id} (HITL approval × N)
step-3: classer chaque email (LLM uniquement, pas d'outil)
step-4: pour les classés "draft" → http_fetch POST drafts (HITL approval × M)
step-5: pour les classés "label" → http_fetch POST labels (HITL approval × K)
step-6: pour les classés "archive" → http_fetch POST archive (HITL approval × L)
```

## Output attendu (après on_plan_complete)
```markdown
# Triage Inbox — Synthèse

- **Emails triés :** 4
- **Drafts préparés (HITL pending) :** 1
- **Labels appliqués :** 2
- **Archivés :** 1
- **Ignorés :** 0

## ⚠️ Escaladés (1)
- pdg@maboite.com — URGENT: validation contrat client X avant 18h
  Raison : urgent + VIP + deadline aujourd'hui
```

## Ce qui est testé
- Mode orchestré ORIA (Reasoner génère plan, ActorLoop exécute)
- `tools_requiring_approval=["http_fetch"]` → HITL automatique avant chaque appel Gmail
- Lecture règles APOLLIA.md via system_prompt instruisant le Reasoner
- on_plan_complete qui agrège les step_results en synthèse
