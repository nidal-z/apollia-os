# Vue d'ensemble du runtime Rust

Jusqu'ici, vous avez utilisé le runtime comme une boîte noire : vous décorez une classe Python, le runtime l'exécute, les outils fonctionnent, la mémoire persiste. Cette partie ouvre la boîte.

Comprendre l'architecture interne n'est pas indispensable pour écrire un agent Apollia. C'est utile pour contribuer au runtime, déboguer un comportement inattendu, ou simplement comprendre ce qui fait tourner vos agents.

---

## Un runtime d'acteurs Tokio

Le runtime n'est pas un monolithe. C'est un ensemble d'acteurs Tokio, chacun avec une responsabilité unique, communiquant exclusivement par messages. Aucun état partagé entre acteurs.

```
┌─────────────────────────────────────────────────────────────┐
│                       APOLLIA OS RUNTIME                    │
│                                                             │
│  Supervisor : watchdog de tous les acteurs                  │
│       │                                                     │
│  ┌────▼──────────┐  ┌───────────────┐  ┌───────────────┐  │
│  │  EventBus     │  │ AgentRegistry │  │  TaskRouter   │  │
│  │ (broadcast)   │  │ (état agents) │  │ (dispatch)    │  │
│  └───────────────┘  └───────────────┘  └───────────────┘  │
│                                                             │
│  ExecutionCoordinator[N] : un par agent actif               │
│  ORIA Engine             : boucle ReAct + StepBudget        │
│  TriggerEngine           : cron, filewatch, webhook         │
│  PipelineEngine          : DAG multi-agents                 │
│  NotificationEngine      : alertes desktop, webhook         │
│  ChatSessionManager      : sessions conversationnelles      │
│  APIServer (axum)        : REST + SSE + socket Unix         │
└─────────────────────────────────────────────────────────────┘
```

---

## Pourquoi Rust + Tokio

**Rust** garantit l'absence de data races à la compilation. Un acteur qui tenterait d'accéder à l'état d'un autre sans passer par un message ne compilerait pas. La sécurité est vérifiée avant l'exécution.

**Tokio** fournit un runtime asynchrone M:N (M tâches sur N threads OS). Des centaines d'agents peuvent s'exécuter concurremment sans créer un thread par agent.

**PyO3** crée le pont entre le runtime Rust et les agents Python sans sérialisation HTTP. Les appels Python/Rust se font en mémoire. C'est ce qui donne une latence de soumission d'une tâche de quelques millisecondes.

---

## Ce que vous allez lire dans cette partie

- [Chapitre 30](30-actors-supervisor.md) : modèle acteur, pattern Handle, le Supervisor et sa séquence de démarrage.
- [Chapitre 31](31-rest-api-config.md) : l'API REST (Unix socket + TCP), la configuration `apollia.toml`.
- [Chapitre 32](32-desktop.md) : l'application Desktop Tauri qui embarque le runtime.
- [Chapitre 33](33-cli-complete.md) : la CLI complète, ses commandes, ses codes de sortie.
- [Chapitre 34](34-adapters.md) : intégrer LangGraph, CrewAI, AutoGen comme agents Apollia.
- [Chapitre 35](35-tools-sandbox-permissions.md) : la sandbox d'exécution, le moteur de permissions, le routing MCP.
- [Chapitre 36](36-triggers.md) : cron, file watcher, webhook, hot reload.

---

## Le bridge PyO3

Le bridge entre Rust et Python vit dans la crate `apollia-aip`. Au démarrage d'un agent, le runtime :

1. Lit le fichier Python de l'agent.
2. Le charge via `PyModule::from_code` (sans installer un package).
3. Récupère l'attribut `agent` du module (l'instance auto-créée par le décorateur `@agent`).
4. Extrait `__apollia_manifest__` (le manifeste construit au décor).
5. Installe le hook `__apollia_dispatch__` qui sera appelé à chaque invocation.

À l'invocation d'une skill ou d'un `@on_message`, le bridge marshalle le payload, appelle la méthode async Python depuis Tokio, et marshalle le retour. Pas de serialisation JSON sur le wire, c'est du PyObject direct.

> **Référence technique :** la spec complète du contrat PyO3 (validateur, dispatcher, marshalling) sera dans la page wiki `Briques-Apollia-AIP` *(wiki disponible prochainement)*.

---

## Ce que le runtime garantit

- **Isolation des agents :** chaque agent tourne dans son venv Python, avec son répertoire workspace, sans pouvoir lire les variables ou la mémoire d'un autre agent.
- **Budget non négociable :** `StepBudget` enforce un plafond de steps, d'appels d'outils, et de wall-clock. Aucun agent ne peut s'y soustraire depuis Python.
- **Audit trail complet :** chaque invocation d'outil, chaque appel A2A, chaque pause HITL est tracé dans une base SQLite locale append-only.
- **Hot reload :** les triggers, les pipelines, et les configurations opérationnelles sont modifiables sans redémarrer.
- **Persistance des tâches :** une tâche suspendue (HITL) survit à un redémarrage du runtime.

---

## Ce que le runtime ne fait pas

- **Clustering multi-nœuds :** v0.1 est single-node. Pas de scaling horizontal natif.
- **Replication d'état :** pas de réplication SQLite live. Pour la HA, c'est sauvegarde + restore.
- **Authentification multi-utilisateurs :** v0.1 suppose un opérateur unique par installation. Les notions de rôles et de permissions multi-comptes arrivent plus tard.

Le runtime est conçu pour le cas PME / poste de travail / serveur de bureau, pas pour le cloud multi-tenant.

---

## ADRs

- `ADR-001` : Rust comme langage runtime
- `ADR-014` : Bridge PyO3 async
- `ADR-026` : Observabilité complète et timeline

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
