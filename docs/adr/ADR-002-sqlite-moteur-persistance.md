# ADR-002 - SQLite comme seul moteur de persistance

**Date :** 2026-03
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation

---

## Contexte

Le principe "zéro dépendance externe" (Principe #2) interdit d'exiger PostgreSQL, Redis, Qdrant ou tout autre service externe. La mémoire des agents doit être persistante, searchable en plein texte, et souveraine (Principe #1). Le volume de données cible est PME : quelques milliers d'épisodes par agent, pas des millions de lignes.

## Décision

Nous utilisons SQLite avec l'extension FTS5 (plein texte) comme seul moteur de persistance. La feature `bundled` de rusqlite compile SQLite directement dans le binaire. Un fichier `.db` par namespace mémoire d'agent : `~/.apollia/memory/<namespace>.db`. sqlite-vec est optionnel pour la recherche vectorielle.

## Alternatives considérées

### Option A - PostgreSQL (rejetée)
**Pour :** Concurrent robuste, FTS mature, JSON natif.
**Contre :** Nécessite un service PostgreSQL séparé. Viole directement Principe #2. Non opérable sans infrastructure.

### Option B - DuckDB (rejetée)
**Pour :** Performant pour les requêtes analytiques, zero-copy.
**Contre :** Optimisé pour OLAP (lectures massives), pas pour les insertions fréquentes d'une mémoire d'agent (OLTP). Moins adapté aux accès concurrents.

### Option C - Fichiers JSON (rejetée)
**Pour :** Simplicité maximale.
**Contre :** Pas de recherche plein texte. Pas de TTL natif. Pas de transactions. Non viable pour la recherche sémantique.

### Option D - LanceDB (rejetée)
**Pour :** Orienté vectoriel natif, Rust API.
**Contre :** Moins mature que sqlite-vec. Dépendance supplémentaire. La recherche vectorielle n'est pas requise pour le MVP.

### Option retenue - SQLite + FTS5 + sqlite-vec optionnel
**Pour :** Zéro dépendance externe (bundled). FTS5 suffisant pour les PME francophones. Un fichier = un namespace = isolation parfaite. Bien supporté par rusqlite.
**Compromis acceptés :** Concurrence limitée (WAL mode atténue). Pas de recherche vectorielle sans modèle d'embedding.

## Conséquences

**Positives :**
- Zero dépendance système : SQLite est compilé dans le binaire via `bundled`.
- Un fichier `.db` par namespace = isolation forte des données entre agents.
- FTS5 avec `unicode61` gère le français correctement (ADR-009).
- Migrations versionnées via `rusqlite` → rollback possible.

**Négatives / Compromis :**
- Concurrence limitée en écriture (WAL mode réduit le problème mais ne l'élimine pas).
- Recherche vectorielle non disponible sans sqlite-vec (optionnel, non bundlé).
- Pas adapté à des volumes > 100M de lignes (non pertinent pour la cible PME).

**Neutres / À surveiller :**
- Performance FTS5 à mesure que la mémoire des agents grossit (STORY-020).
- Intégration sqlite-vec si les agents ont besoin d'embedding (post-Sprint 3).

## Principes architecturaux impactés

- Principe #1 - Local-first : Données stockées localement, zéro cloud.
- Principe #2 - Zéro dépendance externe : SQLite bundlé dans le binaire.

## Liens

- Story associée : STORY-017 (Schema SQLite + migrations)
- ADR précédent sur le même sujet : aucun
