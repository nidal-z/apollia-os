# ADR-008 — Pattern `noun verb` pour la CLI

**Date :** 2026-03
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation

---

## Contexte

La CLI `apollia-os` expose plusieurs dizaines de commandes réparties sur différents domaines (agents, tâches, outils, mémoire, audit). Deux patterns dominent les CLIs modernes : `verb-noun` (ex: `kubectl get pod`) et `noun-verb` (ex: `docker container ls`). Le choix doit être cohérent sur toutes les commandes et intuitif pour les opérateurs.

## Décision

Nous adoptons `noun-verb` : `apollia-os agent start`, `apollia-os task list`, `apollia-os memory inspect`. Les commandes de niveau 1 (`start`, `stop`, `status`, `run`) sont des exceptions justifiées par leur usage quotidien et leur universalité.

## Alternatives considérées

### Option A — `verb-noun` (rejetée)
**Pour :** Similaire à kubectl, familier pour les utilisateurs Kubernetes.
**Contre :** Moins intuitif pour explorer les capacités d'un objet (quels verbes s'appliquent à "agent" ?). L'autocomplétion est moins naturelle.

### Option B — Mixte selon le contexte (rejetée)
**Pour :** Liberté de choisir le plus naturel cas par cas.
**Contre :** Incohérent. Source de confusion pour les opérateurs. Impossible à documenter proprement.

### Option retenue — `noun-verb` uniforme
**Pour :** `apollia-os agent <TAB>` liste toutes les actions possibles sur un agent. Découverte naturelle. Cohérent avec Docker CLI et Homebrew.
**Compromis acceptés :** Les commandes de niveau 1 (`start`, `stop`, `status`, `run`) sont des exceptions — justifiées par leur fréquence d'usage.

## Conséquences

**Positives :**
- L'autocomplétion shell guide l'utilisateur : `apollia-os agent <TAB>` → `start | stop | list | logs`.
- Cohérent avec Docker CLI (référence culturelle pour les développeurs DevOps).
- Structure clap v4 derive naturelle : sous-commandes par domaine.

**Négatives / Compromis :**
- Les exceptions de niveau 1 (`start`, `stop`) créent une légère incohérence.
- `apollia-os run` est un raccourci pour `apollia-os task run` — à documenter explicitement.

**Neutres / À surveiller :**
- Vérifier la cohérence lors de l'ajout de nouvelles commandes en Sprint 5.

## Principes architecturaux impactés

- Principe #8 — CLI humaine, API machine : La CLI doit être intuitive pour un humain.

## Liens

- Story associée : STORY-037 (CLI commandes niveau 1), STORY-038 (CLI commandes niveau 2)
- ADR précédent sur le même sujet : aucun
