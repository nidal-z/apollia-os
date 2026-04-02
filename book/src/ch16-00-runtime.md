# Le runtime Rust

Tout au long de ce livre, vous avez utilisé le runtime comme une boîte noire : vous déployez un agent Python, le runtime l'exécute, les outils fonctionnent, la mémoire persiste. Cette section ouvre la boîte.

Comprendre l'architecture interne n'est pas indispensable pour utiliser Apollia OS. Mais si vous voulez contribuer, déboguer des comportements inattendus, ou simplement satisfaire votre curiosité sur ce qui fait tourner vos agents, ce chapitre est pour vous.

---

## Un runtime de 13 acteurs Tokio

Le runtime Apollia OS n'est pas un monolithe. C'est un ensemble de **13 acteurs Tokio**, chacun avec une responsabilité unique, communiquant exclusivement par messages. Aucun état partagé entre acteurs.

```
┌─────────────────────────────────────────────────────────────┐
│                       APOLLIA OS RUNTIME                    │
│                                                             │
│  Supervisor ← watchdog de tous les acteurs                  │
│       │                                                     │
│  ┌────▼──────────┐  ┌───────────────┐  ┌───────────────┐  │
│  │  EventBus     │  │ AgentRegistry │  │  TaskRouter   │  │
│  │ (broadcast)   │  │ (état agents) │  │ (dispatch)    │  │
│  └───────────────┘  └───────────────┘  └───────────────┘  │
│                                                             │
│  ExecutionCoordinator[N]  ←  un par agent actif             │
│  ORIA Engine              ←  boucle ReAct + StepBudget      │
│  TriggerEngine            ←  cron, filewatch, webhook       │
│  PipelineEngine           ←  DAG multi-agents               │
│  NotificationEngine       ←  alertes desktop/webhook        │
│  ChatSessionManager       ←  sessions conversationnelles    │
│  AgentMailbox             ←  messagerie inter-agents        │
│  APIServer (axum)         ←  REST + SSE + socket Unix       │
└─────────────────────────────────────────────────────────────┘
```

---

## Pourquoi Rust + Tokio ?

**Rust** garantit l'absence de data races à la compilation. Un acteur qui tenterait d'accéder à l'état d'un autre sans passer par un message ne compilerait pas — la sécurité est vérifiée avant l'exécution.

**Tokio** fournit un runtime asynchrone M:N (M tâches sur N threads OS). Des centaines d'agents peuvent s'exécuter concurremment sans créer un thread par agent.

**PyO3** crée le pont entre le runtime Rust et les agents Python sans sérialisation HTTP. Les appels Python/Rust se font en mémoire — c'est pour ça que la latence de soumission d'une tâche est de quelques millisecondes.

---

## Ce que vous allez apprendre

- **Section 1 — Modèle acteur** : le pattern Handle, pourquoi zéro `Arc<Mutex<T>>` entre acteurs, les 8 acteurs principaux et leurs responsabilités
- **Section 2 — Le Supervisor** : séquence de démarrage ordonnée, RestartPolicy, arrêt graceful avec drain
- **Section 3 — L'API REST** : les deux surfaces (Unix socket et TCP), les endpoints, le streaming SSE
- **Section 4 — Configuration** : `apollia.toml` complet, toutes les sections et leurs valeurs par défaut
