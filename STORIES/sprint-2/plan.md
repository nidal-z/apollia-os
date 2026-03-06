# Sprint 2 — Plan

**Sprint Goal :** `bash_executor.run("echo hello")` retourne stdout, l'invocation est tracée dans SQLite.
**Durée estimée :** 26h / budget 24-30h (3 semaines)
**Dates :** semaines 5-7

---

## Stories du sprint (ordre d'implémentation)

| Priorité | ID | Story | Taille | Estimé | Dépend de |
|---|---|---|---|---|---|
| 1 | STORY-010 | ToolDescriptor, ToolKind types dans apollia-tools | S | 2h | STORY-004 ✅ |
| 2 | STORY-011 | ToolRegistry catalogue en mémoire | M | 3h | STORY-010 |
| 3 | STORY-015 | file_io avec validation path traversal | M | 3h | STORY-010 |
| 4 | STORY-013 | bash_executor avec Linux namespaces (unshare) | L | 6h | STORY-010 |
| 5 | STORY-016 | Audit trail SQLite (tool_invocations) | M | 3h | STORY-010, STORY-013 |
| 6 | STORY-014 | python_executor avec virtualenv isolé | L | 6h | STORY-010 |
| 7 | STORY-012 | ToolResolver validation à INITIALIZING | M | 3h | STORY-011, STORY-007 ✅ |

**Sprint Goal atteint apres :** STORY-010 + 011 + 013 + 016 = 14h (fin semaine 6)
**Stories complementaires :** STORY-015, STORY-014, STORY-012 = 12h (semaine 7)

---

## Dependances verifiees

| Dependance | Status | Story dependante |
|---|---|---|
| `SandboxProfile` dans apollia-core (STORY-004) | ✅ Sprint 0 | STORY-010 (reutilisation) |
| `AgentRegistry` acteur Tokio (STORY-007) | ✅ Sprint 1 | STORY-012 |
| `AgentManifest.tools_required` (STORY-002) | ✅ Sprint 0 | STORY-012 |
| `EventBus` (STORY-006) | ✅ Sprint 1 | STORY-016 (emit ToolInvocationCompleted) |

**Note STORY-010 :** `SandboxProfile` est deja defini dans `apollia-core/src/sandbox.rs`.
STORY-010 cree `ToolDescriptor` et `ToolKind` dans `apollia-tools` en important `SandboxProfile` depuis `apollia-core`. Pas de redefinition.

---

## Risques identifies

### Risque #1 — Linux namespaces non disponibles sur macOS (ELEVE) — DOCUMENTE ADR-012
- **Contexte :** `unshare(1)` n'existe pas sur macOS (Darwin). L'environnement de dev est Darwin 25.0.0.
- **Impact :** STORY-013 ne peut pas tester le sandbox reel sur la machine de dev.
- **Decision (ADR-012) :** `#[cfg(target_os = "linux")]` pour deux chemins compiles : `SandboxMode::LinuxNamespaces` sur Linux, `SandboxMode::Dev` sur macOS avec `tracing::warn!` a chaque invocation. `sandbox-exec` macOS rejete (deprecated depuis macOS 10.15, API privee).
- **CI :** Doit tourner sur `ubuntu-latest` pour valider le chemin sandbox reel.
- **Reference :** docs/adr/ADR-012-sandbox-devmode-macos.md

### Risque #2 — rusqlite bundled : premier usage dans apollia-tools (FAIBLE)
- **Contexte :** `rusqlite` est en workspace deps mais jamais utilise jusqu'ici.
- **Impact :** Compilation plus longue la premiere fois (SQLite bundled). Possible friction avec le feature flag `bundled`.
- **Mitigation :** Verifier `rusqlite = { workspace = true }` avec feature `bundled` dans `Cargo.toml` workspace avant STORY-016.

### Risque #3 — python_executor : virtualenv et pip disponibles (MOYEN)
- **Contexte :** `python_executor` suppose `python3` et `venv` installes sur le systeme hote.
- **Impact :** STORY-014 peut echouer sur des machines sans Python.
- **Mitigation :** Verifier la disponibilite de `python3 -m venv` a `INITIALIZING` et retourner une erreur `PythonUnavailable` claire.

### Risque #4 — sha2 absent du workspace (FAIBLE)
- **Contexte :** L'audit trail calcule un SHA256 des parametres d'invocation.
- **Impact :** STORY-016 necessite `sha2` comme nouvelle dependance workspace.
- **Mitigation :** Ajouter `sha2 = "0.10"` dans `[workspace.dependencies]` du `Cargo.toml` racine.

---

## Definition of Done du sprint

- [ ] Sprint Goal atteint et demo-able : `bash_executor` tracé dans SQLite
- [ ] `cargo test --workspace` passe (0 test echoue)
- [ ] `cargo clippy --workspace -- -D warnings` : zero warning
- [ ] `cargo fmt --check` : code formate
- [ ] `sprint-index.md` mis a jour (toutes les stories ✅)
- [ ] `sprint-2/bilan.md` redige

---

## Ordre d'implementation detail

```
semaine 5
  jour 1-2 : STORY-010 — types (2h)
  jour 3-4 : STORY-011 — ToolRegistry catalogue (3h)
  jour 5   : STORY-015 — file_io (debut)

semaine 6
  jour 1-2 : STORY-015 — file_io (fin, 3h total)
  jour 3-5 : STORY-013 — bash_executor (6h)

semaine 7
  jour 1-2 : STORY-016 — Audit trail SQLite (3h) ← Sprint Goal ATTEINT
  jour 3-4 : STORY-014 — python_executor (6h)
  jour 5   : STORY-012 — ToolResolver (3h)
```
