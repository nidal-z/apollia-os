# Roadmap d'Implémentation — 6 Sprints, ~20 Semaines

> *Chaque sprint a un livrable démo-able. Aucun sprint n'est "infrastructure invisible" — tout produit quelque chose de montrable.*

---

## Contraintes de réalité

Ce projet est développé **soir et week-end, en parallèle d'une activité salariée**. Les estimations sont calibrées sur **8-10h de développement effectif par semaine**.

**Règle de priorisation en cas de retard :**
- **Jamais sacrifier** : AIP bridge PyO3, Tool Registry sandbox, Memory FTS5, CLI niveau 1
- **Reporter en v0.2** : ORIA Mode Orchestré, circuit breakers avancés, http_client natif
- **Reporter en v1.0** : Embedding vectoriel, gVisor, consolidation mémoire automatique

**L'objectif absolu** : Un agent Python s'exécute localement, isolé, avec mémoire persistante, opérable depuis la CLI.

---

## Sprint 0 — Fondations (semaines 1-2)

**Objectif :** Un workspace Rust qui compile proprement avec tous les types de base.

**Livrable démo-able :** `cargo build --workspace` sans erreur. `cargo test` passe.

### Tâches

- Workspace Cargo avec toutes les crates déclarées
- `apollia-core` : `AgentManifest`, `AIPTask`, `AIPInput`, `AIPResult`, `TaskStatus`, `ProcessState`, `AIPError`, `AIPPart`, `StepBudgetConfig`
- Serde JSON sur tous les types + tests sérialisation/désérialisation
- CI : `cargo fmt --check`, `cargo clippy`, `cargo test`
- `apollia.toml` template avec valeurs par défaut commentées

**Dépendances clés :**
```toml
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "1"
tracing = "0.1"
```

---

## Sprint 1 — EventBus + AgentRegistry (semaines 3-4)

**Objectif :** Les deux premiers acteurs Tokio fonctionnent et communiquent.

**Livrable démo-able :** Test d'intégration qui démarre l'EventBus, enregistre un agent fictif, et vérifie les transitions de ProcessState.

```rust
#[tokio::test]
async fn test_registry_lifecycle() {
    let bus = EventBus::new(1024);
    let registry = AgentRegistry::spawn(bus.clone()).await;
    let manifest = AgentManifest::test_fixture();
    let id = registry.register(manifest).await.unwrap();
    registry.update_state(&id, ProcessState::Active).await;
    let agent = registry.get_agent(&id).await.unwrap();
    assert_eq!(agent.process_state, ProcessState::Active);
}
```

### Tâches

**EventBus :** `tokio::sync::broadcast` + catalogue `RuntimeEvent` + wrappers + tests.

**AgentRegistry :** Acteur Tokio `mpsc::channel` + `HashMap<AgentId, AgentEntry>` + machine d'état ProcessState stricte + `AgentRegistryHandle`.

---

## Sprint 2 — Tool Registry + Outils natifs (semaines 5-7)

**Objectif :** Les 4 outils natifs core fonctionnent avec sandbox et audit trail.

**Livrable démo-able :** `bash_executor.run("echo hello")` → `stdout: hello` tracé dans SQLite.

### Tâches

- `ToolDescriptor`, `ToolKind`, `SandboxProfile`, `ToolRegistry`, `ToolResolver`
- `bash_executor` : `tokio::process::Command` + `unshare` Linux namespaces
- `python_executor` : virtualenv isolé par agent
- `file_io` : lecture/écriture sandbox, path traversal rejeté
- Audit trail SQLite (`tool_invocations`)

**Point de vigilance :** `unshare` requiert des user namespaces non-privilégiés. Tester dès ce sprint.

---

## Sprint 3 — Memory Engine (semaines 8-10)

**Objectif :** Persistance souveraine fonctionnelle avec recherche FTS5.

**Livrable démo-able :** Stocker 10 épisodes, `memory.search("devis Dupont")` → top 3 classés BM25.

### Tâches

- Schéma SQLite : `episodic_memories`, `semantic_memories`, `procedural_memories`
- FTS5 avec tokenizer `unicode61` + WAL mode + migrations versionnées
- `MemoryInterface` : `remember`, `recall`, `forget`, `record`, `history`, `search`, `purge_expired`
- `MemoryManager` : isolation par namespace
- CLI preview : `apollia-os memory inspect <namespace>`

---

## Sprint 4 — Bridge PyO3 + ORIA Mode Direct (semaines 11-14)

**Objectif :** Un agent Python s'exécute dans le runtime Rust via ORIA Mode Direct.

**Livrable démo-able :**
```bash
$ apollia-os agent start ./agents/hello_agent.py
$ apollia-os run hello-agent "Dis bonjour à Dupont SA"
✔ Terminé en 1.2s — Bonjour Dupont SA !
```

### Tâches

**Bridge PyO3 (`apollia-aip`) :**
- Init interpréteur Python, chargement module agent
- Appel `agent.manifest()` → `AgentManifest` Rust
- Appel `agent.run(task, ctx)` via `pyo3-async-runtimes` (Tokio ↔ asyncio)
- `ToolProxy` Python + `MemoryInterface` Python

**ORIA Mode Direct :**
- `Observer.enrich()` → ContextBundle + classification
- `StepBudget` tri-dimensionnel
- Boucle Direct avec supervision budget

**Runtime Core partiel :** `ExecutionCoordinator` + `TaskRouter`

**Sprint critique.** Le bridge PyO3/async est le point d'incertitude technique le plus élevé. 2 semaines de buffer prévues.

---

## Sprint 5 — APIServer + CLI complète (semaines 15-17)

**Objectif :** Le runtime est entièrement opérable sans modifier le code.

**Livrable démo-able :**
```bash
$ apollia-os start
$ apollia-os agent start ./agents/devis_agent.py
$ apollia-os run devis-agent "Devis Dupont SA, 5 jours à 850€"
$ apollia-os status
$ apollia-os audit
$ apollia-os stop
```

### Tâches

- `APIServer` axum : routes REST complètes + Unix socket + TCP + SSE streaming
- CLI `clap` derive : toutes commandes niveau 1 + 2, flags globaux, TTY auto-détection, exit codes
- `Supervisor` + Graceful Shutdown : séquence ordonnée, `SIGTERM`/`SIGINT`, drain 30s

---

## Sprint 6 — Hardening + Agent de démo (semaines 18-20)

**Objectif :** Runtime prêt pour une démo client réelle.

**Livrable démo-able :** Agent `devis-generator` en démo live — fichiers client, calcul, devis JSON, mémoire persistante. Tout local, zéro cloud.

### Tâches

- Agent `devis-generator` complet : `file_io` + `python_executor` + `ctx.memory`
- `ResilienceLayer` : circuit breaker par outil + retry backoff + classification Transient/Permanent
- Tests d'intégration end-to-end :
  ```
  test_hello_agent.rs · test_devis_workflow.rs
  test_memory_persistence.rs · test_tool_sandbox.rs · test_graceful_shutdown.rs
  ```
- Documentation README : installation, premier agent en 5 min, `apollia.toml` commenté

---

## Tableau de bord

| Sprint | Semaines | Crates | Livrable démo-able | Risque principal |
|---|---|---|---|---|
| 0 | 1-2 | core | `cargo build` propre | Choix dépendances |
| 1 | 3-4 | runtime | EventBus + Registry testés | Architecture acteurs |
| 2 | 5-7 | tools | `bash_executor` sandboxé | User namespaces Linux |
| 3 | 8-10 | memory | FTS5 search fonctionnel | SQLite concurrence |
| **4** | **11-14** | **aip, oria** | **Agent Python s'exécute** | **Bridge PyO3 async** |
| 5 | 15-17 | runtime, cli | Démo CLI complète | Intégration APIServer |
| 6 | 18-20 | oria, agents | Démo PME réelle | LLM Reasoner local |

**Sprint 4 est le sprint critique.** Tout le reste dépend du succès du bridge PyO3.

---

## Roadmap post-v0.1

### v0.2 (mois 6-9)
- `http_client` natif avec whitelist réseau
- `mcp_consumer` : connexion serveurs MCP stdio et HTTP
- ORIA Mode Orchestré avec Reasoner LLM
- Wrappers LangGraph et CrewAI
- Sandbox v0.2 : `nsjail` (seccomp-BPF)

### v0.3 (mois 10-14)
- Embedding vectoriel optionnel (sqlite-vec + all-MiniLM-L6-v2)
- Standard empaquetage agents PyPI (`apollia-agent` tag)
- A2A AgentCard export automatique
- Protocole ACP REST complet

### v1.0 (mois 15-24)
- Marketplace agents (registre communautaire)
- Consolidation mémoire opt-in
- gVisor sandbox optionnel
- Support enterprise (SLA, déploiements managés)
