<!-- Bloc à coller dans le APOLLIA.md à la racine de votre workspace.
     Lu par les workers de code-review-multi via ctx.workspace.get(...).

     ⚠️ CRUCIAL : sans ces sections, les workers utilisent un fallback générique. La qualité dépend
     directement du soin que tu mets à remplir ces règles métier. C'est ICI que vit l'expertise
     de ton équipe — pas dans le code Python des agents. -->

## Code Review — Security Rules

<!-- Liste exhaustive des règles de sécurité de TON projet/équipe. Exemples :

- Aucune chaîne de connexion DB en dur — toujours via secrets manager `cfg.db_url`.
- Tous les endpoints HTTP sont décorés par `@requires_auth(...)` ou explicitement marqués `@public`.
- Inputs utilisateur : validés via `pydantic.BaseModel`, jamais consommés bruts.
- Logs : interdit de logger : `email`, `phone`, `ssn`, `bearer_token`, `api_key_*`.
- Crypto : SHA-256 minimum pour les hashes d'identifiants. MD5/SHA1 réservés aux empreintes non-secret.
- DB : ORM uniquement, pas de SQL string concat. -->

(à remplir par l'équipe)

## Code Review — Style Guide

<!-- Style guide concret de l'équipe. Exemples :

- Python : type hints obligatoires, `from __future__ import annotations`, max 100 cols.
- TypeScript : strict mode on, pas de `any` hors tests, exports nommés (pas de `default`).
- Naming : functions verb-first (`fetch_user`, `parse_payload`), classes noun-PascalCase.
- Tests : pattern GIVEN/WHEN/THEN dans les commentaires. -->

(à remplir par l'équipe)

## Code Review — Performance Budget

<!-- Budget perf et anti-patterns à signaler. Exemples :

- Aucune query DB dans une boucle (forcer batch + IN).
- Allocations dans hot loops : signaler systématiquement.
- Réponse API < 500ms p99 — signaler tout calcul O(n²) sur n>100 dans le path critique.
- Cache : signaler les fonctions pures coûteuses sans `@lru_cache`. -->

(à remplir par l'équipe)
