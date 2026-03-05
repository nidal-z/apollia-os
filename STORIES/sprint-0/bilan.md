# Sprint 0 — Bilan

**Sprint Goal :** `cargo build --workspace` sans erreur + CI verte — **atteint ✅**
**Démo :** `cargo test -p apollia-core` passe (13 tests) + pipeline CI fmt/clippy/test chaîné

---

## Stories livrées

| ID | Story | Taille estimée | Statut |
|---|---|---|---|
| STORY-001 | Init workspace Cargo avec 7 crates | S | ✅ |
| STORY-002 | Types fondamentaux `apollia-core` (AgentManifest, AIPTask, AIPResult) | M | ✅ |
| STORY-003 | Types ProcessState, TaskStatus, AIPError avec serde | S | ✅ |
| STORY-004 | StepBudgetConfig et SandboxProfile | S | ✅ |
| STORY-005 | CI : cargo fmt + clippy + test | S | ✅ |

---

## Ce qui a bien marché

- `[workspace.dependencies]` centralisé : zéro duplication de version dans les crates
- Types `apollia-core` richement couverts par des tests unitaires (structure GIVEN/WHEN/THEN)
- `thiserror` intégré directement sur `AIPError` via `#[derive(thiserror::Error)]`
- `SandboxProfile::requires_dangerous_flag()` : logique métier encodée dans le type, pas dans les appelants
- CI chaînée fmt → clippy → test avec cache Rust (`Swatinem/rust-cache@v2`)
- `rusqlite` en mode `bundled` : Principe #2 (zéro dépendance externe) respecté
- Profile `release` avec LTO + strip : binaire minimal préparé

---

## Ce qui a pose probleme

- L'index sprint (`sprint-index.md` et `sprint-0/index.md`) n'a pas été mis à jour au fil de l'implémentation — synchronisation manuelle requise en fin de sprint. A corriger en Sprint 1 : mettre à jour l'index à chaque story terminée.

---

## Stories reportées

Aucune.

---

## Decisions architecturales prises

- ADR-001 à ADR-010 : extraits et documentés dans `docs/adr/` en début de sprint (déjà actés).
- Décision tacite : `AIPError` derive `thiserror::Error` directement (pas de wrapping intermédiaire) — cohérent avec la règle `thiserror`-only.

---

## Dette technique identifiée

| # | Dette | Sévérité | Sprint cible |
|---|---|---|---|
| DT-001 | 6 crates hors `apollia-core` sont des squelettes vides (`lib.rs` sans contenu) — normal pour Sprint 0, mais à ne pas oublier | Faible | Sprint 1→5 |
| DT-002 | `apollia-cli/src/main.rs` est vide — le binaire ne compile pas encore en mode utile | Faible | Sprint 5 |
| DT-003 | CI sur `ubuntu-latest` uniquement — PyO3 sur macOS (dev) pourrait diverger au moment du Sprint 4 | Moyen | Sprint 4 |
| DT-004 | Index sprint non synchronisé en cours de sprint — risque de perte de contexte entre sessions | Moyen | Process continu |
| DT-005 | Pas de `Cargo.lock` versionné confirmé — à vérifier (doit être commité pour un binaire) | Moyen | Immédiat |

---

## Focus Sprint 1

**Sprint Goal cible :** Test d'intégration `EventBus ↔ AgentRegistry` qui passe.

Stories à implémenter dans l'ordre :
1. STORY-006 — EventBus broadcast Tokio + catalogue RuntimeEvent (M)
2. STORY-007 — AgentRegistry acteur Tokio Register/Unregister/UpdateState (M)
3. STORY-008 — AgentRegistryHandle API publique async (S)
4. STORY-009 — Test d'intégration EventBus ↔ AgentRegistry (M)

Charge estimée : 10h / budget 16-20h — confortable.
