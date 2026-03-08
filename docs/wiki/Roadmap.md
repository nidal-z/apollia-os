# Roadmap — Apollia OS

> Pour la roadmap interne détaillée : voir `docs/internal/roadmap/`.

## v0.1 — Foundation (mars 2026)

**Focus :** Runtime stable, agent Python fonctionnel, CLI opérationnelle.

- ✅ Workspace Rust 7 crates
- ✅ AIP bridge PyO3 (duck typing Python)
- ✅ Tool Registry + sandbox Linux namespaces
- ✅ Memory Engine SQLite + FTS5
- ✅ ORIA Engine Mode Direct + StepBudget
- ✅ ResilienceLayer circuit breakers
- ✅ API REST locale (axum) + SSE streaming
- ✅ CLI complète niveau 1 + niveau 2
- ✅ Supervisor + graceful shutdown
- ✅ 342 tests + CI GitHub Actions

**Livrable :** `cargo install apollia-os` — premier agent en 5 minutes.

---

## v0.2 — Connectivity (prévue Q3 2026)

**Focus :** Interopérabilité standards, résilience production.

- MCP consumer natif (connexion serveurs MCP stdio/HTTP+SSE)
- ORIA Mode Orchestré (tâches multi-étapes complexes)
- Wrappers officiels LangGraph / CrewAI / AutoGen
- `http_client` outil natif (avec whitelist réseau)
- Embedding vectoriel optionnel (sqlite-vec + GGUF local)

---

## v0.3 — Ecosystem (prévue Q1 2027)

**Focus :** Marketplace agents, standard empaquetage PyPI.

- Standard empaquetage agents PyPI (`apollia-agent` tag)
- A2A AgentCard automatique + discovery
- Exposition comme serveur MCP
- Registre communautaire agents

---

## v1.0 — Enterprise (prévue 2027)

**Focus :** Production enterprise, marketplace stable.

- Support enterprise (SLA)
- gVisor sandbox optionnel
- Consolidation mémoire opt-in
- Apollia Cloud (déploiement managé optionnel)

---

*Cette roadmap est indicative. Les priorités évoluent avec les retours de la communauté.*
*Dernière mise à jour : mars 2026*
