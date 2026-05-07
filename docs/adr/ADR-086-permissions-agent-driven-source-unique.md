# ADR-086 — Permissions agent-driven : `governance.db` comme source unique

**Date :** 2026-04-29
**Statut :** Accepté
**Sprint :** Phase release v0.1.0

---

## Contexte

Une story d'implémentation prévoyait un **derivation engine** côté Rust : à la fin
de l'onboarding (et à chaque modification d'une clé profil dérivable depuis Settings),
un module Rust devait lire trois clés mémoire utilisateur —
`user.constraints.sovereignty`, `user.agents.hitl`, `user.tech.integrations` — et les
mapper automatiquement vers des règles dans `governance.db` selon une table de
correspondance figée :

```
local-strict     → deny   http_request https://
critical-only    → approval email_send / file_delete / http_post
hitl=never       → allow  tool_name="*"
github           → allow  github_*
...
```

Trois problèmes ont fait converger l'analyse vers un changement d'approche :

1. **Violation du principe #6.** Apollia garantit que la mémoire utilisateur n'est
   jamais transformée *silencieusement* en effet runtime ("mémoire à initiative de
   l'agent, jamais d'injection automatique"). Un derivation engine qui crée des règles
   de permissions sans décision humaine ou agentique explicite contredit ce principe.

2. **Incompatibilité technique réelle avec le moteur existant.** `RuleAction`
   (`crates/apollia-permissions/src/prefix_rule_engine.rs:35-41`) ne supporte que
   `Allow` et `Deny` — la spec demande une 3ᵉ variante `Approval`. Le lookup utilise
   `WHERE tool_name = ?` exact (`prefix_rule_engine.rs:235`) — la spec demande un
   wildcard `tool_name="*"`. Les deux extensions impacteraient le moteur, l'audit log,
   tous les sites d'usage de `RuleAction`, et nécessiteraient une migration de schéma.
   Tout ce chantier pour automatiser un mapping qui n'est pas demandé par les
   utilisateurs.

3. **Conflit avec la value proposition.** Apollia se différencie par des agents ReAct
   autonomes capables de **décider** comment agir. Un mapping `mémoire → règles`
   hardcodé en Rust est exactement le genre de pipeline déterministe qu'Apollia vient
   d'écarter ailleurs (cf. ADR-085 — pipeline engine TOML supprimé). Faire décider
   l'onboarding-agent quelles règles proposer (en lisant la mémoire et en conversant
   avec l'utilisateur) est cohérent avec ce qu'Apollia veut être.

À cela s'ajoute une dette pré-existante : aujourd'hui `governance.db` n'est **pas**
l'unique source de décision runtime. La couche 1 du `PermissionEngine`
(`engine.rs:175-182`) consulte une `SafeList` chargée au boot depuis
`PermissionsConfig.safe_commands` (TOML opérateur). C'est un 2ᵉ producteur silencieux,
non visible dans l'UI gouvernance, non audité comme une règle.

## Décision

**Nous adoptons un modèle agent-driven pour les permissions, avec
`~/.apollia/governance.db` comme source de vérité unique en lecture runtime.**

Concrètement :

1. **Aucun derivation engine n'est implémenté.** Le mapping
   `mémoire profil → règles` n'existe pas en code Rust.
2. **Trois nouveaux outils natifs** — `permission_rule_add`, `permission_rule_remove`,
   `permission_rule_list` — sont exposés à tout agent qui veut proposer ou inspecter
   des règles. Les écritures (`add`/`remove`) sont systématiquement HITL-gated selon
   ADR-082.
3. **L'onboarding-agent reçoit `add` + `list`** dans son manifest (pas `remove`,
   moindre privilège). Son system prompt est étendu pour qu'il propose explicitement
   les règles correspondant aux préférences collectées, en mentionnant à l'utilisateur
   chaque appel qu'il s'apprête à faire. L'utilisateur valide via le dialogue HITL
   standard ; le bouton « toujours accepter pour cette session » couvre les séries.
4. **Le champ `created_by` (déjà présent dans le schéma `permission_rules` —
   `prefix_rule_engine.rs:624`) devient obligatoire en pratique** :
   `onboarding-agent`, `user-hitl`, `user-settings`, `config-import` selon le
   producteur. L'UI Settings et la CLI affichent cette colonne pour l'audit.
5. **La `SafeList` du TOML est ingérée dans `governance.db` au démarrage du
   `PermissionEngine`** avec `created_by="config-import"`, scope `global`,
   `RuleAction::Allow`. L'opération est idempotente (marqueur de présence). La couche
   1 SafeList runtime reste branchée 1-2 sprints le temps de valider zéro régression,
   puis sera supprimée dans une PR dédiée.
6. **Settings > Profil** affiche une bannière *"Ton profil a changé. Veux-tu que
   l'onboarding-agent ajuste les permissions ?"* qui relance une session onboarding
   ciblée (`trigger_onboarding(topic="permissions")`). Pas de re-derivation
   automatique.

## Alternatives considérées

### Option A — Derivation engine pur (rejetée)

Implémenter la spec initiale telle quelle : table de mapping en Rust, hook sur
`onboarding.completed_at`, suppression des règles `created_by="onboarding-agent"`
puis recréation à chaque modif profil.

**Pour :** déterministe, latence nulle (pas de tour LLM).
**Contre :** viole principe #6 ; nécessite étendre `RuleAction` (variante `Approval`)
et ajouter wildcard `tool_name="*"` dans le lookup ; transforme la mémoire utilisateur
en effet de bord runtime invisible ; opposé à la philosophie ReAct.

### Option B — Derivation engine + extension du moteur de permissions (rejetée)

Comme A mais en assumant le coût de l'extension : ajouter `RuleAction::Approval`,
adapter `engine.rs::decide()` pour mapper Approval → NeedsApproval, ajouter wildcard
match `OR tool_name = '*'` dans la query, mettre à jour audit log + UI gouvernance.

**Pour :** spec littéralement implémentée, sémantique d'`approval` visible en DB.
**Contre :** chantier important (~3 fichiers crate permissions + sites d'usage UI/CLI)
pour automatiser un comportement que les utilisateurs n'ont pas demandé ; ajoute une
3ᵉ variante d'enum dont la valeur n'est utilisée que par le derivation engine ;
n'adresse pas la violation du principe #6.

### Option C — Garder SafeList TOML en parallèle de governance.db (rejetée)

Ne rien migrer. Conserver la couche 1 SafeList comme escape-hatch headless.

**Pour :** zéro touche au boot path, retro-compat maximale.
**Contre :** maintient deux producteurs de décision runtime, aucun visible dans l'UI
gouvernance, complique le mental model "source unique" qui motive cet ADR.

### Option retenue — Permissions agent-driven + migration SafeList

**Pour :**
- Aligne sur principes #6 et #7 (HITL non contournable).
- N'exige aucune extension du moteur de permissions existant.
- Source de vérité **réellement** unique en lecture runtime.
- Cohérent avec ADR-085 (suppression pipeline TOML déterministe) et la value prop
  ReAct.
- Réutilisable : tout agent (pas que onboarding) peut ajuster les permissions via le
  même chemin.
- Traçabilité native via `created_by` déjà présent au schéma.

**Compromis acceptés :**
- Latence : changer `sovereignty` dans Settings → conversation avec onboarding-agent
  → outils → règles. Acceptable car ces changements sont rares (mensuels, pas par
  seconde).
- Non-déterminisme : l'agent peut formuler les règles différemment d'une session à
  l'autre. Mitigé par un system prompt précis et des tests E2E sémantiques.
- Nécessite 3 nouveaux outils natifs `permission_rule_*` — mais ce sont des briques
  de gouvernance légitimes, pas un hack.

## Conséquences

**Positives :**
- `governance.db` devient l'unique source de décision runtime
  (`PermissionEngine.decide()` ne lit que cette table + injection detector hardcodé +
  session rules RAM).
- Toute règle est traçable à son auteur via `created_by` ; UI Settings filtre par ce
  champ pour audit.
- Pattern réutilisable au-delà de l'onboarding : un agent métier peut proposer des
  règles spécifiques à son domaine (ex : `veille-rse` propose `allow http_fetch
  https://www.iso.org/`) avec confirmation HITL.
- L'UI gouvernance, le HITL dialog, la CLI et les agents convergent sur le même API
  d'écriture : `PrefixRuleEngine::add_rule()` (Rust) ou `permission_rule_add` (agent).

**Négatives / Compromis :**
- L'onboarding "first run" implique N dialogues HITL d'affilée si l'utilisateur ne
  clique pas "toujours accepter pour cette session". Un travail UX léger
  (regroupement visuel, message agent contextuel) est attendu en sprint UI.
- Non-déterminisme assumé : couvrir par tests sémantiques (présence d'au moins une
  règle attendue) plutôt que stricts (égalité exacte).

**À surveiller :**
- Suppression effective de la couche 1 SafeList runtime dans `engine.rs:175-182`
  après 1-2 sprints de stabilité de la migration.
- Migration rétroactive de `created_by` pour les règles existantes (NULL aujourd'hui)
  via éventuel `apollia permissions backfill-creator` — ADR séparé si besoin.
- Risque que d'autres agents abusent de `permission_rule_add` pour s'auto-élever de
  privilèges. Mitigé par le HITL systématique + `created_by` qui rend l'abus
  immédiatement traçable.

## Principes architecturaux impactés

- **Principe #6 — Mémoire à initiative de l'agent** : renforcé. La mémoire profil
  n'est plus un input runtime mais un input conversationnel pour l'agent qui propose,
  l'humain confirme.
- **Principe #7 — Garde-fous non-négociables** : renforcé. Toute règle nouvellement
  créée passe par HITL, sans bypass possible côté agent.
- **Principe #8 — CLI humaine, API machine** : `apollia permissions list` affiche
  désormais `created_by` pour audit machine et humain.

## Liens

- ADR-082 — Tool Governance : DB unifiée, scopes HITL, ToolRegistry runtime
  (établit les fondations de gouvernance que ce présent ADR exploite).
- ADR-085 — Pipeline engine TOML supprimé de v0.1.0 (même mouvement de fond :
  abandon des pipelines déterministes au profit du raisonnement agentique).
- Story onboarding (v2.0) : `agents/system/onboarding-agent/agent.py` — agent
  conversationnel qui collecte les préférences profil. Cet ADR étend son rôle au
  versement explicite des préférences en règles de permissions.

## Annexe — Matrice des règles proposées par l'onboarding-agent (post-2026-05-06)

L'agent dérive ses propositions de `_propose_permission_rules` selon trois
dimensions du profil collecté. Toutes les règles sont créées avec
`created_by="onboarding-agent"`, scope `global` par défaut, et chaque appel
`permission_rule_add` traverse la couche HITL desktop (ApprovalCardV2).

### Souveraineté → contrôle réseau sortant

| `user.constraints.sovereignty` | Règles proposées |
|---|---|
| `local-only` | `deny http_fetch https://`, `deny http_fetch http://` |
| `local-preferred` | `deny http_fetch https://api.openai.com`, `deny http_fetch https://api.anthropic.com` |
| `cloud-ok` | aucune règle réseau |

### Niveau HITL → friction sur outils en lecture

| `user.agents.hitl` | Règles proposées |
|---|---|
| `always` | aucune (chaque action sensible déclenche un HITL) |
| `critical-only` ou `never` | `allow file_read` (global) + `allow shell_exec` sur `ls`, `cat`, `grep`, `pwd`, `head`, `tail` |

### Intégrations explicitement activées

`user.tools.integrations` (liste séparée par virgules, casse insensible) →
`allow http_fetch` sur l'API correspondante :

| Valeur reconnue | `arg_prefix` autorisé |
|---|---|
| `github` | `https://api.github.com` |
| `slack`  | `https://slack.com/api/` |
| `notion` | `https://api.notion.com` |
| `gmail`  | `https://gmail.googleapis.com` |

### Idempotence

Avant toute proposition, l'agent appelle
`permission_rule_list(created_by="onboarding-agent")`. Si des règles existent
déjà, **aucune** nouvelle carte n'est émise. Pour ré-évaluer le profil après
une modification majeure, l'opérateur doit soit révoquer manuellement les
règles existantes (Settings → Permissions, filtre `Onboarding`), soit relancer
un reset onboarding depuis la Zone de danger.

### Échecs et rejets

Le code remplace l'ancien `try/except: pass` silencieux par un logging
explicite via `logging.getLogger("onboarding-agent")` :

- Rejet HITL utilisateur → `INFO permission_rule_add rejected/failed (...)`.
  La complétion onboarding continue ; la règle est simplement absente.
- Échec technique (DB lock, etc.) → même log, même non-blocage.

`onboarding.completed_at` n'est écrit qu'après la phase de propositions —
même si toutes les cartes sont rejetées, l'onboarding se termine proprement.

### Architecture des cartes d'approbation (révisée 2026-05-06)

L'implémentation initiale prévoyait que l'agent appelle directement
`permission_rule_add` (HITL-gated risk_score=60) pour faire apparaître
chaque carte dans le chat. Validation manuelle a montré que **les cartes
n'apparaissaient jamais** — cause racine identifiée :
`crates/apollia-tools/src/executor.rs:377-401` retourne
`Err(PermissionDenied)` quand la décision est `NeedsApproval`, sans
suspendre l'exécution. L'event `PermissionRequired` est bien émis mais
aucune surface ne l'écoute pour le contexte d'agent conversationnel.

**Nouvelle architecture (post-2026-05-06)** :

1. L'agent **ne** crée plus les règles directement. Il sérialise la liste
   dérivée en JSON dans la clé mémoire **`onboarding.proposed_rules`** via
   la fonction `_persist_proposed_permission_rules` (renommée depuis
   `_propose_permission_rules`).
2. Le desktop lit cette clé via la Tauri command
   `list_proposed_permission_rules` et rend chaque proposition comme un
   mini-card (composant Svelte `OnboardingPermissionStep.svelte`) inline
   dans la fenêtre d'onboarding, **après le chat, avant le wrap-up**.
3. À l'approbation utilisateur, le desktop appelle
   `apply_proposed_permission_rule(index)` qui invoque **directement**
   `PrefixRuleEngine::add_rule(...)` avec `created_by_agent="onboarding-agent"`.
   Bypass complet du tool dispatcher, donc pas de boucle HITL parasite
   — la décision utilisateur est explicite, l'autorité est claire.
4. Au refus, `dismiss_proposed_permission_rule(index)` retire l'entrée de
   la liste persistée sans créer de règle.
5. Le bouton **Terminer** reste désactivé tant que la liste pending
   n'est pas vide.

Cette architecture préserve la promesse du guide utilisateur (« cartes
d'approbation après les questions ») et reste compatible avec
ADR-086 :

- Le `created_by="onboarding-agent"` continue d'être stampé sur chaque
  règle, satisfaisant l'exigence d'audit (cf. §74 ADR original).
- L'idempotence via `permission_rule_list(created_by=...)` est préservée :
  l'agent ne sérialise pas de nouvelles propositions s'il en a déjà émis
  par le passé.
- Aucune dérivation Rust automatique n'est ajoutée : l'agent reste
  l'unique auteur de la liste, le desktop n'est qu'un transport vers
  l'utilisateur.

Voir `crates/apollia-desktop/src/commands/permissions_proposals.rs` pour
le détail des trois Tauri commands.
