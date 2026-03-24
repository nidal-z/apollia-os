# Brique — Memory Engine

> Apollia OS fournit un système de mémoire persistante par agent, basé sur SQLite et FTS5.
> Chaque agent dispose de son propre namespace isolé. La mémoire utilisateur globale vit dans
> un namespace réservé `__user__`.

---

## Vue d'ensemble

Le Memory Engine (`crate apollia-memory`) fournit trois types de mémoire :

| Type | Fichier | Usage |
|---|---|---|
| **Épisodique** | `episodic.rs` | Événements datés, journal d'exécution |
| **Sémantique** | `semantic.rs` | Connaissances clé-valeur avec confiance |
| **Procédurale** | `procedural.rs` | Procédures et recettes réutilisables |

Chaque type est accessible via le `MemoryManager` qui isole les namespaces par agent.

---

## MemoryStore

Couche SQLite de base (`store.rs`). Ouvre un fichier `.db` et applique les migrations au premier accès. Mode WAL activé pour la concurrence.

---

## Backends

### EpisodicMemory (`episodic.rs`)

Stocke des événements chronologiques associés à un agent.

### SemanticMemory (`semantic.rs`)

Stocke des paires clé-valeur avec un score de confiance et une source optionnelle. Chaque entrée est identifiée par `(namespace, key)`.

Méthodes principales :
- `remember(namespace, key, value, confidence, source, tags)` — upsert
- `recall(namespace, key)` — lecture par clé
- `recall_all(namespace)` — toutes les entrées d'un namespace
- `forget(namespace, key)` — suppression

### ProceduralMemory (`procedural.rs`)

Stocke des procédures réutilisables avec tags et versioning.

---

## Recherche FTS5 (`search.rs`)

Recherche plein texte via SQLite FTS5 avec classement BM25. Supporte le filtrage par source (`SearchSource::Episodic`, `Semantic`, `Procedural`).

---

## MemoryManager (`manager.rs`)

Point d'entrée pour les agents. Gère l'isolation par namespace et le niveau d'accès (`ReadWrite` / `ReadOnly`). Les stores sont ouverts en mode lazy (à la première utilisation).

---

## Namespace `__user__` — Mémoire utilisateur globale

Le namespace réservé `__user__` stocke la mémoire utilisateur globale, distincte des namespaces agents. Ce namespace est géré exclusivement par le `UserMemoryRepository` (`user_memory.rs`).

### UserMemoryRepository

Repository dédié à la mémoire utilisateur. Utilise le `SemanticMemory` sous le namespace `__user__` avec des clés composites `category.key`.

**Catégories** : `preferences`, `habits`, `context`
**Sources** : `onboarding`, `chat_inference`, `user_explicit`, `agent_observation`

Méthodes principales :
- `store(category, key, value, source)` — upsert d'une entrée
- `recall(category, limit)` — lecture par catégorie
- `search(query, limit)` — recherche FTS5 cross-catégories
- `recall_all_for_injection(max_entries)` — format texte pour injection dans le system prompt LLM
- `forget(key)` / `update(key, value)` — suppression / mise à jour

Le format d'injection produit un bloc groupé par catégorie :
```text
Category: preferences
- language: français
- format: markdown
Category: habits
- working_hours: 9h-18h
```

Pour la documentation complète du sous-système, voir [Briques-User-Memory.md](Briques-User-Memory.md).

**Décision architecturale** : [ADR-038 — Mémoire utilisateur globale](adr/ADR-038-global-user-memory.md)

---

## Fichiers du crate

| Fichier | Contenu |
|---|---|
| `store.rs` | `MemoryStore` — couche SQLite, migrations |
| `episodic.rs` | `EpisodicMemory` — mémoire épisodique |
| `semantic.rs` | `SemanticMemory` — mémoire sémantique |
| `procedural.rs` | `ProceduralMemory` — mémoire procédurale |
| `search.rs` | `MemorySearch` — recherche FTS5 BM25 |
| `manager.rs` | `MemoryManager` — isolation namespace, access level |
| `user_memory.rs` | `UserMemoryRepository` — mémoire utilisateur globale (`__user__`) |
