# [SPRINT-5][apollia-runtime] APIServer axum Unix socket + TCP

**ID :** STORY-033
**Sprint :** 5
**Crate cible :** `apollia-runtime`
**Fichier(s) cible(s) :** `crates/apollia-runtime/src/api/mod.rs`, `crates/apollia-runtime/src/api/server.rs`
**Taille :** L
**Depend de :** Sprint 4 ✅ (EventBus, AgentRegistry, TaskRouter, ExecutionCoordinator)
**Statut :** ✅ Terminee

---

## User Story

```
En tant que runtime,
je veux un serveur HTTP axum ecoutant sur Unix socket et TCP simultanement,
afin que la CLI locale et les SDK externes puissent communiquer avec le runtime via une API REST.
```

---

## Contexte technique

Le runtime a besoin d'une surface API pour etre operable sans modifier le code. L'APIServer est le point d'entree unique pour toutes les commandes externes (CLI, SDK Python, futur Apollia Workspace). Il ecoute sur deux interfaces : Unix socket (`/tmp/apollia.sock`) pour la CLI locale (rapide, securise par permissions fichier) et TCP `localhost:7771` pour les SDK et integrations tierces.

**Principe(s) architectural(aux) concerne(s) :**
- Principe #1 — Local-first (Unix socket par defaut, TCP sur localhost uniquement)
- Principe #8 — CLI humaine, API machine (`--json` global, routes REST JSON)

**Position dans l'architecture :**
```
CLI / SDK Python
    |
    v
APIServer (axum)  ← cette story
    ├── Unix socket /tmp/apollia.sock
    └── TCP localhost:7771
    |
    v
TaskRouter / AgentRegistry / EventBus (Sprint 1-4)
```

---

## Criteres d'Acceptation

### AC-1 — APIServer demarre sur TCP

```
ETANT DONNE un APIServer configure avec port 7771
QUAND start() est appele
ALORS le serveur ecoute sur localhost:7771
ET GET /api/v1/health retourne 200 {"status": "ok"}
```

### AC-2 — APIServer demarre sur Unix socket

```
ETANT DONNE un APIServer configure avec socket_path "/tmp/apollia.sock"
QUAND start() est appele
ALORS le serveur ecoute sur /tmp/apollia.sock
ET GET /api/v1/health via Unix socket retourne 200 {"status": "ok"}
ET le fichier socket est supprime au demarrage si deja present (stale socket)
```

### AC-3 — Dual listener simultane

```
ETANT DONNE un APIServer configure avec TCP + Unix socket
QUAND start() est appele
ALORS les deux listeners fonctionnent simultanement (tokio::select! ou tokio::spawn)
ET une requete sur TCP et une requete sur Unix socket retournent les memes reponses
```

### AC-4 — APIServer expose un handle arretable

```
ETANT DONNE un APIServer demarre
QUAND shutdown() est appele sur le handle
ALORS le serveur arrete d'accepter de nouvelles connexions
ET les connexions en cours sont drainees (grace a axum graceful shutdown)
ET start() retourne Ok(())
```

### AC-5 — Erreur si le port est deja utilise

```
ETANT DONNE un port TCP 7771 deja occupe par un autre processus
QUAND start() est appele
ALORS une erreur APIServerError::BindFailed est retournee avec le detail
```

### AC-6 — Shared state injectable

```
ETANT DONNE un APIServer avec un AppState contenant les handles des acteurs
QUAND une route est appelee
ALORS la route a acces a AppState via axum Extension ou State
ET AppState contient : TaskRouterHandle, AgentRegistryHandle, EventBusSender
```

---

## Specification technique

### Types a creer

```rust
/// Shared application state injected into all routes.
pub struct AppState {
    pub router_handle: TaskRouterHandle</* B */>,
    pub registry_handle: AgentRegistryHandle,
    pub event_sender: EventBusSender,
}

/// APIServer configuration.
pub struct APIServerConfig {
    pub socket_path: PathBuf,
    pub tcp_port: u16,
}

/// Handle to control the running APIServer.
pub struct APIServerHandle {
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

/// APIServer errors.
#[derive(Debug, thiserror::Error)]
pub enum APIServerError {
    #[error("failed to bind TCP on port {port}: {source}")]
    BindFailed { port: u16, source: std::io::Error },

    #[error("failed to bind Unix socket at {path}: {source}")]
    SocketBindFailed { path: String, source: std::io::Error },

    #[error("server error: {0}")]
    ServerError(String),
}
```

### Dependances Cargo

```toml
# crates/apollia-runtime/Cargo.toml — deja dans workspace deps
[dependencies]
axum = { workspace = true }
tower = { workspace = true }
tower-http = { workspace = true }
hyper-util = { workspace = true, features = ["tokio"] }
```

### Comportement attendu

1. `APIServer::new(config, app_state)` construit le serveur avec les routes et le state partage
2. `APIServer::start()` lance deux tasks Tokio en parallele : un TCP listener et un Unix socket listener
3. Les deux listeners partagent le meme `Router` axum (memes routes, meme state)
4. Un `watch::channel` permet le shutdown graceful : quand `APIServerHandle::shutdown()` est appele, les deux listeners s'arretent
5. Au demarrage, le fichier socket stale est supprime s'il existe (evite `AddrInUse`)
6. L'APIServer emet `RuntimeEvent::Ready("api_server")` sur l'EventBus une fois les deux listeners demarres

### Ce que cette story N'implemente PAS

- Les routes REST tasks/agents/SSE — STORY-034, 035, 036
- Le Supervisor qui orchestre le demarrage — STORY-039
- Le graceful shutdown SIGTERM — STORY-040
- L'authentification/autorisation sur les routes (hors scope MVP)
- Le CORS ou rate limiting (hors scope MVP)

---

## Tests requis

### Tests unitaires

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_endpoint_returns_ok() {
        // GIVEN un APIServer avec un router minimal
        // WHEN GET /api/v1/health
        // THEN 200 {"status": "ok"}
    }

    #[tokio::test]
    async fn test_tcp_listener_binds_successfully() {
        // GIVEN un port libre
        // WHEN start() est appele
        // THEN le serveur repond sur TCP
    }

    #[tokio::test]
    async fn test_unix_socket_listener_binds_successfully() {
        // GIVEN un chemin socket temporaire
        // WHEN start() est appele
        // THEN le serveur repond sur Unix socket
    }

    #[tokio::test]
    async fn test_stale_socket_cleanup() {
        // GIVEN un fichier socket existant (stale)
        // WHEN start() est appele
        // THEN le fichier stale est supprime et le bind reussit
    }

    #[tokio::test]
    async fn test_shutdown_stops_server() {
        // GIVEN un APIServer demarre
        // WHEN shutdown() est appele
        // THEN start() retourne Ok(())
    }

    #[tokio::test]
    async fn test_unknown_route_returns_404() {
        // GIVEN un APIServer demarre
        // WHEN GET /api/v1/unknown
        // THEN 404 Not Found
    }
}
```

---

## Definition of Done

**Qualite code :**
- [ ] `cargo test -p apollia-runtime` passe (0 test ignore)
- [ ] `cargo clippy -p apollia-runtime -- -D warnings` : zero warning
- [ ] `cargo fmt --check` : code formate
- [ ] Zero `unwrap()` dans le code de production
- [ ] Zero `todo!()` dans le code de production
- [ ] Docstring `///` sur chaque struct, enum, et fonction publique

**Architectural :**
- [ ] Aucune dependance externe non prevue dans l'architecture
- [ ] Pattern acteur respecte (APIServer lance des tasks Tokio, pas de Arc<Mutex> cross-acteurs)
- [ ] `hyper-util` ajoute au workspace deps si necessaire → ADR

**Commit :**
- [ ] `feat(apollia-runtime): add APIServer with dual TCP + Unix socket listeners`

---

## Notes d'implementation

**Decisions prises pendant l'implementation :**
- ADR-017 : hyper-util ajoute explicitement pour Unix socket serving (axum 0.7.9 ne supporte que TcpListener dans axum::serve)
- tower passe a features = ["util"] pour ServiceExt::oneshot() dans les tests
- AppState<B> utilise un impl Clone manuel (evite le bound B: Clone inutile)
- APIServer n'est pas generique sur B — le type parameter est consomme dans build_router() et le Router resultant est type-erased (Router<()>)

**Deviations par rapport a la spec :**
- hyper-util utilise a la place d'axum::serve() pour le Unix socket (prevu comme possibilite dans la spec)
- Pas d'emission RuntimeEvent::Ready("api_server") — sera ajoute avec le Supervisor (STORY-039)

**Dette technique identifiee :**
- Code asymetrique TCP (axum::serve) vs Unix socket (boucle manuelle hyper-util) — simplifiable quand axum 0.8 sera adopte

---

## Liens

- Epic parent : Sprint 5 — APIServer + CLI complete
- Story suivante : STORY-034 (Routes REST tasks)
- ADR associe : ADR-017
- Spec : `docs/Briques-Runtime-Core.md` (section APIServer)
