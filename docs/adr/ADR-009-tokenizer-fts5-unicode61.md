# ADR-009 - Tokenizer FTS5 `unicode61` pour la recherche mémorielle

**Date :** 2026-03
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation

---

## Contexte

La cible principale d'Apollia OS est le marché PME français. Les agents gèrent des documents en français : devis, emails, notes de réunion. Les recherches dans la mémoire doivent être insensibles aux accents : "réunion" doit retrouver "reunion", "société" doit retrouver "societe", "éléphant" doit retrouver "elephant". FTS5 SQLite offre plusieurs tokenizers avec des comportements différents.

## Décision

Nous utilisons le tokenizer `unicode61` pour toutes les tables FTS5 de `apollia-memory`. La déclaration est explicite dans le schéma : `USING fts5(content, tokenize='unicode61')`. Ce tokenizer normalise les caractères Unicode (suppression des diacritiques) avant l'indexation et la recherche.

## Alternatives considérées

### Option A - Tokenizer `simple` (rejetée)
**Pour :** Défaut SQLite, aucune configuration.
**Contre :** Ne gère pas les accents. "réunion" ne retrouve pas "reunion". Inacceptable pour une cible francophone.

### Option B - Tokenizer `porter` (rejetée)
**Pour :** Stemming - "running" retrouve "run".
**Contre :** Algorithme de stemming anglais uniquement. Inapplicable au français. "réunion" toujours problématique.

### Option C - Tokenizer ICU custom (rejetée)
**Pour :** Gestion Unicode complète, multi-langue, stemming par langue.
**Contre :** Dépendance externe (libicu). Viole Principe #2. Compilation complexe. Over-engineered pour les besoins PME.

### Option retenue - `unicode61`
**Pour :** Inclus nativement dans SQLite. Normalise les diacritiques. Zéro dépendance supplémentaire.
**Compromis acceptés :** Légèrement plus lent que `simple` à l'indexation. Non significatif pour les volumes PME (< 100k épisodes).

## Conséquences

**Positives :**
- "réunion" retrouve "reunion", "société" retrouve "societe" - nativement.
- Zéro dépendance externe : `unicode61` est dans SQLite bundlé.
- BM25 scoring reste fiable avec `unicode61`.

**Négatives / Compromis :**
- Pas de stemming français (recherche "générer" ne retrouve pas "génération").
- Légèrement plus lent à l'indexation qu'avec `simple`.

**Neutres / À surveiller :**
- Évaluer si le manque de stemming est problématique sur des cas réels PME.
- Considérer un tokenizer custom avec stemming français en v1.0 si besoin.

## Principes architecturaux impactés

- Principe #2 - Zéro dépendance externe : `unicode61` est natif SQLite.
- Principe #1 - Local-first : Recherche plein texte entièrement locale.

## Liens

- Story associée : STORY-020 (FTS5 search avec tokenizer unicode61 + BM25)
- ADR précédent sur le même sujet : aucun
