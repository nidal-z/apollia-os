# Sprint 0 — Fondations

**Objectif :** Workspace Rust qui compile + tous les types de base définis.
**Livrable démo-able :** `cargo build --workspace` sans erreur + CI verte.
**Durée :** Semaines 1-2

---

## Stories

| ID | Titre | Crate | Taille | Statut |
|---|---|---|---|---|
| [STORY-001](story-001-init-workspace-cargo.md) | Init workspace Cargo avec 7 crates | workspace | S | ✅ |
| [STORY-002](story-002-types-fondamentaux-apollia-core.md) | Types fondamentaux apollia-core (AgentManifest, AIPTask, AIPResult) | apollia-core | M | ✅ |
| [STORY-003](story-003-types-processstate-taskstatus.md) | Types ProcessState, TaskStatus, AIPError avec serde | apollia-core | S | ✅ |
| [STORY-004](story-004-stepbudgetconfig-sandboxprofile.md) | StepBudgetConfig et SandboxProfile | apollia-core | S | ✅ |
| [STORY-005](story-005-ci-cargo-fmt-clippy-test.md) | CI : cargo fmt + clippy + test | workspace | S | ✅ |

---

## Ordre d'implémentation

```
STORY-001 (workspace)
  └── STORY-002 (types AIP)
        └── STORY-003 (enums lifecycle)
              └── STORY-004 (budget + sandbox)
                    └── STORY-005 (CI)
```

## Critère de sortie du sprint

- [x] `cargo build --workspace` : 0 erreur, 0 warning
- [x] `cargo test --workspace` : tous les tests passent
- [x] `cargo clippy --workspace -- -D warnings` : propre
- [x] CI GitHub Actions verte sur main
- [x] Types `AgentManifest`, `AIPTask`, `AIPResult`, `ProcessState`, `TaskStatus`, `StepBudgetConfig`, `SandboxProfile` disponibles dans `apollia-core`
