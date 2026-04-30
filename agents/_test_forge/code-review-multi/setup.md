# Setup — code-review-multi

## Prérequis
- Apollia OS v0.1.0+
- Backend LLM configuré
- Outils activés : `file_read`, `file_grep`, `file_write`, `bash_executor`

## Installation
```bash
apollia agent install ./
apollia agent list | grep code-review
```

## Configuration — ÉTAPE CRUCIALE

⚠️ **Sans la configuration APOLLIA.md, les workers tournent en fallback générique** (qualité ≈ 50% du potentiel).

### 1. Copier les sections APOLLIA.md

Ouvrir `APOLLIA.md` à la racine de votre workspace et y coller (ou compléter) les 3 sections fournies dans `APOLLIA.md` de ce package :
- `## Code Review — Security Rules`
- `## Code Review — Style Guide`
- `## Code Review — Performance Budget`

### 2. Remplir avec les règles de votre équipe

Ces sections sont VOTRE expertise métier. Plus elles sont précises, meilleurs sont les workers.

Exemple security pour une équipe Python/FastAPI :
```markdown
## Code Review — Security Rules

- Aucun secret en dur — tous via `cfg.secrets.X`.
- Endpoints : décorés `@requires_auth(...)` ou explicitement `@public`.
- Inputs : pydantic models, jamais de dict brut.
- Logs : interdiction de logger email/phone/token.
- DB : SQLAlchemy ORM, jamais de SQL string concat.
```

### 3. Profil utilisateur (recommandé)
- `user.tech.stack` → contextualise les recommandations
- `user.constraints.compliance` → escalade les findings security si "GDPR" ou "PCI-DSS"

## Premier run

Sur le fichier d'exemple fourni (anti-patterns intentionnels) :
```bash
apollia agent run code-review-director --input "Review agents/_test_forge/code-review-multi/datasources/example-target.py"
```

Sur un diff git :
```bash
apollia agent run code-review-director --input "Review le diff entre HEAD~1 et HEAD"
```

## Customisation avancée

| Quoi | Où | Comment |
|---|---|---|
| Ajouter un worker (ex: `accessibility`) | dupliquer un worker, ajouter `[[agents]]` au manifest, ajouter `a2a:review-accessibility` dans `tools_optional` du director | |
| Ajuster sévérité par finding | modifier le system prompt du worker | |
| Désactiver un aspect | retirer le worker de `tools_optional` du director | |

## Troubleshooting

| Problème | Cause | Solution |
|---|---|---|
| Workers en mode fallback | APOLLIA.md sans les sections | Voir étape 1 ci-dessus |
| Findings de mauvaise qualité | Règles trop vagues dans APOLLIA.md | Préciser, ajouter des exemples |
| Output non-JSON workers | Modèle LLM trop petit | Augmenter `MAX_STEPS` worker ou changer backend |

## FAQ

**Q: Pourquoi externaliser les règles dans APOLLIA.md plutôt que les hardcoder ?**
R: Chaque équipe a ses règles. Hardcoder = forcer un style ; APOLLIA.md = adaptable, sous le contrôle du client. C'est aussi un argument de prestation : "vos règles, votre style".

**Q: Comment lancer en parallèle les 3 workers ?**
R: Le director appelle les workers séquentiellement via le ReAct loop. Apollia ne supporte pas encore le fan-out parallèle dans un director ReAct (voir Pipeline Engine pour ce besoin).

**Q: Cache reviewé ?**
R: Oui, clé `review:{hash(input)}`. Pour rejouer : `apollia memory forget review:{hash}`.
