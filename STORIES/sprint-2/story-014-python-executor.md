# [Sprint 2][apollia-tools] python_executor — execution Python dans virtualenv isole

**ID :** STORY-014
**Sprint :** 2
**Crate cible :** `apollia-tools`
**Fichier(s) cible(s) :** `crates/apollia-tools/src/tools/python_executor.rs`
**Taille :** L
**Depend de :** STORY-010
**Statut :** 🔲 A faire

---

## User Story

```
En tant que runtime,
je veux un outil python_executor qui execute du code Python dans un virtualenv
dedie par agent,
afin que les agents puissent utiliser des packages Python isoles sans que leurs
environnements interfèrent les uns avec les autres.
```

---

## Contexte technique

Chaque agent possede son virtualenv sous `~/.apollia/sandboxes/<agent_id>/venv/`.
Les packages declares dans le manifest sont installes a `INITIALIZING` (pas a l'execution).
Cette separation garantit le principe "Fail fast" : une erreur d'installation echoue tot.

**Principe(s) architecturaux concernes :**
- Principe #4 — Fail fast (verification de python3 a la construction, packages installes a INITIALIZING)
- Principe #1 — Local-first (virtualenv local, pas de cloud)

**Position dans l'architecture :**
```
apollia-tools
  └── tools/
        └── python_executor.rs  <- cette story
              ├── PythonExecutor    (struct publique)
              ├── PythonInput       (parametres d'execution)
              ├── PythonOutput      (resultat)
              └── venv::setup()     (creation + installation packages)
```

---

## Criteres d'Acceptation

### AC-1 — Execution de code Python simple

```
ETANT DONNE un PythonExecutor avec un venv initialise
QUAND on execute PythonInput { code: "print('hello')", timeout_secs: 30, packages: [] }
ALORS PythonOutput { stdout: "hello\n", stderr: "", exit_code: 0 } est retourne
```

### AC-2 — Python3 absent → erreur claire a la construction

```
ETANT DONNE un systeme sans python3 disponible dans PATH
QUAND on appelle PythonExecutor::new(agent_id, venv_base_dir)
ALORS Err(PythonExecutorError::PythonUnavailable) est retourne
```

### AC-3 — Tentative d'import de package non-installe → exit_code non-zero

```
ETANT DONNE un PythonExecutor avec venv vide (aucun package extra)
QUAND on execute code = "import pandas"
ALORS PythonOutput { exit_code: 1, stderr: "ModuleNotFoundError: ...", ... } est retourne
(c'est un echec de code, pas une erreur Rust)
```

### AC-4 — Timeout hard respecte

```
ETANT DONNE un PythonExecutor
QUAND on execute code = "import time; time.sleep(60)" avec timeout_secs = 1
ALORS Err(PythonExecutorError::Timeout { timeout_secs: 1 }) est retourne
```

### AC-5 — Code vide rejete immediatement

```
ETANT DONNE un PythonExecutor
QUAND on execute PythonInput { code: "", ... }
ALORS Err(PythonExecutorError::EmptyCode) est retourne
```

### AC-6 — Isolation entre agents : les venvs ne partagent pas les packages

```
ETANT DONNE deux PythonExecutors pour agent-A et agent-B
QUAND agent-A a un venv avec "requests" installe et agent-B n'en a pas
ALORS importer "requests" reussit pour A et echoue (exit 1) pour B
```

---

## Specification technique

### Types a creer dans `crates/apollia-tools/src/tools/python_executor.rs`

```rust
/// Outil natif d'execution Python dans un virtualenv isole par agent.
pub struct PythonExecutor {
    /// Identifiant de l'agent proprietaire de ce virtualenv.
    agent_id: String,
    /// Chemin vers le virtualenv dedie : <venv_base_dir>/<agent_id>/venv/
    venv_path: PathBuf,
    /// Chemin vers l'interpreteur Python dans le venv.
    python_bin: PathBuf,
}

/// Parametres d'une invocation Python.
pub struct PythonInput {
    /// Code Python a executer.
    pub code: String,
    /// Timeout en secondes avant SIGKILL.
    pub timeout_secs: u64,
}

/// Resultat d'une invocation Python.
pub struct PythonOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
}

/// Erreurs d'invocation Python.
pub enum PythonExecutorError {
    EmptyCode,
    PythonUnavailable,
    VenvCreationFailed(String),
    PackageInstallFailed { package: String, stderr: String },
    Timeout { timeout_secs: u64 },
    SpawnFailed(String),
}

impl PythonExecutor {
    /// Cree un PythonExecutor pour un agent donne.
    ///
    /// Verifie la disponibilite de python3 dans PATH.
    /// Ne cree pas encore le virtualenv (voir setup_venv).
    pub fn new(agent_id: &str, venv_base_dir: &Path) -> Result<Self, PythonExecutorError> { ... }

    /// Cree le virtualenv et installe les packages declares.
    ///
    /// Appele a INITIALIZING. Peut prendre du temps si packages nombreux.
    pub async fn setup_venv(&self, packages: &[String]) -> Result<(), PythonExecutorError> { ... }

    /// Execute du code Python dans le virtualenv de l'agent.
    pub async fn run(&self, input: PythonInput) -> Result<PythonOutput, PythonExecutorError> { ... }

    /// Retourne le ToolDescriptor de cet outil.
    pub fn descriptor() -> ToolDescriptor { ... }
}
```

### Dependances Cargo

```toml
# Aucune nouvelle dependance
```

### Comportement attendu — Creation du virtualenv

```sh
# Etape 1 : Creer le venv (a INITIALIZING)
python3 -m venv ~/.apollia/sandboxes/<agent_id>/venv/

# Etape 2 : Installer les packages declares dans le manifest
~/.apollia/sandboxes/<agent_id>/venv/bin/pip install <package> --quiet
```

### Comportement attendu — Execution du code

Le code Python est ecrit dans un fichier temporaire (pas de `-c` pour eviter les problemes
avec les guillemets et multi-lignes) :

```sh
# Fichier temporaire : /tmp/apollia_<uuid>.py
~/.apollia/sandboxes/<agent_id>/venv/bin/python /tmp/apollia_<uuid>.py
```

Le fichier temporaire est supprime apres execution (succes ou echec).

### Ce que cette story N'implemente PAS

- Sandbox via Linux namespaces pour Python (roadmap v0.2 — utilise FileSystem profile MVP)
- Installation de packages a l'execution (interdit par design : uniquement a INITIALIZING)
- Audit trail (STORY-016)
- Gestion des packages avec version pinning (tout est latest dans le MVP)

---

## Tests requis

### Tests unitaires dans `crates/apollia-tools/src/tools/python_executor.rs`

Note : Ces tests necessitent `python3` disponible dans PATH. Marques `#[ignore]` si python3 absent.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_venv_dir() -> PathBuf {
        std::env::temp_dir().join("apollia_test_venv")
    }

    #[tokio::test]
    async fn test_ac1_simple_print_returns_stdout() {
        // GIVEN
        let executor = match PythonExecutor::new("test-agent", &test_venv_dir()) {
            Ok(e) => e,
            Err(PythonExecutorError::PythonUnavailable) => return, // skip si python3 absent
            Err(e) => panic!("unexpected error: {:?}", e),
        };
        executor.setup_venv(&[]).await.expect("venv setup failed");
        let input = PythonInput {
            code: "print('hello')".to_string(),
            timeout_secs: 30,
        };
        // WHEN
        let output = executor.run(input).await.expect("execution failed");
        // THEN
        assert_eq!(output.stdout.trim(), "hello");
        assert_eq!(output.exit_code, 0);
    }

    #[tokio::test]
    async fn test_ac4_timeout_kills_python_process() {
        // GIVEN
        let executor = match PythonExecutor::new("test-agent-timeout", &test_venv_dir()) {
            Ok(e) => e,
            Err(PythonExecutorError::PythonUnavailable) => return,
            Err(e) => panic!("{:?}", e),
        };
        executor.setup_venv(&[]).await.expect("venv setup failed");
        let input = PythonInput {
            code: "import time; time.sleep(60)".to_string(),
            timeout_secs: 1,
        };
        // WHEN
        let result = executor.run(input).await;
        // THEN
        assert!(matches!(result, Err(PythonExecutorError::Timeout { .. })));
    }

    #[tokio::test]
    async fn test_ac5_empty_code_rejected() {
        // GIVEN
        let executor = match PythonExecutor::new("test-agent-empty", &test_venv_dir()) {
            Ok(e) => e,
            Err(PythonExecutorError::PythonUnavailable) => return,
            Err(e) => panic!("{:?}", e),
        };
        executor.setup_venv(&[]).await.unwrap();
        let input = PythonInput { code: "".to_string(), timeout_secs: 10 };
        // WHEN / THEN
        assert!(matches!(executor.run(input).await, Err(PythonExecutorError::EmptyCode)));
    }

    #[test]
    fn test_descriptor_is_valid() {
        let descriptor = PythonExecutor::descriptor();
        assert_eq!(descriptor.name, "python_executor");
        assert!(descriptor.validate().is_ok());
    }
}
```

---

## Definition of Done

**Qualite code :**
- [ ] `cargo test -p apollia-tools` passe (0 test ignore, tests python marques `#[ignore]` si python3 absent)
- [ ] `cargo clippy -p apollia-tools -- -D warnings` : zero warning
- [ ] `cargo fmt --check` : code formate
- [ ] Zero `unwrap()` dans le code de production
- [ ] Zero `todo!()` dans le code de production
- [ ] Docstring `///` sur chaque struct, enum, et fonction publique

**Architectural :**
- [ ] Fichier temporaire supprime apres execution (pas de fuite)
- [ ] Principe #4 : `PythonUnavailable` detecte a la construction, pas a l'execution
- [ ] Principe #1 : tout local, aucun package downloade a l'execution

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-tools): add python_executor with per-agent virtualenv isolation`

---

## Notes d'implementation

**Decisions prises pendant l'implementation :**

**Deviations par rapport a la spec :**

**Dette technique identifiee :**

---

## Liens

- Epic parent : Sprint 2 — Tool Registry + Outils natifs
- Story precedente : STORY-013 (bash_executor)
- Story suivante : STORY-012 (ToolResolver)
- ADR associe : ADR-012 (Mode DevSandbox sur macOS — meme pattern que bash_executor)
