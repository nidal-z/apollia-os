# Summary

[Introduction](README.md)

---

# Partie I. Premiers pas

- [Installation](part-i-getting-started/01-installation.md)
- [Quickstart : agent conversationnel](part-i-getting-started/02-quickstart-conversational.md)
- [Quickstart : agent worker](part-i-getting-started/03-quickstart-worker.md)
- [Quickstart : agent director](part-i-getting-started/04-quickstart-director.md)
- [Quickstart : agent orchestré](part-i-getting-started/05-quickstart-orchestrated.md)

# Partie II. Les décorateurs

- [`@agent`](part-ii-the-decorators/06-agent-decorator.md)
- [`@skill`](part-ii-the-decorators/07-skill-decorator.md)
- [`@on_message`](part-ii-the-decorators/08-on-message-decorator.md)
- [`@orchestrated`](part-ii-the-decorators/09-orchestrated-decorator.md)

# Partie III. Le protocole Ctx

- [Vue d'ensemble : 14 services](part-iii-the-ctx-protocol/10-ctx-overview.md)
- [`ctx.llm`](part-iii-the-ctx-protocol/11-ctx-llm.md)
- [`ctx.memory`](part-iii-the-ctx-protocol/12-ctx-memory.md)
- [`ctx.tools`](part-iii-the-ctx-protocol/13-ctx-tools.md)
- [`ctx.a2a` et `apollia.react`](part-iii-the-ctx-protocol/14-ctx-a2a.md)
- [`ctx.datasources` et `ctx.templates`](part-iii-the-ctx-protocol/15-ctx-datasources-templates.md)
- [`ctx.secrets`](part-iii-the-ctx-protocol/16-ctx-secrets.md)
- [`ctx.events`, `ctx.logger`, `ctx.budget`](part-iii-the-ctx-protocol/17-ctx-events-logger-budget.md)
- [Autres services : profile, workspace, stt, notify](part-iii-the-ctx-protocol/18-ctx-other-services.md)

# Partie IV. Design LLM-friendly

- [Descriptions via `Annotated`](part-iv-llm-friendly-design/19-annotated-descriptions.md)
- [Exemples de payloads](part-iv-llm-friendly-design/20-examples-payloads.md)
- [Schémas via `TypedDict`](part-iv-llm-friendly-design/21-typeddict-schemas.md)

# Partie V. Gestion des erreurs

- [`DomainError`](part-v-error-handling/22-domain-errors.md)
- [`NeedHumanInput`](part-v-error-handling/23-need-human-input.md)

# Partie VI. Tests

- [`apollia.testing.mock`](part-vi-testing/24-testing-isomorphic-mock.md)
- [Assertions](part-vi-testing/25-assertions.md)
- [Suites d'évaluation](part-vi-testing/26-eval-suites.md)

# Partie VII. Outillage

- [`apollia inspect`](part-vii-tooling/27-apollia-inspect.md)
- [`apollia new` : scaffolding](part-vii-tooling/28-apollia-new-scaffolding.md)

# Partie VIII. Le runtime Rust

- [Vue d'ensemble du runtime](part-viii-runtime-rust/29-runtime-overview.md)
- [Acteurs Tokio et Supervisor](part-viii-runtime-rust/30-actors-supervisor.md)
- [API REST et configuration](part-viii-runtime-rust/31-rest-api-config.md)
- [L'application Desktop](part-viii-runtime-rust/32-desktop.md)
- [La CLI complète](part-viii-runtime-rust/33-cli-complete.md)
- [Adapter LangGraph / CrewAI](part-viii-runtime-rust/34-adapters.md)
- [Outils, sandbox, permissions](part-viii-runtime-rust/35-tools-sandbox-permissions.md)
- [Triggers](part-viii-runtime-rust/36-triggers.md)

# Partie IX. Projet capstone

- [Vue d'ensemble du capstone](part-ix-capstone/37-capstone-overview.md)
- [Architecture multi-agent](part-ix-capstone/38-capstone-architecture.md)
- [Implémentation des workers](part-ix-capstone/39-capstone-workers.md)
- [Director et résultat](part-ix-capstone/40-capstone-director-result.md)

# Annexes

- [A. Diagrammes d'architecture](annexes/A-diagrams/index.md)
- [B. Glossaire](annexes/B-glossary.md)
- [C. Principes architecturaux](annexes/C-principles.md)
- [D. Roadmap](annexes/D-roadmap.md)
- [E. Vision et positionnement](annexes/E-vision.md)
- [F. Index des ADRs](annexes/F-adr-index.md)
- [G. FAQ](annexes/G-faq.md)
