# ADR-038 - Contrat d'arguments des steps de plan orchestrés

**Date :** 2026-07-08
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation

---

## Contexte

Le chantier #2 a câblé le chemin orchestré ORIA au vrai `ToolProxy` gouverné (permissions + audit + résilience + budget), remplaçant le `NoopToolProxy`. Ce faisant, il a mis à nu un manque : `apollia_core::plan::PlanStep` ne porte **pas d'arguments structurés**, seulement une description. Un step orchestré ne peut donc pas passer d'entrée valide aux outils natifs qui en exigent (bash, file, http). Un outil à entrée triviale (echo) fonctionne ; les autres non. C'est le dernier blocage de la capacité "lancer une task à un agent orchestré" (cap 2.1).

Contrainte de valeur : la force de l'orchestré est le **plan comme artefact de première classe** (DAG pour le parallélisme, plan-gate HITL, audit et replay). C'est la primitive de redevabilité sur laquelle repose le positionnement EU AI Act. Le contrat d'args doit préserver cette propriété, pas la diluer.

Contraintes techniques : modifier `PlanStep` touche un modèle **public** d'`apollia-core` (défini par ADR-031), donc procédure ASK FIRST + cet ADR. Apollia dispose déjà de la génération contrainte par grammaire (GBNF), utilisée par la commande `do` et le tool-calling.

## Décision

Nous adoptons une résolution d'arguments **hybride A+B** pour les steps outil du plan orchestré :

- **A (défaut, au moment du plan)** : `PlanStep` gagne un champ d'arguments structurés (`args: Option<serde_json::Value>`). Le Reasoner les remplit par **génération schema-guided (GBNF)** contrainte au schéma de l'outil ciblé, et ils sont **validés** avant exécution. Le plan est ainsi pleinement spécifié, déterministe, auditable et rejouable avec ses vrais arguments.
- **B (repli, au moment de l'exécution)** : si les args d'un step outil sont absents ou échouent la validation, l'`ActorLoop` déclenche une **extraction JIT** (un appel LLM mappant description + schéma d'outil vers des args), validée à son tour, avant d'échouer le step. Filet de sécurité pour les cas où le Reasoner n'a pas produit d'args valides au plan.

L'`ActorLoop` appelle `tool_proxy.invoke(tool, args)` avec les args résolus (A, puis B en repli). L'exécution reste intégralement sous le ToolProxy gouverné.

## Alternatives considérées

### Option B seule - extraction JIT systématique (rejetée)
**Pour :** ne touche pas le modèle public `PlanStep`.
**Contre :** un appel LLM par step outil (coût, latence), non-déterministe, et le plan reste sous-spécifié, ce qui dégrade l'audit et le replay du plan. Perd la propriété "plan pleinement spécifié".

### Option C - tool-calling natif dans le plan (rejetée)
**Pour :** réutilise le mécanisme du chemin chat, déjà prouvé.
**Contre :** fait converger l'orchestré vers du ReAct générique et dilue le plan-comme-artefact. On perd une partie du moat DAG / audit / replay, qui est précisément le différenciateur EU AI Act.

### Option retenue - A + B hybride
**Pour :** A préserve le plan auditable et rejouable (le moat) ; B apporte la robustesse sans sacrifier cette propriété. S'appuie sur le GBNF déjà présent.
**Compromis acceptés :** deux chemins de résolution d'args (plus de complexité et de tests) ; modification d'un modèle public `apollia-core`.

## Conséquences

**Positives :**
- L'orchestré pilote enfin de vrais outils natifs : débloque la cap 2.1.
- Le plan reste un artefact pleinement spécifié, auditable et rejouable : la primitive EU AI Act est renforcée, pas diluée.
- Réutilise le GBNF existant (pas de nouvelle brique).

**Négatives / Compromis :**
- Changement d'un modèle public d'`apollia-core` (`PlanStep`) : touche plan-mode, `audit_journal` (snapshots de plan), replay, l'UI plan-mode du desktop, et impose une migration/valeur par défaut pour les plans existants. Travail transverse.
- Deux chemins de résolution d'args (A + repli B) : surface de complexité et de tests accrue.
- Le repli B ajoute un appel LLM quand il se déclenche (coût/latence sur ces cas).

**Neutres / À surveiller :**
- Le taux de déclenchement du repli B : s'il est élevé, c'est que la génération d'args au plan (A) est faible et doit être améliorée.
- La compatibilité du replay avec les anciens plans sans args (migration / défaut à `None`).

## Principes architecturaux impactés

- **Principe #7 - Safeguards non-bypassables** : les args résolus passent par le `ToolProxy` gouverné (permissions + audit + budget) ; l'exécution reste sous garde.
- **Moat audit / redevabilité** : un plan pleinement spécifié renforce l'auditabilité et le replay.
- Modifie un modèle **public** d'`apollia-core` : procédure ASK FIRST respectée via cet ADR ; étend ADR-031.

## Liens

- ADR-031 (modèle de plan unifié dans apollia-core) : cet ADR étend `PlanStep`.
- ADR-037 (contrat de pilotage hôte) : chantier précédent.
- Cartographie : `docs/internal/cartography/capability-registry.md` (cap 2.1).
- Origine : rapport du chantier #2 (garde-fous budget + orchestré), qui a mis le besoin à nu.
- Story associée : à créer (chantier #3).
