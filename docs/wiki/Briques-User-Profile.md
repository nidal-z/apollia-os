# Brique - Profil utilisateur (User Profile)

> **Principe fondamental** : le profil utilisateur est un **outil**, pas une règle.
> Le LLM reçoit le contexte profil comme information dans le system prompt.
> Il **DÉCIDE** quoi en faire. Ce n'est **JAMAIS** un filtre déterministe qui modifie le comportement du runtime (Principe #6 - mémoire à initiative de l'agent).

**Décisions architecturales** :
- [ADR-007 - Mémoire à initiative de l'agent](../adr/ADR-007-memoire-initiative-agent.md)
- [ADR-038 - Mémoire utilisateur globale](../adr/ADR-038-global-user-memory.md) (amendée par ADR-087)
- [ADR-040 - Onboarding comme agent conversationnel](../adr/ADR-040-onboarding-conversational-agent.md)
- [ADR-087 - Profil utilisateur canonique avec schéma déclaratif](../adr/ADR-087-user-profile-redesign.md)

---

## Vue d'ensemble

Le profil utilisateur global permet à Apollia OS de conserver des informations persistantes sur l'opérateur entre les sessions de chat et les exécutions d'agents. Ces informations enrichissent le contexte des interactions sans jamais contraindre le comportement du LLM.

Le sous-système repose sur trois piliers :

1. **Schéma canonique** - une const Rust `PROFILE_SCHEMA` déclare la liste des champs canoniques (Tier 1 + Tier 2) regroupés en quatre sections d'affichage.
2. **Stockage** - `UserMemoryRepository` persiste les entrées dans SQLite via `SemanticMemory` sous le namespace réservé `__user__`, avec des **clés plates** (pas de préfixe).
3. **Exposition** - le SDK Python expose `ctx.profile` ; l'UI desktop expose `Paramètres → Profil` ; l'onboarding-agent peuple le Tier 1 lors du premier lancement.

---

## Architecture

### Namespace `__user__`

Toutes les entrées de profil vivent sous le namespace réservé `__user__` dans le `SemanticMemory` (crate `apollia-memory`). Ce namespace est distinct des namespaces agents et n'est jamais accessible en écriture par les agents non gatés.

```
┌─────────────────────────────────────────┐
│           MemoryStore (SQLite)          │
├───────────────┬─────────────────────────┤
│ __user__      │  Profil utilisateur     │
│ <agent_name>  │  Mémoire agent          │
│ <proj>:<ns>   │  Mémoire scoped projet  │
└───────────────┴─────────────────────────┘
```

Les **états internes** de l'onboarding et du desktop (UI bookkeeping) vivent aussi dans `__user__` mais sous des clés préfixées `__` (ex. `__onboarding_skipped`, `__companion_enabled`) - masquées de toutes les listes utilisateur.

### Schéma canonique

**Fichier** : `crates/apollia-memory/src/profile_schema.rs`

Le schéma est compilé dans le binaire. Ajouter un champ requiert une recompilation. Le schéma déclare 15 champs Tier 1 + Tier 2 répartis en 4 sections UI :

| Section | Champs |
|---|---|
| `identity` | `name`, `role`, `goals` |
| `work` | `domain.sector`, `domain.team_size`, `tech.stack`, `tech.proficiency`, `tools.daily`, `tech.integrations` (sensible) |
| `preferences` | `preferences.language`, `preferences.llm`, `agents.hitl` (sensible), `agents.domains`, `agents.trigger` |
| `constraints` | `constraints.sovereignty` (sensible), `constraints.compliance` (sensible) |

Quatre champs portent le flag `sensitive: true` : `agents.hitl`, `tech.integrations`, `constraints.sovereignty`, `constraints.compliance`. Leur modification depuis l'UI affiche un avertissement *"relance l'onboarding pour propager les permissions"* car le runtime **n'auto-rederive jamais** les règles `governance.db` (Principe #6).

Chaque entrée de schéma porte : `key`, `label_fr`, `label_en`, `help_fr`, `help_en`, `section`, `sensitive`, `field_type` (`Text` | `LongText` | `Select`), `options` (pour `Select`).

### UserMemoryRepository

**Fichier** : `crates/apollia-memory/src/user_memory.rs`

Le repository est le point d'entrée unique pour toutes les opérations sur le profil utilisateur. Il délègue le stockage à `SemanticMemory` en utilisant des clés plates.

```rust
pub struct UserMemoryRepository { store: MemoryStore }
```

**API publique** :

| Méthode | Description |
|---|---|
| `new(db_path)` | Ouvre ou crée la base `__user__.db` |
| `set(key, value, written_by)` | Upsert d'une entrée canonique ou extras |
| `get(key)` | Lecture d'une entrée |
| `list_all()` | Liste toutes les entrées visibles (schema + extras), ordonnées schéma puis alpha |
| `list_schema()` | Sous-ensemble des entrées correspondant à `PROFILE_SCHEMA` |
| `list_extras()` | Entrées hors schéma |
| `update(key, value)` | Met à jour la valeur en préservant `written_by` |
| `forget(key)` | Supprime une entrée |
| `reset()` | Purge toutes les entrées visibles, préserve les états internes |
| `search(query, limit)` | FTS5 BM25 |
| `recall_all_for_injection(max)` | Texte structuré pour system prompt LLM (groupé par section) |
| `recall_persona_brief(max)` | Brief persona en français pour system prompt |
| `is_empty()` | True si aucune entrée visible |
| `mark_topic_covered(topic)` / `get_covered_topics()` | Suivi onboarding |
| `get_onboarding_skipped()` / `set_onboarding_skipped(bool)` | Marqueur skip |
| `get_last_onboarding_session()` / `set_last_onboarding_session(ts)` | Timestamp dernière session |
| `get_internal(key)` / `set_internal(key, value)` | Helpers génériques pour états internes (UI desktop, etc.) |

### Provenance - `WrittenBy`

Trois variantes (`WrittenBy { Onboarding, User, Agent(String) }`) sérialisées dans la colonne `source` de `semantic_memories` :

| Tag stocké | Origine |
|---|---|
| `onboarding` | Écriture par l'onboarding-agent |
| `user` | Édition explicite depuis Settings → Profil ou IPC opérateur |
| `agent:<name>` | Observation d'un agent en cours de tâche (ex. `agent:chat-extractor`) |

Pas de score `confidence` exposé, pas de badge `validated`, pas d'enum `category` - supprimés en V1 (ADR-087).

### IPC Tauri

**Fichier** : `crates/apollia-desktop/src/commands/user_memory.rs`

| Commande | Signature | Retour |
|---|---|---|
| `get_profile_schema` | `()` | `Vec<ProfileFieldView>` |
| `get_profile` | `()` | `UserProfileView { schema_entries, extras, entries, last_updated_at }` |
| `set_profile_entry` | `request: { key, value }` | `ProfileEntryView` (forcé `WrittenBy::User`) |
| `delete_profile_entry` | `key: String` | `()` |
| `reset_user_profile` | `()` | `usize` (nombre d'entrées supprimées) |
| `get_conversation_stats` | `session_id: String` | `ConversationStatsView` (pour SidePanel chat) |

### SDK Python - `ctx.profile`

**Bridge Rust** : `crates/apollia-aip/src/profile.rs`  
**Stub Python** : `sdk/apollia/stubs/profile.py`

```python
# Lecture (toujours autorisée - namespace __user__ partagé)
await ctx.profile.get("role")          # str | None
await ctx.profile.has("agents.hitl")    # bool
await ctx.profile.all()                  # dict[str, str] (clés plates)
ctx.profile.schema_keys()                # list[str] - canonical keys (sync)

# Écriture - requiert user_memory_write = true dans le manifest
await ctx.profile.set("role", "CTO fintech")
await ctx.profile.update({"name": "Nidal", "preferences.language": "fr"})
```

Le préfixe legacy `user.` est silencieusement stripé sur `get` et `set` : `ctx.profile.get("user.role")` ≡ `ctx.profile.get("role")`. Préférer la clé plate dans le nouveau code.

Sans la permission manifest, `set`/`update` lèvent `RuntimeError`. La permission est **réservée à l'onboarding-agent** dans la V1 - les autres agents observent passivement et n'écrivent qu'avec une autorisation explicite.

### Injection en chat mode

En chat mode, `BuiltInChatAgent` injecte un snapshot du profil dans le system prompt via `recall_all_for_injection` ou `recall_persona_brief`. Le LLM reçoit cette information comme **un outil**, jamais comme une contrainte. Cf. ADR-007 / ADR-038.

L'injection est marquée dans le prompt par un en-tête `Section: <name>` (ex. `Section: identity`). Le compteur `user_memory_injected` côté `ConversationStatsView` détecte ces marqueurs.

---

## Onboarding-agent - point d'entrée canonique

**Fichier** : `agents/system/onboarding-agent/agent.py`

L'onboarding-agent est le **seul agent système** déclarant `user_memory_write = true`. Il peuple le Tier 1 en quatre tours :

| Tour | Clés écrites | `WrittenBy` |
|---|---|---|
| 1 - Identité | `name`, `role` | `Onboarding` |
| 2 - Supervision | `agents.hitl` | `Onboarding` |
| 3 - Contraintes | `constraints.sovereignty` | `Onboarding` |
| 4 - Proposition de règles | n/a (propose des règles `governance.db`) | - |

Les états internes du dialogue (`bootstrap.snapshot`, topics couverts, `onboarding.completed_at`) restent dans la **namespace propre** de l'agent (`onboarding-agent`), distincte de `__user__`. Les marqueurs UI desktop (`__onboarding_phase`, `__onboarding_skipped`, etc.) vivent eux dans `__user__` sous préfixe `__`.

---

## UI desktop

**Page d'édition unique** : `Paramètres → Profil`
(`crates/apollia-desktop/ui/src/routes/settings/Profile.svelte`)

- Formulaire form-based, autosave au blur de chaque champ.
- Cards par section (Identity / Work / Preferences / Constraints).
- Badge "vous" / "onboarding" / "agent" à côté de chaque champ.
- Avertissement *"sensible"* sur les 4 champs gatés.
- Bouton **Réinitialiser le profil** (zone danger) qui invoque `reset_user_profile` puis `trigger_onboarding`.

**Page Mémoire** (`routes/Memory.svelte`) :
- Explorateur des namespaces, sidebar classifiée (Profil / Agents / Projets / Autres).
- Le namespace `__user__` est **navigable en lecture seule** ; une bannière redirige vers `Paramètres → Profil` pour l'édition.

---

## Tests

- `crates/apollia-memory/src/user_memory.rs` - CRUD, partition schema/extras, reset, persona brief, validation de clés (tests inline).
- `crates/apollia-memory/src/profile_schema.rs` - invariants (clés uniques, options de `Select`, présence Tier 1).
- `crates/apollia-aip/src/memory.rs` - round-trip mémoire agent isolée (sans fallback `__user__`).
- `crates/apollia-desktop/src/commands/user_memory.rs` - round-trip view types.
- UI : Vitest sur `companion.test.ts` (utilise `get_companion_enabled`).

---

## Aide opérateur

- [Mon profil](../help/memoire/mon-profil.md) - éditer son profil depuis Settings.
- [Consulter et nettoyer la mémoire](../help/memoire/consulter-et-nettoyer-la-memoire.md) - explorer les namespaces et supprimer des entrées.
