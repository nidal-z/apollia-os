# Brique — Mémoire Utilisateur Globale

> **Principe fondamental** : La mémoire utilisateur est un **outil**, pas une règle.
> Le LLM reçoit le contexte utilisateur comme information supplémentaire dans le system prompt.
> Il **DÉCIDE** quoi en faire. Ce n'est **JAMAIS** un filtre déterministe qui modifie le comportement du runtime.
> (Extension du Principe #6 — mémoire à initiative de l'agent.)

---

## Vue d'ensemble

La mémoire utilisateur globale permet à Apollia OS de conserver des informations persistantes sur l'utilisateur entre les sessions de chat et les exécutions d'agents. Ces informations enrichissent le contexte des interactions sans jamais contraindre le comportement du LLM.

Le sous-système repose sur trois piliers :

1. **Stockage** — `UserMemoryRepository` persiste les entrées dans SQLite via `SemanticMemory` sous le namespace réservé `__user__`.
2. **Injection** — `BuiltInChatAgent.build_system_prompt()` injecte le contexte utilisateur dans le system prompt du LLM.
3. **Extraction** — Un appel LLM post-session (fire-and-forget) extrait automatiquement de nouvelles informations depuis les conversations.

**Décision architecturale** : [ADR-038 — Mémoire utilisateur globale](adr/ADR-038-global-user-memory.md)

---

## Architecture

### Namespace `__user__`

Toutes les entrées de mémoire utilisateur vivent sous le namespace réservé `__user__` dans le `SemanticMemory` existant (crate `apollia-memory`). Ce namespace est distinct des namespaces agents (`agent.<name>`) et n'est jamais accessible directement par les agents.

```
┌─────────────────────────────────────────┐
│           MemoryStore (SQLite)          │
├───────────────┬─────────────────────────┤
│ __user__      │  Mémoire utilisateur    │
│ agent.foo     │  Mémoire agent foo      │
│ agent.bar     │  Mémoire agent bar      │
└───────────────┴─────────────────────────┘
```

### UserMemoryRepository

**Fichier** : `crates/apollia-memory/src/user_memory.rs`

Le repository est le point d'entrée unique pour toutes les opérations sur la mémoire utilisateur. Il délègue le stockage à `SemanticMemory` et `MemorySearch`, en ajoutant un typage par catégorie et source.

**Clés composites** : Chaque entrée est stockée sous la clé `category.key` dans le `SemanticMemory` (ex. `preferences.language`, `habits.working_hours`).

```rust
pub struct UserMemoryRepository {
    store: MemoryStore,
}
```

Méthodes publiques :

| Méthode | Description |
|---|---|
| `new(db_path)` | Ouvre ou crée la base `user_memory.db` |
| `store(category, key, value, source)` | Upsert d'une entrée |
| `recall(category, limit)` | Récupère les entrées d'une catégorie |
| `search(query, limit)` | Recherche FTS5 BM25 cross-catégories |
| `recall_all_for_injection(max_entries)` | Texte formaté pour injection LLM |
| `forget(key)` | Supprime une entrée (toutes catégories) |
| `update(key, value)` | Met à jour la valeur d'une entrée existante |

---

## Catégories

Trois catégories structurent la mémoire utilisateur (`UserMemoryCategory`) :

| Catégorie | Usage | Exemples |
|---|---|---|
| `preferences` | Préférences explicites de l'utilisateur | `language: français`, `format: markdown`, `tone: concis` |
| `habits` | Habitudes observées par le système ou les agents | `working_hours: 9h-18h`, `review_frequency: daily` |
| `context` | Informations contextuelles sur l'utilisateur | `current_project: apollia-os`, `role: CTO`, `team: backend` |

Les catégories sont sérialisées en `snake_case` dans l'API REST et les réponses JSON.

---

## Sources

Chaque entrée est associée à une source (`UserMemorySource`) qui indique son origine :

| Source | Description | Confiance implicite |
|---|---|---|
| `onboarding` | Renseignée lors de l'onboarding conversationnel (voir [Guide Onboarding](Agents-Onboarding-Guide.md)) | Élevée (0.9 explicite, 0.5 inféré) |
| `chat_inference` | Inférée par le LLM depuis une conversation | Moyenne |
| `user_explicit` | Saisie manuellement par l'utilisateur | Élevée |
| `agent_observation` | Observée par un agent pendant l'exécution d'une tâche | Moyenne |

---

## API REST

**Fichier** : `crates/apollia-runtime/src/api/routes_user.rs`

### `GET /api/v1/user/profile`

Retourne le profil utilisateur agrégé depuis les trois catégories.

**Réponse** `200 OK` :
```json
{
  "name": "Nidal",
  "preferences": { "language": "français", "format": "markdown" },
  "habits": { "working_hours": "9h-18h" },
  "context": { "current_project": "apollia-os", "role": "CTO" }
}
```

### `PUT /api/v1/user/profile`

Upsert du profil utilisateur. Seuls les champs fournis sont modifiés (merge, pas replace).

**Corps** :
```json
{
  "name": "Nidal",
  "preferences": { "language": "français" },
  "habits": { "working_hours": "9h-18h" }
}
```

**Réponse** : `200 OK`

### `GET /api/v1/user/memory`

Liste les entrées mémoire avec filtrage optionnel.

**Paramètres query** :
- `category` (optionnel) : `preferences`, `habits`, ou `context` — filtre par catégorie. Retourne `422` si invalide.
- `limit` (optionnel) : nombre max d'entrées (défaut : 100).

**Réponse** `200 OK` :
```json
{
  "entries": [
    {
      "key": "language",
      "value": "français",
      "source": "user_explicit",
      "updated_at": "2026-03-24T10:00:00Z"
    }
  ]
}
```

### `DELETE /api/v1/user/memory/:key`

Supprime une entrée mémoire par clé (scan toutes catégories).

**Réponse** : `204 No Content` — ou `404 Not Found` si la clé n'existe pas.

**Codes d'erreur communs** : `503 Service Unavailable` si la mémoire utilisateur n'est pas configurée.

---

## Injection dans le chat

**Fichier** : `crates/apollia-runtime/src/chat/builtin_agent.rs`

### Mécanisme

La méthode `BuiltInChatAgent::build_system_prompt()` injecte le contexte utilisateur dans le system prompt du LLM :

1. Appel à `repo.recall_all_for_injection(50)` — récupère jusqu'à 50 entrées.
2. Si le résultat est non-vide, ajoute un bloc `## User Context (for reference, use as you see fit)`.
3. En cas d'erreur (mutex poisonné, erreur SQLite), le bloc est simplement omis — le chat continue normalement.

### Format du bloc injecté

```text
## User Context (for reference, use as you see fit)
Category: preferences
- language: français
- format: markdown
Category: habits
- working_hours: 9h-18h
Category: context
- current_project: apollia-os
- role: CTO
```

Les entrées sont groupées par catégorie dans l'ordre : `preferences` → `habits` → `context`.

### Placement dans le system prompt

Le bloc est ajouté **à la fin** du system prompt de base, séparé par deux retours à la ligne. L'ordre final des messages LLM est :

```
[system prompt + user context] → [summary (si présent)] → [messages windowed] → [user message]
```

---

## Extraction LLM

**Fichier** : `crates/apollia-runtime/src/chat/extractor.rs`

### Déclenchement

L'extraction est lancée automatiquement à la fermeture d'une session de chat, **si et seulement si** la conversation contient au moins 4 messages.

### Processus

1. `extract_user_memory(messages, llm)` formate la conversation en transcript et l'envoie au LLM avec un prompt d'extraction.
2. Le LLM répond avec un objet JSON structuré par catégorie :
   ```json
   {
     "preferences": [{"key": "...", "value": "..."}],
     "habits": [{"key": "...", "value": "..."}],
     "context": [{"key": "...", "value": "..."}]
   }
   ```
3. Le parseur gère les réponses enveloppées dans des blocs markdown (` ```json ... ``` `).
4. Chaque entrée extraite est stockée avec `UserMemorySource::ChatInference` (upsert).

### Fire-and-forget

`spawn_extraction()` lance l'extraction dans une tâche Tokio asynchrone avec un timeout de 30 secondes. Les erreurs sont loguées en `warn!` mais **jamais propagées** — la fermeture de session n'est jamais bloquée.

---

## Cross-session recall

**Fichier** : `crates/apollia-runtime/src/chat/manager.rs`

### Mécanisme

Au premier message d'une nouvelle session, `ChatSessionManager::build_cross_session_context()` recherche les sessions passées pertinentes :

1. **Filtre** : Le message doit contenir au moins 20 caractères (les salutations triviales sont ignorées).
2. **Recherche** : FTS5 sur la table `chat_sessions_fts` (index des résumés de sessions passées), max 3 résultats.
3. **Format** : Un bloc est injecté dans le contexte :

```text
## Previous conversations (for reference)
- [2026-03-20T10:00:00Z] Discussion sur la migration des données et le batch processing.
- [2026-03-18T14:30:00Z] Choix de l'architecture pipeline pour le projet X.
```

### Stockage des résumés

Les résumés de sessions sont stockés dans le champ `summary` de la table `chat_sessions` et indexés via la table virtuelle FTS5 `chat_sessions_fts` (colonnes : `session_id UNINDEXED`, `created_at UNINDEXED`, `summary`).

---

## Gestion de la fenêtre de contexte

**Fichiers** : `crates/apollia-runtime/src/chat/builtin_agent.rs`, `crates/apollia-runtime/src/chat/summarizer.rs`

### Sliding window

La fonction `build_llm_messages()` applique une fenêtre glissante sur l'historique des messages. Seuls les `context_window_size` derniers messages sont envoyés au LLM (défaut : 20).

### Summarization

Quand l'historique dépasse la taille de la fenêtre et qu'aucun résumé n'existe encore :

1. `summarize(messages, llm)` produit un résumé de 2-3 paragraphes (max 500 tokens).
2. Le résumé est persisté dans la session pour les échanges futurs.
3. Il est injecté comme message système : `"Previous context summary:\n{summary_text}"`.

Le prompt de summarization demande au LLM de se concentrer sur :
- Les décisions prises
- Le contexte établi
- Les questions non résolues

Les salutations et le small talk sont exclus.

**Décision architecturale** : [ADR-039 — Conversation memory management](adr/ADR-039-conversation-memory-management.md)

---

## ctx.user_context (agents Python)

**Fichier** : `crates/apollia-aip/src/context.rs`

En mode chat, le `RuntimeContext` Python expose une propriété `ctx.user_context` :

```python
# Type : dict[str, list[tuple[str, str]]] | None
user_ctx = ctx.user_context

if user_ctx is not None:
    for key, value in user_ctx.get("preferences", []):
        print(f"Preference: {key} = {value}")
    for key, value in user_ctx.get("habits", []):
        print(f"Habit: {key} = {value}")
    for key, value in user_ctx.get("context", []):
        print(f"Context: {key} = {value}")
```

- **En mode chat** : Contient les catégories `preferences`, `habits`, `context` avec les paires clé-valeur.
- **En mode task** : Vaut `None`.
- **Principe #6** : L'agent décide quoi faire du contexte — ce n'est jamais déterministe.

---

## Scores de confiance

Chaque entrée mémoire est associée à un score de confiance (`confidence`, flottant 0.0–1.0) qui reflète la fiabilité de l'information. Le score est utilisé pour arbitrer les mises à jour : une nouvelle valeur ne remplace pas une valeur existante avec un score de confiance strictement supérieur.

| Score | Signification | Source typique |
|---|---|---|
| `0.95` | Validée par l'utilisateur (feedback loop UI) | `user_explicit` |
| `0.9` | Déclarée explicitement pendant l'onboarding | `onboarding` |
| `0.5` | Inférée du contexte (conversation ou observation) | `chat_inference`, `agent_observation` |

**Méthodes liées** : `store_with_confidence()`, `update_confidence()` dans `UserMemoryRepository`.

---

## Onboarding conversationnel

L'onboarding est le principal vecteur de peuplement initial de la mémoire utilisateur. Implémenté comme un agent conversationnel standard (`onboarding-agent`), il explore 5 domaines et persiste chaque information en temps réel avec un score de confiance adapté (0.9 pour les déclarations explicites, 0.5 pour les déductions).

Pour le détail complet du fonctionnement, des clés mémoire, et de l'utilisation : voir [Guide Onboarding](Agents-Onboarding-Guide.md).

**Méthodes liées** dans `UserMemoryRepository` :
- `get_covered_topics()` — Domaines couverts par l'onboarding
- `mark_topic_covered(topic)` — Marque un domaine comme couvert
- `get_onboarding_skipped()` / `set_onboarding_skipped(skipped)` — État "Plus tard"
- `get_last_onboarding_session()` / `set_last_onboarding_session(timestamp)` — Dernière session

---

## Enrichissement passif

**Fichier** : `crates/apollia-runtime/src/chat/extractor.rs`

En plus de l'extraction post-session déjà documentée (section "Extraction LLM"), le système supporte un enrichissement passif continu. Le `UserMemoryExtractor` est un composant stateful qui :

1. **Rate-limite** les extractions (cooldown de 1 heure entre sessions).
2. **Déduplique** : ignore les entrées dont la valeur est identique à l'existante.
3. **Respecte la confiance** : ne remplace jamais une entrée avec un score supérieur (les données d'onboarding à 0.9 ne sont pas écrasées par l'extraction passive à 0.5).
4. **Enrichit progressivement** : les nouvelles clés découvertes sont ajoutées, les clés existantes avec un score égal ou inférieur sont mises à jour.

Le seuil minimal est de 6 messages pour l'enrichissement passif (vs 4 pour l'extraction standard).

---

## Feedback loop UI

L'utilisateur peut gérer ses entrées mémoire depuis l'interface desktop (Settings > Mes Mémoires) :

- **Valider** une entrée — augmente le score de confiance à 0.95 (`update_confidence()`)
- **Corriger** une entrée — modifie la valeur via `update()`, source mise à `user_explicit`
- **Supprimer** une entrée — suppression immédiate et définitive via `forget()`

Ce mécanisme ferme la boucle : les informations inférées (confiance 0.5) peuvent être validées par l'utilisateur pour devenir des données de confiance élevée (0.95).

---

## Diagrammes

- [seq-chat-user-memory.puml](diagrams/seq-chat-user-memory.puml) — Injection de la mémoire utilisateur dans le chat
- [seq-conversation-summarize.puml](diagrams/seq-conversation-summarize.puml) — Flux de summarization des conversations
- [seq-chat-libre.puml](diagrams/seq-chat-libre.puml) — Chat libre complet (mis à jour avec user memory et summary)
- [seq-onboarding-flow.puml](diagrams/seq-onboarding-flow.puml) — Flux d'onboarding complet

---

## Liens

- [Guide Onboarding](Agents-Onboarding-Guide.md)
- [ADR-040 — Onboarding comme agent conversationnel](adr/ADR-040-onboarding-conversational-agent.md)
- [Brique — CLI](Briques-CLI.md) — Commande `apollia-os onboard`
