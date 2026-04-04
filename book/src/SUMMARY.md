# Summary

[Introduction](introduction.md)

---

# Premiers pas

- [Mise en route](ch01-00-getting-started.md)
  - [Installation](ch01-01-installation.md)
  - [Premier lancement](ch01-getting-started/first-launch.md)
  - [Tour guidé interactif](ch01-getting-started/onboarding-tour.md)
  - [Bonjour, Agent !](ch01-02-hello-agent.md)
  - [Anatomie d'un agent Apollia](ch01-03-anatomy.md)

# Projet : l'assistant fichier

- [Votre premier agent](ch02-00-first-agent.md)
  - [Concevoir l'agent](ch02-01-design.md)
  - [Le manifest](ch02-02-manifest.md)
  - [Implémenter run()](ch02-03-run.md)
  - [Utiliser les outils](ch02-04-tools.md)
  - [Tester et exécuter](ch02-05-testing.md)

# Concepts fondamentaux

- [Le contrat AIP](ch03-00-aip-contract.md)
  - [manifest() — déclarer ses capacités](ch03-01-manifest.md)
  - [run() — exécuter une tâche](ch03-02-run.md)
  - [Cycle de vie : ProcessState et TaskState](ch03-03-lifecycle.md)

- [Les outils](ch04-00-tools.md)
  - [Les 10 outils natifs](ch04-01-native-tools.md)
  - [Appeler un outil depuis Python](ch04-02-calling.md)
  - [Sandbox et sécurité](ch04-03-sandbox.md)
  - [Outils MCP externes](ch04-04-mcp.md)

- [La mémoire](ch05-00-memory.md)
  - [Trois types de mémoire](ch05-01-types.md)
  - [Recherche FTS5](ch05-02-search.md)
  - [Namespaces et isolation](ch05-03-namespaces.md)

- [Le LLM](ch06-00-llm.md)
  - [Backends locaux et cloud](ch06-01-backends.md)
  - [ctx.llm : chat, complete, stream](ch06-02-api.md)
  - [La boucle ReAct](ch06-03-react-loop.md)

- [Les garde-fous](ch07-00-guardrails.md)
  - [StepBudget](ch07-01-step-budget.md)
  - [Circuit breakers](ch07-02-resilience.md)
  - [Garde-fous A2A](ch07-03-a2a-guards.md)
  - [Moteur de permissions 3 couches](ch07-04-permissions.md)

# Projet : un Worker Agent

- [Construire un Worker Agent](ch08-00-worker-project.md)
  - [Le pattern Worker](ch08-01-pattern.md)
  - [Le SYSTEM_PROMPT](ch08-02-system-prompt.md)
  - [Guardrails de domaine](ch08-03-domain-guardrails.md)
  - [Tests et benchmark](ch08-04-testing.md)
  - [Publier dans le registre](ch08-05-publishing.md)

# Fonctionnalités avancées

- [Le mode orchestré](ch09-00-orchestrated.md)
  - [ORIA : Observer → Reasoner → Actor](ch09-01-oria.md)
  - [Plans et replanification](ch09-02-plans.md)
  - [on_plan_complete()](ch09-03-hook.md)

- [L'humain dans la boucle](ch10-00-hitl.md)
  - [Suspendre et reprendre](ch10-01-suspend-resume.md)
  - [Approbation d'outils](ch10-02-tool-approval.md)
  - [Notifications](ch10-03-notifications.md)

- [Agents qui collaborent : A2A](ch11-00-a2a.md)
  - [Skills et discovery](ch11-01-skills.md)
  - [Déléguer à un Worker](ch11-02-delegate.md)
  - [Outils A2A dans ORIA](ch11-03-tools-provider.md)

- [Le chat interactif](ch12-00-chat.md)
  - [Sessions et streaming](ch12-01-sessions.md)
  - [Chat Libre vs Chat Agent](ch12-02-modes.md)
  - [Mémoire utilisateur](ch12-03-user-memory.md)

- [Pipelines multi-agents](ch13-00-pipelines.md)
  - [Topologie et dépendances](ch13-01-topology.md)
  - [Conditions et fallbacks](ch13-02-conditions.md)

- [Triggers](ch14-00-triggers.md)
  - [Cron, FileWatch, Webhook](ch14-01-sources.md)
  - [Hot reload](ch14-02-hot-reload.md)

# Projet : une solution PME complète

- [Solution de bout en bout](ch15-00-full-project.md)
  - [Architecture cible](ch15-01-architecture.md)
  - [Les Worker Agents](ch15-02-workers.md)
  - [Le Director Agent](ch15-03-director.md)
  - [Le pipeline](ch15-04-pipeline.md)
  - [Résultat final](ch15-05-result.md)

# Aller plus loin

- [Le runtime Rust](ch16-00-runtime.md)
  - [Architecture acteurs Tokio](ch16-01-actors.md)
  - [Le Supervisor](ch16-02-supervisor.md)
  - [L'API REST](ch16-03-api.md)
  - [Configuration](ch16-04-config.md)

- [L'application Desktop](ch17-00-desktop.md)

- [Adapter LangGraph / CrewAI](ch18-00-adapters.md)

- [La CLI complète](ch19-00-cli.md)

# Annexes

- [A — Diagrammes d'architecture](appendix-a-diagrams/index.md)
- [B — Glossaire](appendix-b-glossary.md)
- [C — Principes architecturaux](appendix-c-principles.md)
- [D — Roadmap](appendix-d-roadmap.md)
- [E — Sprint Summary](appendix-e-sprint-summary.md)
- [F — Vision et positionnement](appendix-f-vision.md)
