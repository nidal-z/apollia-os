# Triage Inbox — {{date}}

## Synthèse
- Emails triés : {{triaged_count}}
- Drafts préparés (HITL pending) : {{actions.reply_draft}}
- Labels appliqués : {{actions.label}}
- Archivés : {{actions.archive}}
- Ignorés : {{actions.skip}}

## ⚠️ Escaladés ({{#count escalated}})
{{#each escalated}}
- **{{this.from}}** — {{this.subject}}
  *Raison :* {{this.reason}}
{{/each}}

## Drafts préparés (en attente d'approbation)
{{#each drafts}}
### Pour {{this.recipient}}
**Sujet :** {{this.subject}}
**Corps :**
> {{this.body}}
{{/each}}
