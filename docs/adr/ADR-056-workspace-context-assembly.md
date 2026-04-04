# ADR-056 — Workspace Context Assembly

**Date :** 2026-04-04
**Statut :** Accepté
**Décideur :** Nidal
**Sprint :** 35 — Workspace Intelligence & Execution Performance

---

## Contexte

L'analyse comparative Apollia OS vs Claude Code (211 fichiers TypeScript, `docs/internal/plans/glowing-tumbling-donut.md`) a identifié 6 manques critiques. Parmi eux : l'agent ignore complètement le projet dans lequel il opère. Sans contexte workspace, un agent demandé d'analyser un repo doit découvrir lui-même la branche git, les fichiers modifiés, le langage dominant — en consommant des steps et des tokens précieux.

**Besoin :** Injecter automatiquement dans le system prompt les métadonnées du projet courant (branche, fichiers modifiés, APOLLIA.md, arborescence) avant chaque appel Reasoner.

**Contraintes :**
- Principe #1 (Local-first) : zéro appel réseau
- Principe #2 (Zéro dépendance externe) : le binaire doit fonctionner sans bibliotèque C externe
- La collecte ne doit jamais bloquer ni retarder l'exécution d'une tâche

---

## Décision

Créer la crate `apollia-workspace` avec les composants suivants :

- **`WorkspaceAssembler`** : orchestrateur principal, agrège les providers avec timeout global 2s et TTL de cache 30s
- **`GitContextCollector`** : collecte branche, HEAD, fichiers modifiés via subprocess `git`
- **`ApolliamdFinder`** : recherche `APOLLIA.md` en remontant depuis CWD → parents → `$HOME`
- **`DirectoryTreeBuilder`** : arborescence limitée à 3 niveaux, exclusions `.git`, `node_modules`, `target`
- **`ContextProvider` trait** : interface générique permettant l'extension (Rust natif, duck-typing Python, script stdin/stdout JSON)

### Rejet de git2 crate

`git2` est la bibliothèque Rust la plus connue pour interagir avec git. Elle est rejetée pour trois raisons :
1. Dépendance dynamique à `libgit2` (C) — incompatible avec Principe #2 (zéro dépendance externe)
2. Temps de compilation > 30s (build script + compilation C) — ralentit le CI
3. Binary size +5 MB pour une fonctionnalité qui peut être obtenue via un subprocess

**Subprocess `git` :** disponible partout où git est installé. Sur les repos sans git, `GitContextCollector` retourne `None` (fail-silent) — les agents non-git continuent de fonctionner normalement.

### Convention APOLLIA.md

La recherche suit la même priorité que `CLAUDE.md` de l'écosystème Claude Code :
1. CWD (`.`)
2. Parents successifs jusqu'à la racine du filesystem
3. `$HOME`

Premier fichier trouvé gagne. Si aucun `APOLLIA.md` n'existe, le champ est `None` dans `WorkspaceContext`.

---

## Conséquences

**Positives :**
- Timeout global 2s : la collecte ne bloque jamais l'exécution d'une tâche
- TTL 30s : évite les I/O répétées sur les sessions longues (hot path)
- Fail-silent sur git : les agents dans des répertoires sans repo git fonctionnent sans modification
- `APOLLIA.md` permet à l'utilisateur de personnaliser le comportement de l'agent par projet

**Négatives / Compromis :**
- Estimation tokens approximative pour le `WorkspaceContext` sérialisé (chars/4 × 1.2) — conservateur intentionnel pour éviter les dépassements context
- L'arborescence est limitée à 3 niveaux et 200 entrées — repos très profonds sont tronqués

**Neutres / À surveiller :**
- Si `git` n'est pas dans le `$PATH`, `GitContextCollector` retourne `None` silencieusement. Sur Windows, git est souvent absent par défaut.

---

## Principes architecturaux impactés

- **Principe #2 — Zéro dépendance externe** : Pas de `libgit2`. Subprocess `git` — ubiquitaire, fail-silent si absent. Conforme.
- **Principe #4 — Fail fast** : Timeout 2s global. Si la collecte dépasse ce délai, le contexte partiel est utilisé. Conforme.
- **Principe #5 — Un acteur, une responsabilité** : `WorkspaceAssembler` délègue à des providers spécialisés, chacun avec une seule responsabilité.

---

## Liens

- Stories d'implémentation : STORY-458, STORY-459, STORY-460
- Implémenté dans : `crates/apollia-workspace/`
- Trait `ContextProvider` : [ADR-060](ADR-060-context-provider-trait.md)
- Wiki : [Briques-Workspace](../wiki/Briques-Workspace.md)
