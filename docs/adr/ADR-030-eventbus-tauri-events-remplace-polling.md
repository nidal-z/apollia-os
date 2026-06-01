# ADR-030 - EventBus → Tauri events remplace le polling IPC

**Date :** 2026-03-16
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 16

---

## Contexte

L'application desktop Apollia OS (Sprint 14-15) utilise un polling IPC toutes les 3 secondes pour rafraîchir l'état du frontend Svelte. Le fichier `sse.ts` - nommé de manière trompeuse - appelle `setInterval(poll, 3000)` qui exécute 6 `invoke()` Tauri en parallèle (`list_agents`, `list_tasks`, `list_llm_backends`, `list_triggers`, `list_all_pipeline_runs`, `list_pending_approvals`).

Ce polling pose trois problèmes :
1. **Latence** : 0 à 3 secondes entre un événement runtime et sa visibilité dans l'UI
2. **Gaspillage** : 6 requêtes IPC toutes les 3 secondes même quand rien ne change
3. **Incohérence** : le runtime émet déjà des `RuntimeEvent` via `broadcast::Sender<RuntimeEvent>` (Sprint 1, EventBus) mais ce canal n'est jamais connecté au frontend Tauri

L'ADR-027 (processus unique Tauri + runtime embarqué) avait prévu d'utiliser le SSE existant (`/api/v1/dashboard/stream`), mais cette approche s'est avérée impraticable : le WebView Tauri en production (assets embarqués) ne peut pas faire de requêtes HTTP directes vers localhost sans problèmes CORS. Le polling IPC a été adopté comme workaround temporaire.

### Contraintes

- **Principe #8 - CLI humaine, API machine** : le desktop doit recevoir les mêmes events que le CLI
- Les events Tauri (`app.emit()`) sont fire-and-forget - pas de backpressure, pas de garantie de livraison
- Le `connectionStatus` store doit continuer à refléter l'état de la connexion au runtime
- Le code Svelte doit rester simple (pas de gestion de reconnexion complexe)

---

## Décision

Nous remplaçons le polling IPC par un **pont EventBus → Tauri events** : un `tokio::task` Rust subscribe au `broadcast::Sender<RuntimeEvent>` existant, convertit chaque event en JSON, et l'émet via `app_handle.emit("runtime-event", payload)`. Le frontend Svelte écoute via `listen("runtime-event")` de `@tauri-apps/api/event`.

### Architecture

```
apollia-runtime (EventBus broadcast::Sender<RuntimeEvent>)
  │
  └── apollia-desktop/src/events.rs
        │ spawn tokio::task
        │ recv() → map category → serde_json::to_value()
        │ app_handle.emit("runtime-event", TauriRuntimeEvent)
        │
        └── ui/src/lib/stores/sse.ts
              listen("runtime-event") → dispatch par category
              → agents.set() / tasks.set() / pendingApprovals.set() / ...
```

### Catégories d'events

| Catégorie Tauri | RuntimeEvent Rust correspondants |
|---|---|
| `agent-changed` | AgentStateChanged, AgentDegraded, AgentRegistered |
| `task-changed` | TaskStarted, TaskCompleted, TaskFailed, TaskCanceled |
| `approval-changed` | TaskInputRequired, TaskResumed, TaskApprovalTimeout |
| `llm-changed` | LlmModelReady, LlmModelFailed, LlmCallCompleted |
| `trigger-fired` | TriggerFired, TriggerSkipped, TriggerError |
| `pipeline-changed` | PipelineStepStarted/Completed/Failed, PipelineCompleted/Failed/Suspended |

### Mécanisme de fallback

- `refreshAll()` est appelé **une seule fois** au démarrage (hydratation initiale)
- Un timer watchdog de 10 secondes détecte l'absence d'events → déclenche un `refreshAll()` unique
- Si le watchdog se déclenche 3 fois consécutives, `connectionStatus` passe à `"reconnecting"`

---

## Alternatives considérées

### Option A - Conserver le polling IPC (rejetée)

**Pour :**
- Fonctionne déjà, zéro effort
- Simple à comprendre et débugger

**Contre :**
- Latence de 0 à 3 secondes inacceptable pour une app desktop moderne
- Gaspillage CPU/mémoire : 6 requêtes IPC toutes les 3 secondes même au repos
- Ne scale pas : chaque nouvelle fonctionnalité nécessite une requête de polling supplémentaire
- Le nom `sse.ts` est trompeur - dette cognitive pour les futurs contributeurs

### Option B - SSE HTTP depuis le WebView (rejetée)

**Pour :**
- Le endpoint SSE `/api/v1/dashboard/stream` existe déjà (Sprint 9)
- Pattern EventSource standard bien documenté

**Contre :**
- **CORS bloquant** : en production, le WebView Tauri charge les assets depuis `tauri://localhost`, et les requêtes vers `http://127.0.0.1:7771` sont bloquées par la Same-Origin Policy
- Contournements possibles (Tauri `allowlist.http`, proxy dans les commandes) mais fragiles et non-standard
- En mode dev (Vite sur `:5173`), le SSE fonctionnait - ce qui a masqué le problème jusqu'au premier build production
- ADR-027 l'avait initialement prévu mais l'expérience Sprint 14 a prouvé que c'est impraticable

### Option retenue - EventBus → Tauri events

**Pour :**
- Latence quasi-nulle : l'event arrive dans le frontend en < 50ms après émission Rust
- Aucune requête HTTP - communication in-process via le bridge Tauri natif
- Un seul canal pour tous les types d'events (pas N requêtes de polling)
- Cohérent avec l'architecture Tauri v2 (events are first-class citizens)
- Le `broadcast::Sender<RuntimeEvent>` existe depuis le Sprint 1 - zéro modification du runtime

**Compromis acceptés :**
- Les events Tauri sont fire-and-forget : si le frontend est lent à traiter un event, il est perdu (mitigé par le fallback watchdog)
- `broadcast::Receiver` peut lag : si le frontend ne consomme pas assez vite, les events les plus anciens sont droppés (le receiver reçoit un `RecvError::Lagged(n)`)
- Le fallback `refreshAll()` reste nécessaire pour l'hydratation initiale et la recovery

---

## Conséquences

**Positives :**
- Réactivité instantanée : les changements d'état sont visibles en < 50ms
- Réduction de la charge CPU/réseau : plus de polling toutes les 3 secondes
- Architecture propre : le frontend est event-driven, pas poll-driven
- Le fichier `sse.ts` portera enfin un nom cohérent avec son comportement

**Négatives / Compromis :**
- Si le `broadcast` channel est saturé (100+ events/seconde), des events seront perdus - le watchdog compense
- Le fallback `refreshAll()` reste du polling (mais appelé rarement : au démarrage + toutes les 10s max en dégradé)
- Complexité légèrement accrue côté Rust (mapping RuntimeEvent → catégories Tauri)

**Neutres / À surveiller :**
- Monitorer le nombre d'events/seconde en charge réelle (agents multiples + triggers actifs)
- Si flood constaté : ajouter un debounce/batch de 100ms côté Svelte
- Le endpoint SSE REST (`/api/v1/dashboard/stream`) reste actif pour le CLI et les intégrations externes

---

## Principes architecturaux impactés

- **Principe #8 - CLI humaine, API machine** : respecté - le desktop reçoit les mêmes `RuntimeEvent` que le CLI. Le SSE REST reste disponible pour les clients externes.

---

## Liens

- Story associée : STORY-156
- ADR précédent lié : ADR-027 (processus unique Tauri - cette ADR corrige l'approche SSE HTTP initialement prévue)
- ADR précédent lié : ADR-017 (hyper-util Unix socket - le socket reste actif en parallèle)
