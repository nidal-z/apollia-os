# ADR-087 - Profil utilisateur canonique avec schéma déclaratif

**Date :** 2026-05-11
**Statut :** Accepté
**Sprint :** Phase release v0.1.0 (PLAN-13-JOURS)

---

## Contexte

La mémoire utilisateur globale (namespace SQLite `__user__`) est sur-dimensionnée
par rapport à son usage réel et fragmente la personnalisation en multiples surfaces
incohérentes.

**État observé au 2026-05-11 :**

- L'onboarding-agent v2.2 écrit **4 clés Tier 1** (`user.name`, `user.role`,
  `user.agents.hitl`, `user.constraints.sovereignty`). Aucun autre flux ne peuple
  `__user__` en production.
- **3 agents** consomment ces clés (`veille-ia`, `markdown-summarizer`,
  `code-review-multi`) via `ctx.memory.recall("user.X")` + un helper
  `_load_user_context()` qui hardcode la liste des clés à lire - la convention
  `user.*` n'est documentée nulle part de manière centrale.
- Le backend Rust expose pourtant un modèle complexe :
  `UserMemoryCategory { Profile, Preferences, Habits, Context }` (la variante
  `Profile` est code mort, exclue de `VALID_CATEGORIES` côté IPC),
  `UserMemorySource { Onboarding, ChatInference, UserExplicit, AgentObservation }`,
  un score `confidence: f64`, un badge `validated`, un champ `expires_at` jamais
  utilisé.
- L'UI desktop expose **deux entrées d'édition concurrentes** :
  - `routes/Memory.svelte` tab `user_memory` avec `UserMemoryDashboard.svelte`
    (3 chips Préférences/Habitudes/Contexte, confidence bars, source badges,
    validated badges) - labels différenciés operator/builder pour un contenu
    identique.
  - `routes/settings/Profile.svelte` *déjà* form-based, qui ré-implémente un
    schéma déclaratif (`KEY_CATEGORY` + `SENSITIVE_KEYS`) en TypeScript.
- L'onboarding-agent écrit dans **deux namespaces** (`__user__` pour `user.*` et
  `onboarding-agent` pour `onboarding.*`). Cette dichotomie est légitime (l'un est
  le profil canonique, l'autre est la trace épisodique du dialogue), mais
  insuffisamment matérialisée dans le code et la doc.

La friction n'est plus tenable : trop de surface API pour quelques dizaines
d'entrées, deux UIs concurrentes, pas de schéma central, et un mode operator vs
builder qui ne change que des étiquettes.

## Décision

**Nous adoptons un profil utilisateur canonique unique, déclaré dans un schéma
Rust central (`crates/apollia-memory/src/profile_schema.rs`), exposé via
`ctx.profile.*` au SDK Python, et édité depuis une seule page
`Paramètres → Profil` côté UI. Comme Apollia OS n'a pas encore d'utilisateurs
en production, aucun code legacy / rétrocompat / migration n'est conservé -
c'est la V1 du profil.**

Concrètement :

1. **Schéma déclaratif central** - une const `PROFILE_SCHEMA: &[ProfileField]`
   liste les ~15 champs canoniques (Tier 1 + Tier 2), regroupés en 4 sections
   d'affichage (`Identity`, `Work`, `Preferences`, `Constraints`), avec un flag
   `sensitive: bool` qui déclenche un avertissement "relance l'onboarding".
2. **Storage** - la table `semantic_memories` namespace `__user__` est
   conservée (aucun changement de schéma SQL). Les clés sont **plates** -
   pas de préfixe `user.`, pas de préfixe catégorie. Les états internes
   d'onboarding utilisent un préfixe `__` qui les masque automatiquement du
   listing profil (`get_internal`/`set_internal`).
3. **API SDK Python `ctx.profile`** - propriétés `.name`/`.role`/...,
   méthodes `.get(key)`/`.has(key)`/`.all()`/`.set(key, value)`/`.update(dict)`.
   Gating manifeste `user_memory_write: true` pour l'écriture, lecture
   inconditionnelle.
4. **Provenance simplifiée** - `WrittenBy { Onboarding, User, Agent(name) }`
   (3 variantes). Plus de score `confidence` exposé, plus de badge `validated`
   - la confiance reste à 1.0 en colonne SQL.
5. **UI unique** - `Paramètres → Profil` (`routes/settings/Profile.svelte`)
   est la seule surface d'édition. Le tab `user_memory` de
   `routes/Memory.svelte` est supprimé (avec `UserMemoryDashboard.svelte` et
   `MemoryRow.svelte`). La page Mémoire conserve uniquement le namespace
   explorer pour le debug.
6. **Suppression des anciennes API** - `MemoryInterface.remember_user`,
   `recall("user.X")` fallback vers `__user__`, `UserMemoryCategory`,
   `UserMemorySource`, `UserMemoryEntry`, `store(category, …)`,
   `recall(category, limit)`, `recall_by_key`, `update_confidence`,
   `store_with_confidence`, `validate_user_memory` IPC, routes HTTP
   `/api/v1/user/*`, commandes Tauri `commands::user::*` : **supprimés**.
   ADR-038 (mémoire user globale) reste valide, **amendé** par cet ADR.

## Alternatives considérées

### Option A - Profil JSON unique typé (rejetée)
**Pour :** typage strict côté Rust (struct `UserProfile` sérialisée en blob),
  auto-complétion IDE pour les agents.
**Contre :** rupture totale du contrat actuel (plus de fallback `recall("user.X")`
  sans wrapper lourd), aucune extensibilité par les agents (chaque nouveau champ
  requiert recompilation), migration mapping plus complexe. Effort ~6j.

### Option B - Profil typé + zone "notes libres" séparée (rejetée)
**Pour :** sépare le canonique du libre, scalable.
**Contre :** réintroduit deux concepts (`ctx.profile.*` + `ctx.user_notes.*`),
  plus de surface API à maintenir, plus de complexité UI (2 sections). Effort ~5-6j.

### Option retenue - Profil canonique + schéma déclaratif central (Piste B du plan)
**Pour :** schéma déclaratif central résout la convention floue actuelle, UI
  cible déjà préparée (`settings/Profile.svelte` existe), aucun ADR à
  superseder, surface API drastiquement réduite. Apollia OS n'ayant pas
  encore d'utilisateurs en production, on en profite pour livrer la V1
  sans dette legacy (pas de migration, pas de rétrocompat, pas de wrappers
  dépréciés). Effort ~4 j.
**Compromis acceptés :** ajouter un champ canonique nécessite une recompilation
  Rust (les agents peuvent toujours écrire en clé libre - visibles dans la
  section "Autres entrées" - mais la promotion en champ canonique reste un acte
  explicite). Tout client qui dépendait des anciennes API (`remember_user`,
  `recall("user.X")`, routes HTTP `/api/v1/user/*`, etc.) doit migrer vers
  `ctx.profile.*`.

## Conséquences

**Positives :**
- Une seule source de vérité pour les clés du profil (le schéma Rust).
- Une seule surface d'édition utilisateur (`Paramètres → Profil`).
- Surface API réduite : 3 commandes Tauri ajoutées, 2 supprimées
  (`validate_user_memory`, `VALID_CATEGORIES`), 5 marquées deprecated.
- Frottement cognitif divisé pour les agents builders : `ctx.profile.name` est
  plus parlant que `ctx.memory.recall("user.name")`.
- L'onboarding-agent ne dépend plus de la convention `user.*` (elle survit en
  rétrocompat mais n'est plus la voie canonique).
- Mode UI operator vs builder retrouve un sens (label = même contenu = supprimé
  pour le profil ; conservé pour d'autres surfaces où il est légitime).

**Négatives / Compromis :**
- L'ajout d'un champ Tier 3 nécessite : update `PROFILE_SCHEMA` Rust + i18n
  labels + (optionnel) extension onboarding-agent. Compense partiellement la
  flexibilité libre actuelle.
- Confidence et validated badge disparaissent de l'UI - si un besoin de
  scoring ré-émerge, il faudra réintroduire un mécanisme (la colonne SQL reste,
  donc rétro-faisable).
- `_load_user_context()` dans les 3 agents consommateurs reste sur l'ancienne
  API par rétrocompat - refactor vers `ctx.profile.all()` est optionnel
  post-release.

**À surveiller :**
- Adoption de `ctx.profile.*` par les futurs agents (sprint post-launch).
- Émergence d'un besoin de scoring/validation (réintroduction
  optionnelle d'un workflow `validated` côté UI builder).
- Croissance du schéma : si on dépasse ~30 champs, envisager un mécanisme
  d'enregistrement déclaratif par fichier TOML chargé au boot (pas pour v0.1.0).

## Principes architecturaux impactés

- **Principe #6 - Mémoire à initiative de l'agent** : conservé. Aucune injection
  automatique. `ctx.profile` est consulté explicitement par l'agent quand il en
  a besoin (comme `ctx.memory.recall` aujourd'hui).
- **Principe #3 - Contrat minimal** : renforcé. Une API typée et explicite
  (`ctx.profile.name`) remplace une convention floue (`recall("user.name")`).
- **Principe #8 - CLI humaine, API machine** : l'UX est simplifiée
  (`Paramètres → Profil` = formulaire form-based humain) et l'API agent est
  plus expressive.

## Liens

- ADR-007 - Mémoire à initiative de l'agent (préservé)
- ADR-038 - Mémoire utilisateur globale (**amendé** par cet ADR - fallback
  `recall("user.X")` conservé, le reste évolue)
- ADR-040 - Onboarding comme agent conversationnel (l'agent migre vers
  `ctx.profile.set(...)`)
- ADR-086 - Permissions agent-driven : `governance.db` source unique
  (l'onboarding-agent lit toujours le profil pour proposer ses règles -
  via `ctx.profile.*` ou `recall("user.*")` rétrocompat)
- Plan d'implémentation : `~/.claude/plans/j-ai-besoin-de-repenser-expressive-cat.md`
