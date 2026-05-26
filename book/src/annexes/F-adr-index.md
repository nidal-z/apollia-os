# Annexe F. Index des ADRs

Les **Architecture Decision Records** (ADRs) documentent chaque décision architecturale significative d'Apollia OS : pourquoi nous avons choisi cette approche, quelles alternatives ont été considérées, et quelles conséquences l'accompagnent.

Au moment de la sortie de la v0.1.0, les ADRs ne sont pas encore publiées en ligne. Elles seront mises en ligne dans une révision proche (cf. l'encadré "ADRs et wiki" en introduction du book). Cette annexe liste les ADRs les plus structurantes, groupées par thème, pour servir de table d'orientation.

---

## Fondations runtime

- `ADR-001` : Rust comme langage runtime.
- `ADR-002` : SQLite comme moteur de persistance.
- `ADR-003` : Duck typing pour AIP (contrat minimal `manifest` + `run`).
- `ADR-005` : Sandbox sans Docker (Linux user namespaces natifs).
- `ADR-006` : REST + JSON API locale.
- `ADR-008` : Pattern noun-verb pour la CLI.
- `ADR-010` : Pivot SaaS vers runtime Rust open-source.
- `ADR-014` : Bridge PyO3 async (`spawn_blocking` + `asyncio.run`).

---

## Acteurs et concurrence

- `ADR-015` : `ToolExecutor` trait abstraction.
- `ADR-016` : `AgentRunner` trait abstraction.
- `ADR-017` : `hyper-util` pour le serving Unix socket.
- `ADR-018` : Bootstrap CLI sans Supervisor (mode dégradé `inspect`).
- `ADR-019` : `AgentLoader` trait, découplage runtime et PyO3.

---

## Moteur ORIA et orchestrated

- `ADR-022` : Mode orchestré ORIA, option B.
- `ADR-035` : Observation par étape en orchestrated.
- `ADR-036` : Stratégie plan cache.
- `ADR-053` : Pipeline fanout et loops.

---

## Mémoire

- `ADR-007` : Mémoire à l'initiative de l'agent (principe #6).
- `ADR-009` : Tokenizer FTS5 `unicode61`.
- `ADR-038` : Global user memory.
- `ADR-039` : Conversation memory management.
- `ADR-054` : Memory episodic consolidation.
- `ADR-058` : Context window management.
- `ADR-066` : Memory export/import format.
- `ADR-070` : Memory namespace project-scoped.
- `ADR-071` : Context bootstrap convention.
- `ADR-087` : User profile redesign.

---

## LLM

- `ADR-020` : `apollia-llm` moteur embarqué, modèles externes feature-flags.
- `ADR-042` : Remplacement de mistral-rs par llama-cpp statique.
- `ADR-047` : Multi-LLM backend registry.
- `ADR-057` : Prompt caching strategy.
- `ADR-067` : Bedrock SigV4 vs SDK.
- `ADR-068` : Vertex ADC vs service account.

---

## Outils et sandbox

- `ADR-012` : Sandbox devmode macOS.
- `ADR-043` : Décomposition atomique des outils.
- `ADR-044` : Client MCP natif.
- `ADR-046` : Transport HTTP + SSE pour MCP.
- `ADR-052` : Windows sandbox.
- `ADR-059` : Concurrent tool execution.
- `ADR-061` : Permission engine 3 layers (session, project, global).
- `ADR-062` : MCP server mode.
- `ADR-082` : Tool governance unifiée.
- `ADR-091` : Catalogue MCP statique, registry vs marketplace.
- `ADR-092` : Exposition resources MCP côté agent ReAct.
- `ADR-093` : Sampling MCP HITL pre-approval.
- `ADR-095` : MCP OAuth orchestrator end-to-end.
- `ADR-096` : Tool execution paths convergence.

---

## HITL et notifications

- `ADR-023` : HITL `is_resumed`, `input_response`, `tools_requiring_approval`.
- `ADR-024` : `apollia-notifications` trait + channel JSON fixe.

---

## A2A et pipelines

- `ADR-025` : `apollia-pipelines`, TOML déclaratif, HITL intégré.
- `ADR-049` : A2A routing inter-agents.

---

## Triggers

- `ADR-021` : Triggers TOML, HMAC, hot reload.

---

## Desktop

- `ADR-027` : Apollia Desktop, processus unique Tauri runtime embarqué.
- `ADR-028` : Frontend Svelte, UX-first.
- `ADR-029` : Settings en lecture seule.
- `ADR-030` : EventBus + Tauri events (remplace polling).
- `ADR-031` : i18n svelte-i18n (FR + EN).
- `ADR-034` : Chat hybride, sessions, streaming, HITL inline.
- `ADR-045` : Page Integrations, wizard générique.
- `ADR-065` : Auto-updater distribution.
- `ADR-097` : Google Drive picker integration.

---

## STT et observabilité

- `ADR-026` : Observabilité complète, persistance timeline, troncature.
- `ADR-041` : Moteur STT embarqué (whisper-rs), trait `SttBackend`.

---

## Auth et OAuth

- `ADR-051` : API auth.
- `ADR-064` : OAuth2 PKCE keyring.
- `ADR-094` : Linux keyring fallback strategy.

---

## Workspace et contexte

- `ADR-033` : Config opérateur en SQLite.
- `ADR-056` : Workspace context assembly (APOLLIA.md).
- `ADR-060` : ContextProvider trait.
- `ADR-069` : Autonomie filesystem, friction graduée, journal réversible.

---

## Apollia AgentKit v0.5 (refonte SDK)

Les décisions qui définissent le SDK Python decorator-first :

- `ADR-098` : Apollia AgentKit, decorator-first.
- `ADR-099` : Signature inference comme schéma I/O.
- `ADR-100` : Exceptions typées au boundary.
- `ADR-101` : Ctx exhaustif et typé via Protocol (14 services).
- `ADR-102` : SDK A2A API unifiée.
- `ADR-103` : SDK datasources et templates runtime.
- `ADR-104` : SDK secrets read-only gating.
- `ADR-105` : SDK events types publics.
- `ADR-106` : SDK logger structure.
- `ADR-107` : SDK auto module instance.
- `ADR-108` : SDK mailbox A2A suppression.
- `ADR-109` : SDK AIPResult interne.
- `ADR-110` : `apollia inspect` CLI.
- `ADR-112` : SDK stream cleanup et rename.

---

## Comment lire une ADR

Chaque ADR suit la même structure :

1. **Contexte.** Le problème observé et son ampleur (LOC, agents impactés, etc.).
2. **Décision.** Ce qu'on a choisi de faire, en une phrase, suivi du détail.
3. **Alternatives considérées.** 2 ou 3 options rejetées, avec pour chacune les pour et les contre.
4. **Conséquences.** Positives, négatives, et choses à surveiller.
5. **Principes impactés.** Quels principes architecturaux (parmi les 8) sont touchés.
6. **Liens.** ADRs reliées.

Quand les ADRs seront publiées, vous pourrez les lire directement depuis le dossier `docs/adr/` du repo. En attendant, cette table d'orientation vous indique les thèmes couverts.

---

## Comment proposer une ADR

Cf. [Annexe D (Roadmap)](D-roadmap.md), section "Comment proposer une évolution".
