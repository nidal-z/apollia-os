# Review — {{target}}

**Date :** {{date}}
**Reviewer :** code-review-multi v{{version}}

## 🔒 Sécurité
{{#each security.findings}}
- **{{this.severity}}** L{{this.line}} — {{this.description}}
  *Recommandation :* {{this.recommendation}}
{{/each}}
{{#unless security.findings}}
✅ Aucun problème détecté.
{{/unless}}

## 🎨 Style
{{#each style.findings}}
- **{{this.severity}}** L{{this.line}} — {{this.description}}
  *Recommandation :* {{this.recommendation}}
{{/each}}
{{#unless style.findings}}
✅ Aucun écart détecté.
{{/unless}}

## ⚡ Performance
{{#each perf.findings}}
- **{{this.severity}}** L{{this.line}} — {{this.description}}
  *Recommandation :* {{this.recommendation}}
{{/each}}
{{#unless perf.findings}}
✅ Aucun problème détecté.
{{/unless}}

## Synthèse
- Total findings : {{totals.total}}
- Critiques : {{totals.critical}}
- Hauts : {{totals.high}}
- Moyens/bas : {{totals.medium_low}}
