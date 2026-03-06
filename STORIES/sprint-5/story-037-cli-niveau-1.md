# [SPRINT-5][apollia-cli] CLI commandes niveau 1 (start/stop/status/run)

**ID :** STORY-037
**Sprint :** 5
**Crate cible :** `apollia-cli`
**Fichier(s) cible(s) :** `crates/apollia-cli/src/main.rs`, `crates/apollia-cli/src/commands/start.rs`, `crates/apollia-cli/src/commands/stop.rs`, `crates/apollia-cli/src/commands/status.rs`, `crates/apollia-cli/src/commands/run.rs`, `crates/apollia-cli/src/client.rs`
**Taille :** L
**Depend de :** STORY-039 (Supervisor), STORY-040 (Graceful shutdown)
**Statut :** ✅ Terminee

---

## User Story

```
En tant qu'operateur PME,
je veux demarrer, arreter, surveiller et executer des agents via la CLI,
afin d'operer le runtime au quotidien sans toucher au code.
```

---

## Contexte technique

Les commandes niveau 1 sont les operations quotidiennes de l'operateur. `start` demarre le runtime en daemon, `stop` l'arrete proprement, `status` affiche l'etat, `run` soumet une tache et attend le resultat. La CLI communique avec le runtime via un client HTTP Unix socket vers l'APIServer (STORY-033).

**Principe(s) architectural(aux) concerne(s) :**
- Principe #8 — CLI humaine, API machine (`--json` sur toutes les commandes, TTY auto-detecte)
- Principe #1 — Local-first (Unix socket par defaut)

**Position dans l'architecture :**
```
apollia-os start/stop/status/run  ← cette story
    └── Client HTTP Unix socket
          └── APIServer (STORY-033)
                └── Supervisor (STORY-039)
```

---

## Criteres d'Acceptation

### AC-1 — `apollia-os start` demarre le runtime

```
ETANT DONNE le runtime non demarre
QUAND `apollia-os start` est execute
ALORS le Supervisor demarre tous les acteurs en sequence
ET la CLI affiche la progression :
  ✔ EventBus        ready
  ✔ AgentRegistry   ready
  ...
  ✔ Runtime ready in X.Xs
ET le processus reste en foreground (pas de daemon pour le MVP)
```

### AC-2 — `apollia-os stop` arrete le runtime

```
ETANT DONNE un runtime demarre
QUAND `apollia-os stop` est execute (depuis un autre terminal)
ALORS une requete POST /api/v1/shutdown est envoyee via Unix socket
ET la CLI affiche "Runtime stopping..." puis "Runtime stopped."
```

### AC-3 — `apollia-os status` affiche l'etat

```
ETANT DONNE un runtime demarre avec 1 agent ACTIVE
QUAND `apollia-os status` est execute
ALORS la CLI affiche :
  AGENTS (1 active)
  NOM              STATE    TASKS
  hello-agent      ● active 3

  TASKS IN PROGRESS (0)
```

### AC-4 — `apollia-os status --json` retourne du JSON

```
ETANT DONNE un runtime demarre
QUAND `apollia-os status --json` est execute
ALORS la sortie est un JSON valide parsable par un script
ET contient les memes informations que le format texte
```

### AC-5 — `apollia-os run <agent> <input>` soumet et attend

```
ETANT DONNE un runtime demarre et un agent "hello-agent" ACTIVE
QUAND `apollia-os run hello-agent "Bonjour"` est execute
ALORS la CLI soumet la tache via POST /api/v1/tasks
ET attend la completion via SSE ou polling
ET affiche le resultat : "✔ Completed in X.Xs (N steps)"
```

### AC-6 — `apollia-os run --stream` affiche la progression

```
ETANT DONNE un runtime demarre
QUAND `apollia-os run hello-agent "Bonjour" --stream` est execute
ALORS la CLI affiche chaque step en temps reel via SSE :
  → Task t-001 submitted
  ⠿ Step 1: ...
  ⠿ Step 2: tool_call file_io
  ✔ Completed in 3.1s (2 steps, 1 tool call)
```

### AC-7 — CLI sans runtime retourne une erreur claire

```
ETANT DONNE le runtime non demarre (socket inexistant)
QUAND `apollia-os status` est execute
ALORS exit code 2 avec message "Error: runtime not started (connection refused)"
```

### AC-8 — Exit codes POSIX

```
ETANT DONNE les differentes situations d'erreur
QUAND la CLI est executee
ALORS les exit codes suivent la convention :
  0 — Success
  1 — General error
  2 — Runtime error (not started)
  3 — Task failed (run avec tache en echec)
  4 — Timeout
```

---

## Specification technique

### Types a creer

```rust
/// Client HTTP pour communiquer avec le runtime via Unix socket.
pub struct RuntimeClient {
    socket_path: PathBuf,
}

/// Exit codes POSIX.
pub mod exit_codes {
    pub const SUCCESS: i32 = 0;
    pub const GENERAL_ERROR: i32 = 1;
    pub const RUNTIME_ERROR: i32 = 2;
    pub const TASK_FAILED: i32 = 3;
    pub const TIMEOUT: i32 = 4;
}
```

### Dependances Cargo

```toml
# crates/apollia-cli/Cargo.toml
[dependencies]
# Client HTTP pour Unix socket
hyper = { workspace = true }
hyper-util = { workspace = true, features = ["tokio", "client-legacy"] }
http-body-util = "0.1"
tokio = { workspace = true, features = ["full"] }
serde_json = { workspace = true }
```

### Comportement attendu

1. **`start`** : instancie le Supervisor (STORY-039), appelle `supervisor.start()`, bloque en foreground. Ctrl+C → graceful shutdown (STORY-040)
2. **`stop`** : cree un `RuntimeClient`, envoie POST `/api/v1/shutdown`, attend la confirmation ou timeout 5s
3. **`status`** : cree un `RuntimeClient`, GET `/api/v1/agents` + `/api/v1/health`, formate la sortie (TTY = couleurs, `--json` = JSON)
4. **`run`** : cree un `RuntimeClient`, POST `/api/v1/tasks`, puis GET `/api/v1/tasks/{id}/stream` (SSE) ou polling, affiche le resultat
5. **`RuntimeClient`** : wrappeur HTTP utilisant `tokio::net::UnixStream` pour se connecter au socket. Support `--socket` pour custom path.

### Ce que cette story N'implemente PAS

- Le mode daemon (background) pour `start` — foreground pour le MVP
- La commande `restart` — hors scope MVP
- Les flags `--timeout` et `--quiet` — hors scope MVP (ajout facile post-MVP)
- Les couleurs ANSI et spinners — simplifie pour le MVP (texte brut)
- Les commandes niveau 2 (agent, task, tools, memory, audit) — STORY-038

---

## Tests requis

### Tests unitaires

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parses_start_command() {
        // GIVEN "apollia-os start"
        // WHEN parse
        // THEN Commands::Start
    }

    #[test]
    fn test_cli_parses_stop_command() {
        // GIVEN "apollia-os stop"
        // WHEN parse
        // THEN Commands::Stop
    }

    #[test]
    fn test_cli_parses_status_command() {
        // GIVEN "apollia-os status"
        // WHEN parse
        // THEN Commands::Status
    }

    #[test]
    fn test_cli_parses_status_json_flag() {
        // GIVEN "apollia-os status --json"
        // WHEN parse
        // THEN Commands::Status avec json=true
    }

    #[test]
    fn test_cli_parses_run_command() {
        // GIVEN "apollia-os run hello-agent Bonjour"
        // WHEN parse
        // THEN Commands::Run avec agent_id et input
    }

    #[test]
    fn test_cli_parses_run_stream_flag() {
        // GIVEN "apollia-os run hello-agent Bonjour --stream"
        // WHEN parse
        // THEN Commands::Run avec stream=true
    }

    #[tokio::test]
    async fn test_runtime_client_connection_refused() {
        // GIVEN un socket inexistant
        // WHEN RuntimeClient.health()
        // THEN erreur connection refused
    }

    #[test]
    fn test_exit_codes_constants() {
        // GIVEN les exit codes
        // THEN ils suivent la convention POSIX
        assert_eq!(exit_codes::SUCCESS, 0);
        assert_eq!(exit_codes::RUNTIME_ERROR, 2);
    }
}
```

---

## Definition of Done

**Qualite code :**
- [ ] `cargo test -p apollia-cli` passe
- [ ] `cargo clippy -p apollia-cli -- -D warnings` : zero warning
- [ ] `cargo fmt --check` : code formate
- [ ] Zero `unwrap()` en production
- [ ] Docstring `///` sur chaque type/fn publique

**Architectural :**
- [ ] La CLI ne contient aucune logique metier — delegation pure au RuntimeClient
- [ ] `--json` fonctionne sur toutes les commandes niveau 1

**Commit :**
- [ ] `feat(apollia-cli): add level-1 commands (start/stop/status/run)`

---

## Liens

- Story precedente : STORY-039 (Supervisor), STORY-040 (Shutdown)
- Story suivante : STORY-038 (CLI niveau 2)
- Spec : `docs/Briques-CLI.md`
