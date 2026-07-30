# Decisions Log - Apollia OS

> Journal des décisions architecturales significatives.
> Chaque entrée pointe vers le fichier ADR détaillé dans `docs/adr/`.

---

| Date | ID | Titre | Lien |
|---|---|---|---|
| 2026-06-04 | ADR-001 | Vision et fondations de la stack | [ADR-001](adr/ADR-001-foundations-stack.md) |
| 2026-06-04 | ADR-002 | Bridge PyO3 et découplage par traits | [ADR-002](adr/ADR-002-pyo3-bridge-decoupling.md) |
| 2026-06-04 | ADR-003 | Sandbox, modèle de confiance des agents et périmètre plateformes | [ADR-003](adr/ADR-003-sandbox-trust-platform-scope.md) |
| 2026-06-04 | ADR-004 | Conception de la CLI (noun-verb, inspect) | [ADR-004](adr/ADR-004-cli-design.md) |
| 2026-06-04 | ADR-005 | Modèle d'exécution ORIA | [ADR-005](adr/ADR-005-oria-execution-model.md) |
| 2026-06-04 | ADR-006 | Sous-système d'outils et outils natifs | [ADR-006](adr/ADR-006-tool-subsystem.md) |
| 2026-06-04 | ADR-007 | Runtime d'inférence en sidecar multi-runner | [ADR-007](adr/ADR-007-inference-multi-runner-sidecar.md) |
| 2026-06-04 | ADR-008 | Backends LLM, gestion des modèles et transparence | [ADR-008](adr/ADR-008-llm-backends-model-management.md) |
| 2026-06-04 | ADR-009 | Moteur de reconnaissance vocale (speech-to-text) | [ADR-009](adr/ADR-009-speech-to-text.md) |
| 2026-06-04 | ADR-010 | Architecture mémoire et assemblage de contexte | [ADR-010](adr/ADR-010-memory-context-architecture.md) |
| 2026-06-04 | ADR-011 | Profil utilisateur canonique | [ADR-011](adr/ADR-011-user-profile.md) |
| 2026-06-04 | ADR-012 | Observabilité et feedback sur les plans | [ADR-012](adr/ADR-012-observability-feedback.md) |
| 2026-06-04 | ADR-013 | Human-in-the-loop (HITL) | [ADR-013](adr/ADR-013-human-in-the-loop.md) |
| 2026-06-04 | ADR-014 | Config opérationnelle, triggers et notifications | [ADR-014](adr/ADR-014-operational-config-triggers-notifications.md) |
| 2026-06-04 | ADR-015 | Gouvernance des permissions et des outils | [ADR-015](adr/ADR-015-permission-tool-governance.md) |
| 2026-06-04 | ADR-016 | Secrets, stockage keyring et auth de l'API locale | [ADR-016](adr/ADR-016-secrets-keyring-api-auth.md) |
| 2026-06-04 | ADR-017 | Client MCP, transport et mode serveur | [ADR-017](adr/ADR-017-mcp-client-transport-server.md) |
| 2026-06-04 | ADR-018 | Client OAuth MCP et orchestration | [ADR-018](adr/ADR-018-mcp-oauth.md) |
| 2026-06-04 | ADR-019 | Connecteurs natifs et intégrations | [ADR-019](adr/ADR-019-connectors-integrations.md) |
| 2026-06-04 | ADR-020 | Architecture de l'application desktop | [ADR-020](adr/ADR-020-desktop-architecture.md) |
| 2026-06-04 | ADR-021 | Design system frontend et i18n | [ADR-021](adr/ADR-021-design-system-i18n.md) |
| 2026-06-04 | ADR-022 | Sous-système de chat | [ADR-022](adr/ADR-022-chat-subsystem.md) |
| 2026-06-04 | ADR-023 | Conception du SDK Python / AgentKit | [ADR-023](adr/ADR-023-sdk-agentkit-design.md) |
| 2026-06-04 | ADR-024 | Contrat runtime du SDK (ctx) | [ADR-024](adr/ADR-024-sdk-runtime-contract-ctx.md) |
| 2026-06-04 | ADR-025 | Worker agents et routing A2A | [ADR-025](adr/ADR-025-worker-agents-a2a-routing.md) |
| 2026-06-04 | ADR-026 | Installation des agents, format de bundle et distribution | [ADR-026](adr/ADR-026-agent-install-distribution.md) |
| 2026-06-04 | ADR-027 | Agent d'onboarding | [ADR-027](adr/ADR-027-onboarding-agent.md) |
| 2026-06-04 | ADR-028 | Distribution de release, updater et signature de code | [ADR-028](adr/ADR-028-release-updater-code-signing.md) |
| - | ADR-029 | Numéro sauté, aucun ADR associé | - |
| 2026-06-05 | ADR-030 | Fiabilité du modèle local et routage hybride | [ADR-030](adr/ADR-030-local-model-reliability-hybrid-routing.md) |
| 2026-06-10 | ADR-031 | Modèle de plan unifié dans apollia-core | [ADR-031](adr/ADR-031-unified-plan-model.md) |
| 2026-06-10 | ADR-032 | Moteur de plan natif au chat | [ADR-032](adr/ADR-032-chat-native-plan-engine.md) |
| 2026-06-10 | ADR-033 | Audit et rejeu de la construction du plan | [ADR-033](adr/ADR-033-plan-construction-audit-replay.md) |
| 2026-07-06 | ADR-034 | Taxonomie CLI v2 (verbes canoniques, top-level git-style, renommage pre-release) | [ADR-034](adr/ADR-034-cli-taxonomy-v2.md) |
| 2026-07-06 | ADR-035 | Surface CLI IA-native (do/explain/reprompt) sur le modele local, GBNF + dry-run | [ADR-035](adr/ADR-035-cli-ai-native-surface.md) |
| 2026-07-06 | ADR-036 | Decouvrabilite CLI (completions clap_complete, palette fuzzy maison, guide, did-you-mean) | [ADR-036](adr/ADR-036-cli-discoverability.md) |
| 2026-07-08 | ADR-037 | Contrat de pilotage partage pour l'integration hote (OpenAPI genere, SDK client hote TS+Python, auth TCP coherente, fix execution MCP) | [ADR-037](adr/ADR-037-host-driving-contract.md) |
| 2026-07-08 | ADR-038 | Contrat d'arguments des steps de plan orchestres (hybride A+B: args structures PlanStep remplis par le Reasoner en GBNF, repli extraction JIT) | [ADR-038](adr/ADR-038-orchestrated-step-args-contract.md) |
| 2026-07-08 | ADR-039 | Verification et critic sur le chemin orchestre (verdict audite via VerificationCompleted, replan-on-fail borne, gating par tier d'autonomie) | [ADR-039](adr/ADR-039-orchestrated-verification-critic.md) |
| 2026-07-08 | ADR-040 | Site de documentation adopters (Docusaurus, Diataxis, references API/CLI/SDK generees de la source unique, clap-markdown derriere une feature gen-docs) | [ADR-040](adr/ADR-040-docs-site-docusaurus.md) |
| 2026-07-10 | ADR-041 | Messagerie inter-agents durable et auditable (service `ctx.mail`, store SQLite at-least-once, TTL, auditabilite HMAC, controle hote) | [ADR-041](adr/ADR-041-inter-agent-messaging.md) |
| 2026-07-10 | ADR-042 | Ancre globale de chaine pour la preuve de troncature du journal d'audit | [ADR-042](adr/ADR-042-audit-journal-global-anchor.md) |
| 2026-07-10 | ADR-043 | Outillage de verification concurrence (Loom) et UB (Miri), dev-only sans dependance runtime | [ADR-043](adr/ADR-043-loom-miri-verification-tooling.md) |
| 2026-07-10 | ADR-044 | Durcissement de l'isolation des agents et visibilite de la posture | [ADR-044](adr/ADR-044-agent-isolation-hardening.md) |
| 2026-07-13 | ADR-045 | Superviseur fail-fast puis degradation, sans redemarrage d'acteur au crash | [ADR-045](adr/ADR-045-supervisor-fail-fast-degrade.md) |
| 2026-07-13 | ADR-046 | Preuves Kani des invariants cardinaux et fence de bail du mailbox | [ADR-046](adr/ADR-046-kani-invariant-proofs-mailbox-lease-fence.md) |
| 2026-07-14 | ADR-047 | TLS natif sur le listener TCP de l'API et fail-fast sur bind distant non securise | [ADR-047](adr/ADR-047-api-tls-and-non-loopback-fail-fast.md) |
| 2026-07-20 | ADR-048 | Les executeurs de code ne sont jamais autorises en bloc par leur nom | [ADR-048](adr/ADR-048-code-executors-never-blanket-authorized.md) |
| 2026-07-30 | ADR-049 | Windows est une plateforme supportee pour la v0.1.0 (supersede le scope d'ADR-003) | [ADR-049](adr/ADR-049-windows-in-scope-for-v0-1-0.md) |
