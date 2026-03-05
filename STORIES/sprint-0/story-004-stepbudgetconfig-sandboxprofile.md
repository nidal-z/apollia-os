## [SPRINT-0][CORE] Implémenter StepBudgetConfig et SandboxProfile

**ID :** STORY-004
**Sprint :** 0
**Crate cible :** `apollia-core`
**Fichier(s) cible(s) :**
- `crates/apollia-core/src/budget.rs`
- `crates/apollia-core/src/sandbox.rs`
- `crates/apollia-core/src/lib.rs` (re-exports)

**Taille :** S
**Dépend de :** STORY-003
**Statut :** 🔲 À faire

---

## User Story

```
En tant que runtime Apollia OS,
je veux des types configurables pour les limites d'exécution (StepBudgetConfig)
et les profils d'isolation sandbox (SandboxProfile),
afin que les agents puissent déclarer leurs contraintes dans leur AgentManifest
et que le runtime puisse les appliquer sans possibilité de contournement.
```

---

## Contexte technique

`StepBudgetConfig` est le type utilisé dans `AgentManifest.step_budget` (STORY-002) pour que l'agent puisse surcharger les defaults runtime. C'est une configuration déclarative — la logique d'application est dans `apollia-oria` (STORY-030). Il est critique que ces types soient définis avant STORY-002 pour que le champ `AgentManifest.step_budget: Option<StepBudgetConfig>` compile.

`SandboxProfile` est utilisé par `ToolDescriptor` dans `apollia-tools` (Sprint 2) mais défini dans `apollia-core` car c'est une contrainte architecturale fondamentale (ADR-005).

**Principe(s) architectural(aux) concerné(s) :**
- Principe #7 — Garde-fous non-négociables : StepBudget appliqué par le runtime, non configurable à runtime

**Position dans l'architecture :**
```
apollia-core/src/
  ├── budget.rs      ← StepBudgetConfig   ← cette story
  └── sandbox.rs     ← SandboxProfile     ← cette story

apollia-oria (Sprint 4)
  └── StepBudget (runtime) ← utilise StepBudgetConfig comme config initiale

apollia-tools (Sprint 2)
  └── ToolDescriptor ← contient SandboxProfile
```

---

## Critères d'Acceptation

### AC-1 — StepBudgetConfig a des valeurs par défaut cohérentes

```
ÉTANT DONNÉ StepBudgetConfig::default()
QUAND on inspecte les champs
ALORS max_steps == 10,
  ET max_tool_calls == 20,
  ET wall_clock_secs == 300
```

### AC-2 — StepBudgetConfig est sérialisable avec round-trip JSON

```
ÉTANT DONNÉ StepBudgetConfig { max_steps: 5, max_tool_calls: 10, wall_clock_secs: 60 }
QUAND on sérialise puis désérialise en JSON
ALORS les valeurs sont identiques après round-trip
```

### AC-3 — SandboxProfile couvre les 4 niveaux d'isolation

```
ÉTANT DONNÉ l'enum SandboxProfile
QUAND on liste ses variants
ALORS il contient exactement : ReadOnly, FileSystem, NetworkRestricted, Full
```

### AC-4 — SandboxProfile::Full nécessite une déclaration explicite dangerous=true

```
ÉTANT DONNÉ SandboxProfile::Full
QUAND on appelle profile.requires_dangerous_flag()
ALORS la méthode retourne true
  ET tous les autres profiles retournent false
```

---

## Spécification technique

### Types à créer

```rust
// crates/apollia-core/src/budget.rs

/// Configuration du budget d'exécution déclarée par l'agent dans son AgentManifest.
///
/// Ces valeurs sont des suggestions maximales. Le runtime (ORIA StepBudget, STORY-030)
/// applique les valeurs minimales entre la config agent et la config runtime globale.
/// Un agent ne peut PAS dépasser les limites configurées dans apollia.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepBudgetConfig {
    /// Nombre maximum de steps ORIA (appels successifs à l'agent). Défaut: 10.
    pub max_steps: u32,
    /// Nombre maximum d'appels d'outils au total sur la tâche. Défaut: 20.
    pub max_tool_calls: u32,
    /// Durée maximum wall-clock en secondes. Défaut: 300 (5 minutes).
    pub wall_clock_secs: u64,
}

impl Default for StepBudgetConfig {
    fn default() -> Self {
        Self {
            max_steps: 10,
            max_tool_calls: 20,
            wall_clock_secs: 300,
        }
    }
}

// crates/apollia-core/src/sandbox.rs

/// Profil d'isolation sandbox appliqué à l'exécution d'un outil natif.
///
/// Défini dans apollia-core car c'est une contrainte architecturale fondamentale (ADR-005).
/// L'isolation effective est implémentée dans apollia-tools via Linux namespaces (STORY-013).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxProfile {
    /// tmpfs read-only + PID namespace. 128MB RAM, 30s timeout.
    /// Usage: lecture de fichiers, calculs purs.
    ReadOnly,
    /// Sandbox filesystem rw + PID namespace. 256MB RAM, 60s timeout.
    /// Usage: écriture de fichiers dans un répertoire isolé.
    FileSystem,
    /// FileSystem + network namespace + iptables allowlist.
    /// Accès réseau limité à network_allowlist du manifest.
    NetworkRestricted,
    /// Tout autorisé — aucune restriction de sandbox.
    /// EXIGE dangerous=true dans ToolDescriptor. Non recommandé en production.
    Full,
}

impl SandboxProfile {
    /// Retourne true si ce profil exige `dangerous=true` dans ToolDescriptor.
    pub fn requires_dangerous_flag(&self) -> bool {
        matches!(self, SandboxProfile::Full)
    }
}
```

### Dépendances Cargo

```toml
# Aucune nouvelle dépendance — serde déjà déclaré dans STORY-001
```

### Ce que cette story N'implémente PAS

- La logique d'application du StepBudget à runtime → STORY-030 (apollia-oria)
- L'isolation sandbox effective via unshare → STORY-013 (apollia-tools)
- La validation que dangerous=true est présent lors de l'utilisation de Full → STORY-012

---

## Tests requis

### Tests unitaires

```rust
// crates/apollia-core/src/budget.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ac1_step_budget_defaults() {
        // GIVEN / WHEN
        let budget = StepBudgetConfig::default();
        // THEN
        assert_eq!(budget.max_steps, 10);
        assert_eq!(budget.max_tool_calls, 20);
        assert_eq!(budget.wall_clock_secs, 300);
    }

    #[test]
    fn test_ac2_step_budget_round_trip_json() {
        // GIVEN
        let budget = StepBudgetConfig { max_steps: 5, max_tool_calls: 10, wall_clock_secs: 60 };
        // WHEN
        let json = serde_json::to_string(&budget).expect("serialize");
        let restored: StepBudgetConfig = serde_json::from_str(&json).expect("deserialize");
        // THEN
        assert_eq!(restored.max_steps, 5);
        assert_eq!(restored.max_tool_calls, 10);
        assert_eq!(restored.wall_clock_secs, 60);
    }
}

// crates/apollia-core/src/sandbox.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ac3_sandbox_profile_variants() {
        // GIVEN / WHEN
        let profiles = [
            SandboxProfile::ReadOnly,
            SandboxProfile::FileSystem,
            SandboxProfile::NetworkRestricted,
            SandboxProfile::Full,
        ];
        // THEN
        assert_eq!(profiles.len(), 4);
    }

    #[test]
    fn test_ac4_only_full_requires_dangerous_flag() {
        // GIVEN / WHEN / THEN
        assert!(SandboxProfile::Full.requires_dangerous_flag());
        assert!(!SandboxProfile::ReadOnly.requires_dangerous_flag());
        assert!(!SandboxProfile::FileSystem.requires_dangerous_flag());
        assert!(!SandboxProfile::NetworkRestricted.requires_dangerous_flag());
    }
}
```

---

## Definition of Done

**Qualité code :**
- [ ] `cargo test -p apollia-core` passe (tous les tests de STORY-002, 003 et 004)
- [ ] `cargo clippy -p apollia-core -- -D warnings` : zéro warning
- [ ] `cargo fmt --check` : code formatté
- [ ] Zéro `unwrap()` dans le code de production
- [ ] Docstring `///` sur chaque struct, enum, variant et méthode publique

**Architectural :**
- [ ] `StepBudgetConfig::default()` reflète les valeurs du fichier `apollia.toml` (max_steps=10, max_tool_calls=20, wall_clock=300s)
- [ ] `SandboxProfile::Full.requires_dangerous_flag() == true`
- [ ] Round-trip serde JSON vérifié par test

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-core): add StepBudgetConfig and SandboxProfile types`

---

## Notes d'implémentation

**Décisions prises pendant l'implémentation :**

**Déviations par rapport à la spec :**

**Dette technique identifiée :**

---

## Liens

- Epic parent : Sprint 0 — Fondations
- Story précédente : STORY-003
- Story suivante : STORY-005
- ADR associé : ADR-005 (Sandbox sans Docker — SandboxProfile reflète cette décision)
