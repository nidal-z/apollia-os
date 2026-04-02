# Annexe E — Historique des sprints

Vue condensée des 32+ sprints de développement d'Apollia OS — ce qui a été livré et les décisions architecturales clés prises en cours de route.

---

## Tableau de bord

| Sprint | Nom | Statut | Stories |
|---|---|---|---|
| 0 | Fondations | ✅ | 5/5 |
| 1 | EventBus + AgentRegistry | ✅ | 4/4 |
| 2 | Tool Registry + Outils natifs | ✅ | 7/7 |
| 3 | Memory Engine | ✅ | 7/7 |
| 4 | Bridge PyO3 + ORIA Direct | ✅ | — |
| 5 | CLI + API REST + Supervisor | ✅ | — |
| 6 | ResilienceLayer + Circuit Breakers | ✅ | — |
| 7 | ORIA Mode Orchestré V1 | ✅ | — |
| 8 | HITL Suspend/Resume | ✅ | — |
| 9 | Notifications + Triggers V1 | ✅ | — |
| 10 | Plans SQLite + task inspect | ✅ | — |
| 11 | Pipelines DAG V1 | ✅ | — |
| 12 | MCP Consumer | ✅ | — |
| 13 | Embedding vectoriel SQLite | ✅ | — |
| 14 | Desktop Tauri V1 (squelette) | ✅ | — |
| 15 | Worker Agent Pattern V1 | ✅ | — |
| 16 | A2A V1 (SkillIndex + ctx.delegate) | ✅ | — |
| 17 | Chat Libre (Rust ReAct) | ✅ | — |
| 18 | Chat Agent + HITL inline | ✅ | — |
| 19 | Desktop complet (46 IPC commandes) | ✅ | — |
| 20 | STT (Whisper embarqué) | ✅ | — |
| 21 | SDK Python (apollia_os) | ✅ | — |
| 22 | Onboarding conversationnel | ✅ | — |
| 23 | Pipelines V2 (fan-out, conditions, fallback) | ✅ | — |
| 24 | Triggers V2 (FileWatch, Webhook HMAC) | ✅ | — |
| 25 | Outils fichiers refactoring (ADR-043) | ✅ | — |
| 26 | User Memory (__user__ namespace) | ✅ | — |
| 27 | Observabilité + timeline unifiée | ✅ | — |
| 28 | Config simplification (apollia.toml) | ✅ | — |
| 29 | Dashboard + SSE stores Svelte | ✅ | — |
| 30 | A2A routing V1 (skills dynamiques) | ✅ | — |
| 31 | Worker Agents V2 | ✅ | — |
| 32 | A2A complet + Distribution locale + Registre communautaire | ✅ | 8/8 |

---

## Décisions architecturales clés

Les ADR (Architecture Decision Records) complets sont dans `docs/adr/`. Voici les décisions qui ont le plus marqué l'évolution du projet :

| ADR | Décision | Sprint | Impact |
|---|---|---|---|
| ADR-003 | AIP duck typing (pas de classe de base) | 4 | Adoption zéro friction |
| ADR-010 | Pivot SaaS → runtime Rust open-source | — | Refondation totale |
| ADR-011 | `AgentId`/`TaskId` comme aliases String | 1 | Simplification types |
| ADR-012 | `SandboxMode::Dev` macOS via `#[cfg]` | 2 | Support macOS dev |
| ADR-027 | Runtime embarqué dans Desktop (processus unique) | 14 | Architecture Desktop |
| ADR-033 | Config structurelle TOML + config opérationnelle SQLite | 28 | Hot reload triggers/pipelines |
| ADR-039 | Sliding window + résumé LLM pour contexte chat | 18 | Mémoire chat bornée |
| ADR-043 | Éclatement `file_io` en 6 outils spécialisés | 25 | Ergonomie outils fichiers |
| ADR-050 | Distribution Worker Agents : bundled + registre communautaire | 32 | Écosystème agents |

---

## Ce qui a changé en cours de route

**Ajouté sans être planifié initialement :**
- Mode Chat hybride (Libre + Agent) — demande forte de la communauté
- STT embarqué (whisper-rs) — différenciateur pour usage offline
- Application Desktop Tauri — meilleure UX pour non-développeurs
- A2A avec SkillIndex dynamique — émergence du standard Google A2A pendant le développement

**Reporté à v0.2+ :**
- Consolidation mémoire automatique (trop de risques de coûts LLM incontrôlés)
- gVisor sandbox renforcé (complexité, besoin réel pas encore validé)
- Multi-tenancy (hors scope MVP)
- Traduction anglaise du book (sprint séparé futur)
