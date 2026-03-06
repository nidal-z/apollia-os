# [SPRINT-5][apollia-cli] CLI commandes niveau 2 (agent/task/tools/memory/audit)

**ID :** STORY-038
**Sprint :** 5
**Crate cible :** `apollia-cli`
**Fichier(s) cible(s) :** `crates/apollia-cli/src/commands/agent.rs`, `crates/apollia-cli/src/commands/task.rs`, `crates/apollia-cli/src/commands/tools.rs`, `crates/apollia-cli/src/commands/audit.rs`
**Taille :** L
**Depend de :** STORY-037 (CLI niveau 1)
**Statut :** ✅ Terminee

---

## User Story

```
En tant que developpeur d'agent,
je veux des commandes CLI detaillees pour gerer agents, taches, outils, memoire et audit,
afin de debugger et operer le runtime de maniere granulaire.
```

---

## Contexte technique

Les commandes niveau 2 completent la CLI avec des operations avancees. Elles suivent le pattern `noun verb` (ADR-008) et communiquent avec l'APIServer via le `RuntimeClient` (STORY-037). La commande `memory` existe deja (STORY-023) — elle sera conservee et integree.

**Principe(s) architectural(aux) concerne(s) :**
- Principe #8 — CLI humaine, API machine (pattern noun-verb, `--json`)

**Position dans l'architecture :**
```
apollia-os agent list|start|stop|info  ← cette story
apollia-os task list|status|cancel
apollia-os tools list|describe
apollia-os audit [list]|stats
    └── RuntimeClient (STORY-037)
          └── APIServer routes (STORY-034, 035)
```

---

## Criteres d'Acceptation

### AC-1 — `apollia-os agent list` affiche les agents

```
ETANT DONNE un runtime avec 2 agents
QUAND `apollia-os agent list` est execute
ALORS la CLI affiche un tableau avec agent_id, state, tasks_completed
ET `--json` retourne la meme info en JSON
```

### AC-2 — `apollia-os agent start <path>` demarre un agent

```
ETANT DONNE un module Python valide
QUAND `apollia-os agent start /path/to/agent.py` est execute
ALORS POST /api/v1/agents est appele
ET la CLI affiche "Agent hello-agent started (initializing)"
```

### AC-3 — `apollia-os agent stop <id>` arrete un agent

```
ETANT DONNE un agent "hello-agent" ACTIVE
QUAND `apollia-os agent stop hello-agent` est execute
ALORS DELETE /api/v1/agents/hello-agent est appele
ET la CLI affiche "Agent hello-agent stopping"
```

### AC-4 — `apollia-os agent info <id>` affiche le detail

```
ETANT DONNE un agent "hello-agent" enregistre
QUAND `apollia-os agent info hello-agent` est execute
ALORS GET /api/v1/agents/hello-agent est appele
ET la CLI affiche le manifest, l'etat, le nombre de taches
```

### AC-5 — `apollia-os task list` affiche les taches

```
ETANT DONNE des taches en cours et terminees
QUAND `apollia-os task list` est execute
ALORS la CLI affiche un tableau avec task_id, agent_id, status, duration
```

### AC-6 — `apollia-os task cancel <id>` annule une tache

```
ETANT DONNE une tache "t-001" en cours
QUAND `apollia-os task cancel t-001` est execute
ALORS DELETE /api/v1/tasks/t-001 est appele
ET la CLI affiche "Task t-001 canceled"
```

### AC-7 — `apollia-os tools list` affiche les outils

```
ETANT DONNE un runtime avec des outils enregistres
QUAND `apollia-os tools list` est execute
ALORS GET /api/v1/tools est appele
ET la CLI affiche la liste des outils avec leur type (Native/MCP)
```

### AC-8 — `apollia-os audit` affiche les derniers events

```
ETANT DONNE des tool invocations dans l'audit trail
QUAND `apollia-os audit` est execute
ALORS GET /api/v1/audit est appele
ET la CLI affiche les derniers 20 events par defaut
```

### AC-9 — Commande existante `memory` conservee

```
ETANT DONNE la commande `apollia-os memory inspect` existante
QUAND `apollia-os memory inspect <ns>` est execute
ALORS le comportement est identique a STORY-023
ET la commande est integree dans la structure CLI unifiee
```

---

## Specification technique

### Types a creer

```rust
/// Agent subcommands: apollia-os agent <verb>
#[derive(Debug, clap::Subcommand)]
pub enum AgentCommand {
    List,
    Start { path: String },
    Stop { agent_id: String },
    Info { agent_id: String },
}

/// Task subcommands: apollia-os task <verb>
#[derive(Debug, clap::Subcommand)]
pub enum TaskCommand {
    List,
    Status { task_id: String },
    Cancel { task_id: String },
}

/// Tools subcommands: apollia-os tools <verb>
#[derive(Debug, clap::Subcommand)]
pub enum ToolsCommand {
    List,
    Describe { tool_name: String },
}

/// Audit subcommands: apollia-os audit [verb]
#[derive(Debug, clap::Subcommand)]
pub enum AuditCommand {
    /// List recent audit events (default).
    #[command(name = "list")]
    List {
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    Stats,
}
```

### Dependances Cargo

Aucune nouvelle au-dela de STORY-037.

### Comportement attendu

1. Chaque commande niveau 2 est un module dans `commands/`
2. Toutes les commandes utilisent `RuntimeClient` (STORY-037) pour appeler l'APIServer
3. Toutes les commandes supportent `--json` pour la sortie machine
4. Le format texte est un tableau aligne pour les listes, detail pour les info/status
5. La commande `memory` existante (STORY-023) est preservee dans `commands/memory.rs`

### Ce que cette story N'implemente PAS

- `agent validate` (validation offline sans runtime) — hors scope MVP
- `agent logs` (streaming logs agent) — hors scope MVP
- `task retry` / `task resume` — Sprint 6
- `tools register` / `tools unregister` (modification dynamique) — hors scope MVP
- `tools test` / `tools reset-circuit` — Sprint 6 (ResilienceLayer)
- `memory export` / `memory import` — hors scope MVP
- `audit export` — hors scope MVP

---

## Tests requis

### Tests unitaires

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parses_agent_list() {
        // GIVEN "apollia-os agent list"
        // WHEN parse
        // THEN Commands::Agent { command: AgentCommand::List }
    }

    #[test]
    fn test_cli_parses_agent_start() {
        // GIVEN "apollia-os agent start /path/to/agent.py"
        // WHEN parse
        // THEN Commands::Agent { command: AgentCommand::Start { path } }
    }

    #[test]
    fn test_cli_parses_task_cancel() {
        // GIVEN "apollia-os task cancel t-001"
        // WHEN parse
        // THEN Commands::Task { command: TaskCommand::Cancel { task_id: "t-001" } }
    }

    #[test]
    fn test_cli_parses_tools_list() {
        // GIVEN "apollia-os tools list"
        // WHEN parse
        // THEN Commands::Tools { command: ToolsCommand::List }
    }

    #[test]
    fn test_cli_parses_audit_default() {
        // GIVEN "apollia-os audit list"
        // WHEN parse
        // THEN Commands::Audit { command: AuditCommand::List { limit: 20 } }
    }

    #[test]
    fn test_cli_parses_audit_stats() {
        // GIVEN "apollia-os audit stats"
        // WHEN parse
        // THEN Commands::Audit { command: AuditCommand::Stats }
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
- [ ] Pattern noun-verb respecte (ADR-008)
- [ ] `--json` fonctionne sur toutes les commandes niveau 2
- [ ] Commande `memory` existante non cassee

**Commit :**
- [ ] `feat(apollia-cli): add level-2 commands (agent/task/tools/audit)`

---

## Liens

- Story precedente : STORY-037 (CLI niveau 1)
- Story memory existante : STORY-023
- Spec : `docs/Briques-CLI.md`
