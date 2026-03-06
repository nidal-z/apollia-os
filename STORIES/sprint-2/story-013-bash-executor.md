# [Sprint 2][apollia-tools] bash_executor — execution shell avec Linux namespaces

**ID :** STORY-013
**Sprint :** 2
**Crate cible :** `apollia-tools`
**Fichier(s) cible(s) :** `crates/apollia-tools/src/tools/bash_executor.rs`
**Taille :** L
**Depend de :** STORY-010
**Statut :** 🔲 A faire

---

## User Story

```
En tant que runtime,
je veux un outil bash_executor qui execute des commandes shell dans un environnement
isole par Linux namespaces (PID + mount),
afin qu'un agent puisse lancer des commandes systeme sans que celles-ci ne compromettent
l'hote ou les autres agents.
```

---

## Contexte technique

Outil natif central du sprint 2, et cle du Sprint Goal. Implemente `SandboxProfile::FileSystem`
via `unshare(1)` sur Linux. Sur macOS (environnement de dev), un mode `NoSandbox` est utilise
avec un warning explicite — le sandbox reel n'est valide que sur Linux / CI.

**Principe(s) architecturaux concernes :**
- Principe #2 — Zero dependance externe (utilise `unshare` pre-installe sur Linux, pas de crate tierce)
- Principe #4 — Fail fast (timeout hard applique, exit code non-zero → erreur structuree)

**Position dans l'architecture :**
```
apollia-tools
  ├── descriptor.rs  (STORY-010 ✅)
  ├── registry.rs    (STORY-011 ✅)
  └── tools/
        └── bash_executor.rs  <- cette story
              ├── BashExecutor      (struct publique)
              ├── BashInput         (parametres d'execution)
              ├── BashOutput        (resultat d'execution)
              └── sandbox::apply()  (abstraction Linux/macOS)
```

---

## Criteres d'Acceptation

### AC-1 — Execution reussie retourne stdout

```
ETANT DONNE un BashExecutor initialise
QUAND on execute BashInput { command: "echo hello", timeout_secs: 30, working_dir: None }
ALORS BashOutput { stdout: "hello\n", stderr: "", exit_code: 0, duration_ms: _ } est retourne
```

### AC-2 — Commande echouee retourne exit_code non-zero

```
ETANT DONNE un BashExecutor initialise
QUAND on execute BashInput { command: "exit 42", timeout_secs: 30, working_dir: None }
ALORS BashOutput { exit_code: 42, ... } est retourne (pas d'erreur Rust, c'est un echec de commande)
```

### AC-3 — Timeout hard respecte

```
ETANT DONNE un BashExecutor avec timeout = 1 seconde
QUAND on execute BashInput { command: "sleep 10", timeout_secs: 1, working_dir: None }
ALORS Err(BashExecutorError::Timeout { command, timeout_secs: 1 }) est retourne
ET le processus enfant est termine (pas de zombie)
```

### AC-4 — Commande vide rejetee (fail fast)

```
ETANT DONNE un BashExecutor initialise
QUAND on execute BashInput { command: "", ... }
ALORS Err(BashExecutorError::EmptyCommand) est retourne immediatement
```

### AC-5 — stderr capture separement

```
ETANT DONNE une commande qui ecrit sur stderr : "echo error >&2"
QUAND on execute cette commande
ALORS BashOutput.stderr contient "error\n" ET BashOutput.stdout est vide
```

### AC-6 — working_dir invalide retourne erreur

```
ETANT DONNE un BashExecutor initialise
QUAND on execute BashInput { working_dir: Some("/nonexistent"), ... }
ALORS Err(BashExecutorError::WorkingDirNotFound(_)) est retourne
```

---

## Specification technique

### Types a creer dans `crates/apollia-tools/src/tools/bash_executor.rs`

```rust
/// Outil natif d'execution shell avec isolation namespace Linux.
///
/// Sur Linux : utilise unshare(1) pour isoler PID + mount namespaces.
/// Sur macOS (dev uniquement) : execution directe avec warning tracing.
pub struct BashExecutor {
    /// Mode sandbox actif pour cet executeur.
    sandbox_mode: SandboxMode,
}

/// Parametres d'une invocation bash.
pub struct BashInput {
    /// Commande shell a executer (interpretee par /bin/sh -c).
    pub command: String,
    /// Timeout en secondes avant SIGKILL. Max 300s.
    pub timeout_secs: u64,
    /// Repertoire de travail. None = repertoire courant du processus.
    pub working_dir: Option<PathBuf>,
}

/// Resultat d'une invocation bash.
pub struct BashOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
}

/// Mode d'application du sandbox (cf. ADR-012).
///
/// Determine a la compilation via cfg(target_os), pas a l'execution.
enum SandboxMode {
    /// Linux namespaces via unshare --pid --mount (production Linux).
    LinuxNamespaces,
    /// Mode developpement — aucun sandbox, warning emis a chaque invocation.
    /// Actif sur tout OS non-Linux (macOS, Windows). Jamais en CI/production.
    Dev,
}

/// Erreurs d'invocation bash.
pub enum BashExecutorError {
    EmptyCommand,
    WorkingDirNotFound(PathBuf),
    Timeout { command: String, timeout_secs: u64 },
    SpawnFailed(String),
    OutputCaptureFailed(String),
}

impl BashExecutor {
    /// Cree un BashExecutor. Detecte automatiquement Linux vs macOS.
    ///
    /// Sur macOS, emet tracing::warn! pour signaler l'absence de sandbox.
    pub fn new() -> Self { ... }

    /// Execute une commande shell avec isolation sandbox.
    pub async fn run(&self, input: BashInput) -> Result<BashOutput, BashExecutorError> { ... }

    /// Retourne le ToolDescriptor de cet outil pour enregistrement dans ToolRegistry.
    pub fn descriptor() -> ToolDescriptor { ... }
}
```

### Dependances Cargo

```toml
# Aucune nouvelle dependance — tokio (process feature), thiserror, tracing sont deja declares
# tokio doit avoir la feature "process" activee dans workspace Cargo.toml
```

**Verification :** S'assurer que `tokio = { version = "1", features = ["full"] }` dans
`[workspace.dependencies]` inclut bien "process". Avec "full" c'est inclus.

### Comportement attendu — Sandbox Linux

Commande effective sur Linux :
```sh
/usr/bin/unshare --pid --mount --fork /bin/sh -c "<commande>"
```

- `--pid` : Nouveau namespace PID (processus isole, ne voit pas les autres)
- `--mount` : Nouveau namespace mount (modifications de mount non visibles hote)
- `--fork` : Necessaire pour que l'init du namespace PID soit un child du processus unshare

Limites de ressources appliquees via timeout Tokio (pas cgroups MVP) :
- Timeout : via `tokio::time::timeout(Duration::from_secs(input.timeout_secs), ...)`
- Kill sur timeout : `child.kill().await` puis `child.wait().await` (pas de zombie)

### Comportement attendu — Mode Dev (macOS, cf. ADR-012)

Le mode est determine a la compilation via `#[cfg(target_os = "linux")]`, pas a l'execution.

```rust
#[cfg(not(target_os = "linux"))]
fn build_command(input: &BashInput) -> tokio::process::Command {
    tracing::warn!(
        command = %input.command,
        "bash_executor: running in Dev mode — no sandbox active. \
         Linux namespaces are not available on this platform. \
         Production deployments require Linux."
    );
    // tokio::process::Command directement
}
```

### Ce que cette story N'implemente PAS

- Cgroups (CPU/RAM limits) — roadmap v0.2
- Mount namespace avec tmpfs dedie — roadmap v0.2
- Audit trail (STORY-016)
- Enregistrement automatique dans ToolRegistry au demarrage (hors scope sprint 2)

---

## Tests requis

### Tests unitaires dans `crates/apollia-tools/src/tools/bash_executor.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ac1_echo_returns_stdout() {
        // GIVEN
        let executor = BashExecutor::new();
        let input = BashInput {
            command: "echo hello".to_string(),
            timeout_secs: 10,
            working_dir: None,
        };
        // WHEN
        let output = executor.run(input).await.expect("execution failed");
        // THEN
        assert_eq!(output.stdout.trim(), "hello");
        assert_eq!(output.exit_code, 0);
    }

    #[tokio::test]
    async fn test_ac2_failed_command_returns_nonzero_exit_code() {
        // GIVEN
        let executor = BashExecutor::new();
        let input = BashInput {
            command: "exit 42".to_string(),
            timeout_secs: 10,
            working_dir: None,
        };
        // WHEN
        let output = executor.run(input).await.expect("should not be a Rust error");
        // THEN
        assert_eq!(output.exit_code, 42);
    }

    #[tokio::test]
    async fn test_ac3_timeout_kills_process() {
        // GIVEN
        let executor = BashExecutor::new();
        let input = BashInput {
            command: "sleep 60".to_string(),
            timeout_secs: 1,
            working_dir: None,
        };
        // WHEN
        let result = executor.run(input).await;
        // THEN
        assert!(matches!(result, Err(BashExecutorError::Timeout { .. })));
    }

    #[tokio::test]
    async fn test_ac4_empty_command_rejected_immediately() {
        // GIVEN
        let executor = BashExecutor::new();
        let input = BashInput {
            command: "".to_string(),
            timeout_secs: 10,
            working_dir: None,
        };
        // WHEN
        let result = executor.run(input).await;
        // THEN
        assert!(matches!(result, Err(BashExecutorError::EmptyCommand)));
    }

    #[tokio::test]
    async fn test_ac5_stderr_captured_separately() {
        // GIVEN
        let executor = BashExecutor::new();
        let input = BashInput {
            command: "echo error >&2".to_string(),
            timeout_secs: 10,
            working_dir: None,
        };
        // WHEN
        let output = executor.run(input).await.expect("execution failed");
        // THEN
        assert!(output.stdout.is_empty() || output.stdout.trim().is_empty());
        assert!(output.stderr.contains("error"));
    }

    #[tokio::test]
    async fn test_ac6_invalid_working_dir_returns_error() {
        // GIVEN
        let executor = BashExecutor::new();
        let input = BashInput {
            command: "echo ok".to_string(),
            timeout_secs: 10,
            working_dir: Some(std::path::PathBuf::from("/nonexistent_apollia_test_dir")),
        };
        // WHEN
        let result = executor.run(input).await;
        // THEN
        assert!(matches!(result, Err(BashExecutorError::WorkingDirNotFound(_))));
    }

    #[test]
    fn test_descriptor_is_valid() {
        // GIVEN / WHEN
        let descriptor = BashExecutor::descriptor();
        // THEN
        assert_eq!(descriptor.name, "bash_executor");
        assert!(descriptor.validate().is_ok());
    }
}
```

---

## Definition of Done

**Qualite code :**
- [ ] `cargo test -p apollia-tools` passe (0 test ignore)
- [ ] `cargo clippy -p apollia-tools -- -D warnings` : zero warning
- [ ] `cargo fmt --check` : code formate
- [ ] Zero `unwrap()` dans le code de production
- [ ] Zero `todo!()` dans le code de production
- [ ] Docstring `///` sur chaque struct, enum, et fonction publique

**Architectural :**
- [ ] Principe #2 respecte : zéro nouvelle crate tierce
- [ ] Principe #4 respecte : `EmptyCommand` rejete avant tout IO
- [ ] Mode `NoSandbox` avec warning sur macOS
- [ ] Processus zombie impossible : `child.kill()` + `child.wait()` sur timeout

**Documentation :**
- [ ] ADR-012 reference dans le commit message si le mode DevSandbox est modifie

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-tools): add bash_executor with Linux namespace sandbox`

---

## Notes d'implementation

**Decisions prises pendant l'implementation :**

**Deviations par rapport a la spec :**

**Dette technique identifiee :**

---

## Liens

- Epic parent : Sprint 2 — Tool Registry + Outils natifs
- Story precedente : STORY-011 (ToolRegistry)
- Story suivante : STORY-016 (Audit trail SQLite)
- ADR associe : ADR-012 (Mode DevSandbox sur macOS)
