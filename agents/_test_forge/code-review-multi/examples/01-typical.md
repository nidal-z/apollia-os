# Exemple — Review d'un fichier Python avec anti-patterns connus

## Input
```
Review datasources/example-target.py
```

## Output attendu (forme abrégée)
```markdown
# Review — datasources/example-target.py

**Date :** 2026-04-30

## 🔒 Sécurité
- **high** L5 — Credential en dur (`API_KEY`). *Recommandation :* charger depuis env/secrets manager.
- **critical** L18 — Hash MD5 pour mot de passe. *Recommandation :* utiliser bcrypt/argon2.
- **critical** L25 — SQL injection (string concat). *Recommandation :* requêtes paramétrées.

## 🎨 Style
- **medium** L8 — Pas de type hints. *Recommandation :* `users: list[dict], email: str -> dict | None`.
- **low** L8 — Naming générique (`u`). *Recommandation :* `user`.

## ⚡ Performance
- **medium** L9 — Lookup O(n) répété. *Recommandation :* indexer en dict si appelé dans une boucle.
- **high** L24 — N+1 query. *Recommandation :* `WHERE email IN (...)` une seule requête.

## Synthèse
- Total findings : 7
- Critiques : 2 (security)
- Hauts : 2
- Moyens/bas : 3
```

## Ce qui est testé
- Délégation A2A vers les 3 workers
- Lecture règles depuis APOLLIA.md (sections `Code Review — *`)
- Agrégation au format markdown
- Cache mémoire sur hash input
- Output fichier
