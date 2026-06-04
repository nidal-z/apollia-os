# Log des Décisions Architecturales

> *Chaque décision majeure est documentée dans un ADR avec son contexte, les alternatives considérées et la justification.*

Le corpus ADR couvre 28 décisions structurantes, du socle runtime jusqu'à la distribution de release. Chaque ligne ci-dessous renvoie vers l'ADR détaillé dans `docs/adr/`.

---

| ID | Titre | Justification courte | Lien |
|---|---|---|---|
| ADR-001 | Vision et fondations de la stack | Rust + Tokio + SQLite + contrat AIP minimal posent le socle local-first et zéro dépendance. | [ADR-001](../adr/ADR-001-foundations-stack.md) |
| ADR-002 | Bridge PyO3 et découplage par traits | Les agents Python passent par PyO3, le runtime reste découplé via des traits testables sans interpréteur. | [ADR-002](../adr/ADR-002-pyo3-bridge-decoupling.md) |
| ADR-003 | Sandbox, confiance et périmètre plateformes | Isolation native sans Docker, modèle de confiance du code agent et cibles macOS/Linux assumées. | [ADR-003](../adr/ADR-003-sandbox-trust-platform-scope.md) |
| ADR-004 | Conception de la CLI | Pattern noun-verb cohérent et commande `inspect` pour explorer un agent sans runtime. | [ADR-004](../adr/ADR-004-cli-design.md) |
| ADR-005 | Modèle d'exécution ORIA | Classification automatique direct vs orchestré, garde-fous runtime appliqués sans coopération de l'agent. | [ADR-005](../adr/ADR-005-oria-execution-model.md) |
| ADR-006 | Sous-système d'outils | Outils natifs atomiques, exécution concurrente en lot pour les lectures, outils web opt-in en deux étages. | [ADR-006](../adr/ADR-006-tool-subsystem.md) |
| ADR-007 | Runtime d'inférence multi-runner | Inférence locale via un sidecar multi-runner, isolant le moteur du processus principal. | [ADR-007](../adr/ADR-007-inference-multi-runner-sidecar.md) |
| ADR-008 | Backends LLM et gestion des modèles | Registry multi-backend, gestion des modèles et transparence des appels LLM. | [ADR-008](../adr/ADR-008-llm-backends-model-management.md) |
| ADR-009 | Reconnaissance vocale | Moteur speech-to-text embarqué via un backend abstrait, sans service tiers. | [ADR-009](../adr/ADR-009-speech-to-text.md) |
| ADR-010 | Mémoire et assemblage de contexte | Mémoire à l'initiative de l'agent sur SQLite + FTS5, séparée d'une couche de contexte bornée. | [ADR-010](../adr/ADR-010-memory-context-architecture.md) |
| ADR-011 | Profil utilisateur canonique | Un profil utilisateur unique et canonique, source de vérité pour la personnalisation. | [ADR-011](../adr/ADR-011-user-profile.md) |
| ADR-012 | Observabilité et feedback | Persistance SQLite avec troncature configurable, timeline unifiée et feedback optionnel sur les plans. | [ADR-012](../adr/ADR-012-observability-feedback.md) |
| ADR-013 | Human-in-the-loop | Reprise via `agent.run()` enrichi, approbation d'outils déclarée dans le manifest. | [ADR-013](../adr/ADR-013-human-in-the-loop.md) |
| ADR-014 | Config opérationnelle, triggers, notifications | Config opérateur, déclenchement par triggers et notifications poussées depuis l'EventBus. | [ADR-014](../adr/ADR-014-operational-config-triggers-notifications.md) |
| ADR-015 | Gouvernance des permissions et outils | Moteur de permissions multi-couches gouvernant l'accès aux outils. | [ADR-015](../adr/ADR-015-permission-tool-governance.md) |
| ADR-016 | Secrets, keyring et auth API | Secrets dans le keyring OS avec repli fichier chiffré, API TCP protégée par token bearer sur loopback. | [ADR-016](../adr/ADR-016-secrets-keyring-api-auth.md) |
| ADR-017 | Client MCP, transport, serveur | Client `apollia-mcp` natif, trait de transport (stdio, HTTP, SSE) et exposition d'Apollia en serveur MCP. | [ADR-017](../adr/ADR-017-mcp-client-transport-server.md) |
| ADR-018 | OAuth MCP et orchestration | Client OAuth 2.1 générique sans code spécifique fournisseur, câblé de bout en bout par un orchestrateur. | [ADR-018](../adr/ADR-018-mcp-oauth.md) |
| ADR-019 | Connecteurs et intégrations | Connecteurs natifs et intégrations exposés via un wizard générique. | [ADR-019](../adr/ADR-019-connectors-integrations.md) |
| ADR-020 | Architecture desktop | Application Tauri à processus unique embarquant le runtime. | [ADR-020](../adr/ADR-020-desktop-architecture.md) |
| ADR-021 | Design system et i18n | Design system frontend tokenisé et internationalisation FR + EN. | [ADR-021](../adr/ADR-021-design-system-i18n.md) |
| ADR-022 | Sous-système de chat | Acteur `ChatSessionManager` dédié, chemin d'exécution séparé du `TaskRouter`, streaming SSE. | [ADR-022](../adr/ADR-022-chat-subsystem.md) |
| ADR-023 | SDK Python / AgentKit | SDK decorator-first dérivant les schémas I/O des signatures. | [ADR-023](../adr/ADR-023-sdk-agentkit-design.md) |
| ADR-024 | Contrat runtime du SDK (ctx) | Un `Ctx` typé unique exposant le backend via des services imbriqués, miroir du `RuntimeContext` Rust. | [ADR-024](../adr/ADR-024-sdk-runtime-contract-ctx.md) |
| ADR-025 | Worker agents et routing A2A | Worker agents spécialisés, découverte et invocation A2A via le `skill_id` complet. | [ADR-025](../adr/ADR-025-worker-agents-a2a-routing.md) |
| ADR-026 | Installation et distribution d'agents | Format de bundle d'agent, installation et distribution. | [ADR-026](../adr/ADR-026-agent-install-distribution.md) |
| ADR-027 | Agent d'onboarding | Onboarding conversationnel non déterministe porté par un agent dédié. | [ADR-027](../adr/ADR-027-onboarding-agent.md) |
| ADR-028 | Release, updater, signature de code | Distribution de release avec auto-updater et signature de code. | [ADR-028](../adr/ADR-028-release-updater-code-signing.md) |
