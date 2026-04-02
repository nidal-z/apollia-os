# Annexe D — Roadmap

---

## v0.1 — Foundation (mars 2026) ✅

**Focus :** Runtime stable, agent Python fonctionnel, CLI opérationnelle.

| Composant | Statut |
|---|---|
| Workspace Rust 7 crates | ✅ |
| AIP bridge PyO3 (duck typing Python) | ✅ |
| Tool Registry + sandbox Linux namespaces | ✅ |
| Memory Engine SQLite + FTS5 | ✅ |
| ORIA Engine Mode Direct + StepBudget | ✅ |
| ResilienceLayer circuit breakers | ✅ |
| API REST locale (axum) + SSE streaming | ✅ |
| CLI complète niveau 1 + niveau 2 | ✅ |
| Supervisor + graceful shutdown | ✅ |
| ORIA Mode Orchestré (plans multi-étapes) | ✅ |
| HITL suspend/resume | ✅ |
| A2A (Agent-to-Agent) via SkillIndex | ✅ |
| Pipelines DAG multi-agents | ✅ |
| Triggers (cron, webhook, filewatch) | ✅ |
| Chat hybride : Libre + Agent | ✅ |
| Application Desktop (Tauri v2 + Svelte 5) | ✅ |
| Worker Agent pattern + registre communautaire | ✅ |
| 342+ tests + CI GitHub Actions | ✅ |

**Livrable :** `cargo install apollia-os` — premier agent en 5 minutes.

---

## v0.2 — Connectivity (prévue Q3 2026)

**Focus :** Interopérabilité standards, résilience production.

- MCP consumer natif amélioré (connexion serveurs MCP stdio/HTTP+SSE)
- Wrappers officiels LangGraph / CrewAI / AutoGen stables
- Embedding vectoriel optionnel (sqlite-vec + GGUF local)
- `http_client` outil natif avec whitelist réseau configurable
- Distribution binaires pour Windows

---

## v0.3 — Ecosystem (prévue Q1 2027)

**Focus :** Marketplace agents, standard empaquetage PyPI.

- Standard empaquetage agents PyPI (`apollia-agent` tag)
- A2A AgentCard automatique + discovery inter-runtimes
- Exposition comme serveur MCP (Apollia OS en tant que fournisseur d'outils)
- Registre communautaire agents versionné

---

## v1.0 — Enterprise (prévue 2027)

**Focus :** Production enterprise, marketplace stable.

- Support enterprise (SLA, support prioritaire)
- gVisor sandbox optionnel (isolation renforcée)
- Consolidation mémoire opt-in
- Apollia Cloud (déploiement managé optionnel)
- Multi-tenancy pour équipes

---

*Cette roadmap est indicative. Les priorités évoluent avec les retours de la communauté.*
