# Exemple 1 — Typique

## Input
```
Résume cette URL : https://www.anthropic.com/news/claude-3-5-sonnet
```

## Output attendu
```markdown
# Claude 3.5 Sonnet — Anthropic

**Source :** https://www.anthropic.com/news/claude-3-5-sonnet
**Date :** 2026-04-30

## Résumé
Anthropic dévoile Claude 3.5 Sonnet, modèle intermédiaire de la famille 3.5...
(3-6 phrases)

## Points clés
- Performances en hausse vs Opus 3 sur la majorité des benchmarks
- Latence améliorée de 2x
- Disponibilité immédiate dans claude.ai et l'API
- ...

## Citations notables
> "We're committed to delivering frontier intelligence at a fraction of the cost"
```

## Ce qui est testé
- Lecture URL via web_read
- Personnalisation par user.tech.stack si présent
- Style maison via APOLLIA.md "Markdown Summary Style" si présent
- Cache mémoire sur clé summary:{hash}
- Output fichier ~/Documents/markdown-summarizer/{date}-{hash}.md
