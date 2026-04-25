# Le Supervisor

Le Supervisor est le gardien du runtime. Il démarre les acteurs dans le bon ordre, surveille leur santé, les redémarre s'ils tombent, et orchestre l'arrêt graceful quand vous demandez `apollia-os stop`.

---

## Séquence de démarrage ordonnée (13 phases)

Le Supervisor démarre les acteurs dans un ordre strict — chaque acteur dépend de ceux qui précèdent.

```
Phase  1 : EventBus           → bus interne (tout le monde en dépend)
Phase  2 : AgentRegistry      → état des agents
Phase  3 : Tool Registry      → catalogue outils + résolution MCP
Phase  4 : Memory Engine      → connexions SQLite
Phase  5 : LlmRouter          → backends LLM (local + cloud)
Phase  6 : TriggerEngine      → ouvre triggers_def.db, charge les triggers
Phase  7 : PipelineEngine     → ouvre pipelines_def.db, charge les pipelines
Phase  8 : APIServer          → accepte les connexions externes
Phase  9 : NotificationEngine → ouvre notifications.db
Phase 10 : AgentMailbox       → files de messages inter-agents
Phase 11 : ChatSessionManager → ouvre chat.db, restaure sessions
Phase 12 : SttEngine          → charge le modèle Whisper (conditionnel)
Phase 13 : BundledAgents      → auto-installe les 4 agents bundled si absents
```

Si la phase N échoue, toutes les phases précédentes sont arrêtées en ordre inverse avant que le processus se termine. Aucun démarrage partiel silencieux.

```bash
apollia-os start
# ✔ EventBus         prêt
# ✔ AgentRegistry    prêt
# ✔ Tool Registry    6 outils chargés
# ✔ Memory Engine    prêt
# ✔ LlmRouter        2 backends (local · anthropic)
# ✔ TriggerEngine    3 triggers actifs
# ✔ PipelineEngine   2 pipelines chargés
# ✔ APIServer        localhost:7771 · /tmp/apollia.sock
# ✔ NotificationEngine
# ✔ AgentMailbox
# ✔ ChatSessionManager
# ─────────────────────────────────────────
# ✔ Runtime prêt en 1.4s
```

---

## RestartPolicy — comportement en cas de panique

Chaque acteur a une politique de redémarrage :

```rust
pub enum RestartPolicy {
    Always,      // Redémarre toujours après une panique
    OnFailure,   // Redémarre seulement si exit non-normal
    Never,       // Pas de redémarrage
}
```

| Acteur | Policy | Raison |
|---|---|---|
| EventBus | `Always` | Canal central — indisponible = runtime aveugle |
| AgentRegistry | `Always` | État des agents — indisponible = dispatch impossible |
| TaskRouter | `Always` | Dispatch — indisponible = plus de tâches acceptées |
| APIServer | `OnFailure` | Peut rebinder si le port était occupé |

Si un acteur dépasse `max_restarts` (défaut : 5) dans `restart_window_secs` (défaut : 60s), le runtime s'arrête entièrement avec `exit(1)`. Le système préfère un arrêt net à un état incohérent.

```bash
# Dans les logs quand max_restarts est atteint :
# FATAL: EventBus a planté 5 fois en 60s — arrêt du runtime
```

---

## Mode embarqué — Desktop Tauri

Quand l'application desktop Tauri démarre, elle appelle `init_embedded` :

```rust
// La même séquence de démarrage que la CLI, dans un thread dédié
pub fn init_embedded(config: EmbeddedConfig) -> Result<RuntimeHandle, EmbeddedError>
```

`init_embedded` spawne un thread `"apollia-runtime"` qui crée un `tokio::Runtime`, démarre le `Supervisor` complet, et attend `AllReady` (timeout 30s par défaut). Le `RuntimeHandle` retourné contient les handles Tokio de tous les acteurs — utilisables directement par les commandes Tauri `#[tauri::command]` sans passer par HTTP.

Le socket Unix et l'API TCP restent actifs : la CLI fonctionne en parallèle du desktop, sur le même runtime.

---

## Arrêt graceful (drain)

```
SIGTERM / SIGINT / apollia-os stop
       │
       ▼
EventBus.broadcast(ShutdownRequested)
       │
       ▼
APIServer : refuse les nouvelles connexions
       │
       ▼
TaskRouter : refuse les nouvelles soumissions (ShuttingDown)
       │
       ▼
Pour chaque agent ACTIVE :
  ProcessState → STOPPING
  ├── Drain des tâches en cours (timeout : 30s)
  ├── on_stop() callback Python appelé
  └── ProcessState → STOPPED
       │
       ▼
Memory Engine → flush SQLite + fermeture connexions
Tool Registry → fermeture connexions MCP ouvertes
       │
       ▼
Supervisor → arrêt de tous les acteurs Tokio
       │
       ▼
exit(0)
```

**Timeout de drain : 30s.** Si une tâche n'est pas terminée dans ce délai, elle est annulée (`CANCELED`) et tracée dans l'audit log. Aucune tâche n'est perdue silencieusement.

```bash
apollia-os stop
# Apollia OS arrêt en cours...
# → Drain (timeout: 30s)
#   ✔ facture-director  — tâche t-042 terminée (2.1s)
#   ✔ compta-worker     — aucune tâche active
# → Flush SQLite · Fermeture MCP
# ✔ Arrêt propre en 4.3s
```

Pour forcer l'arrêt immédiat sans drain :

```bash
apollia-os stop --force
```

`--force` annule immédiatement toutes les tâches en cours. À utiliser seulement en cas de blocage — les tâches HITL en attente d'approbation seront perdues.
