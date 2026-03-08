# Apollia OS — Wiki

> Runtime open-source Rust pour l'exécution souveraine d'agents IA autonomes.
> Local-first. Zéro cloud. Un binaire.

---

## Démarrage rapide

| | |
|---|---|
| [Installation en 5 min](./INSTALL-Quickstart) | Rust + Python + `cargo build` |
| [Premier agent en 5 min](./Agents-Quickstart) | `manifest()` + `async run()` — c'est tout |

---

## Guides Agents

| Document | Audience |
|---|---|
| [Tutoriel Hello Agent](./Agents-Tutoriel-Hello-Agent) | Débutant — pas à pas avec explications |
| [RuntimeContext Guide](./Agents-RuntimeContext-Guide) | Intermédiaire — `ctx.tools`, `ctx.memory`, `ctx.step_budget` |
| [Adapter LangGraph / CrewAI](./Agents-Adapter-Existants) | Intermédiaire — adapter un agent existant |
| [Bonnes pratiques](./Agents-Bonnes-Pratiques) | Avancé — StepBudget, coûts LLM, résilience |
| [Troubleshooting](./Agents-Troubleshooting) | Tous — par symptôme |

---

## Référence AIP

| Document | Description |
|---|---|
| [AIP Specification](./Briques-AIP-Specification) | Contrat complet : AgentManifest, AIPTask, AIPResult, RuntimeContext |

---

## Architecture & Principes

| Document | Description |
|---|---|
| [Principes Architecturaux](./Architecture-Principes) | Les 8 décisions fondamentales non-négociables |
| [Vue d'ensemble technique](./Architecture-Vue-Ensemble) | Stack, workspace 7 crates, Agent Interface Protocol |
| [Modèle Acteur Tokio](./Architecture-Modele-Acteur) | 6 acteurs, pattern Handle, séquence démarrage |
| [Machines d'état](./Architecture-Machines-Etat) | ProcessState vs TaskState — la distinction critique |
| [Protocoles & Standards](./Architecture-Protocoles-Standards) | MCP, A2A, ACP — alignement avec l'écosystème |

---

## Briques Fondamentales

| Document | Description |
|---|---|
| [Tool Registry](./Briques-Tool-Registry) | Catalogue d'outils, sandbox, audit trail |
| [Memory Engine](./Briques-Memory-Engine) | SQLite, FTS5, épisodique / sémantique / procédural |
| [ORIA Engine](./Briques-ORIA-Engine) | Observer-Reasoner-Actor, StepBudget, ResilienceLayer |
| [Runtime Core](./Briques-Runtime-Core) | Supervision, routing, API, EventBus |
| [Apollia CLI](./Briques-CLI) | Interface d'administration et de debug |

---

## Installation & Configuration

| Document | Description |
|---|---|
| [Installation complète](./INSTALL) | Prérequis, build, macOS PyO3, dépannage |
| [Production Linux](./INSTALL-Production) | systemd, sécurité, mise à jour |
| [apollia.toml](./Config-apollia-toml) | Toutes les options de configuration |

---

## API & Intégration

| Document | Description |
|---|---|
| [API HTTP](./API-HTTP-Reference) | Endpoints REST, SSE, codes d'erreur |
| [MCP Integration](./MCP-Integration) | Consommer des serveurs MCP depuis les agents |
| [A2A / ACP](./A2A-ACP-Alignement) | Alignement avec les standards émergents |

---

## Sécurité

| Document | Description |
|---|---|
| [Local-First](./Securite-Local-First) | Garantie de souveraineté des données |
| [Sandbox Isolation](./Securite-Sandbox-Isolation) | Linux namespaces, unshare, SandboxProfile |
| [Guardrails](./Securite-Guardrails) | StepBudget, circuit breakers, RetryPolicy |

---

## Ops & Roadmap

| Document | Description |
|---|---|
| [Exploitation & Debug](./Ops-Exploitation-et-Debug) | Monitoring, diagnostics, logs |
| [Roadmap](./Roadmap-Implementation) | v0.1 → v0.2 → v1.0 |
| [Décisions Architecturales](./Decisions-Log) | ADR-001 à ADR-019 |

---

## En 3 lignes

Apollia OS est un **runtime Rust open-source** qui permet à n'importe quel agent IA Python (CrewAI, LangGraph, custom) de s'exécuter de manière **isolée, souveraine, et outillée** — sans dépendance cloud, sans Docker obligatoire, avec mémoire persistante locale.

**État :** v0.1.0 en préparation — 342 tests, MVP validé (mars 2026)

---

*Auteur : Nidal — CTO & Co-fondateur Apollia*
*Dernière mise à jour : mars 2026*
