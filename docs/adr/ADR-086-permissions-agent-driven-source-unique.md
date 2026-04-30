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
