# Cycle de vie : ProcessState et TaskState

Apollia OS gère deux machines d'état distinctes : une pour le **processus agent** (ProcessState) et une pour chaque **tâche individuelle** (TaskState). Comprendre ces états permet d'interpréter correctement l'output de la CLI et de concevoir des agents robustes.

---

## ProcessState — le cycle de vie du processus agent

Un agent passe par ces états depuis son déploiement jusqu'à son arrêt :

```
                    apollia-os agent start
                            │
                            ▼
                     ┌─────────────┐
                     │ INITIALIZING│  Chargement Python, validation AIP,
                     └──────┬──────┘  résolution des outils
                            │
               ┌────────────┴────────────┐
               │ succès                  │ outil requis manquant
               ▼                         ▼
          ┌─────────┐             ┌─────────────┐
          │  ACTIVE │             │   STOPPED   │
          └────┬────┘             └─────────────┘
               │
          ┌────┴────────────────────┐
          │ outil optionnel absent  │
          ▼                         │
     ┌──────────┐                   │
     │ DEGRADED │                   │
     └────┬─────┘                   │
          │                         │
          └────────────┬────────────┘
                       │  apollia-os agent stop
                       ▼
                 ┌──────────┐
                 │ STOPPING │  Drain des tâches en cours
                 └────┬─────┘
                      │
                      ▼
                 ┌─────────────┐
                 │   STOPPED   │
                 └─────────────┘
```

### INITIALIZING

L'état transitoire de démarrage. Le runtime :
1. Charge le module Python via PyO3
2. Appelle `agent.manifest` et valide le dictionnaire
3. Résout les outils `tools_required` et `tools_optional`
4. Ouvre le namespace mémoire si `memory_namespace` est défini
5. Crée le sémaphore de concurrence

Si tout se passe bien → `ACTIVE`. Si un outil requis est absent → `STOPPED`.

### ACTIVE

L'état normal d'exécution. L'agent est prêt à recevoir des tâches.

```bash
$ apollia-os agent list
  NAME             STATUS    TASKS    VERSION
  file-assistant   ACTIVE    0/1      1.0.0
```

`0/1` signifie : 0 tâches en cours sur une capacité de 1.

### DEGRADED

L'agent fonctionne mais avec des capacités réduites. Un outil `tools_optional` était absent au démarrage. L'agent doit gérer ce cas dans `run()` en vérifiant `ctx.tools.list_tools` avant d'appeler l'outil manquant.

```bash
$ apollia-os agent list
  NAME             STATUS     TASKS    VERSION
  file-assistant   DEGRADED   0/1      1.0.0   ⚠ mcp:notion/search introuvable
```

### STOPPING

État transitoire pendant l'arrêt. Le runtime attend que toutes les tâches en cours se terminent avant de couper le processus Python. La durée de cet état dépend de la durée des tâches en cours.

```bash
$ apollia-os agent stop file-assistant
  Drain des tâches en cours... (1 active)
  [████████████████░░░░] 80% — t-xyz789 en cours (2.1s)
  ✔ file-assistant arrêté proprement
```

### STOPPED

L'agent est arrêté. Soit le démarrage a échoué (outil requis manquant), soit l'arrêt propre s'est terminé, soit le runtime a détecté une erreur fatale.

```bash
$ apollia-os agent info file-assistant
  ProcessState : STOPPED
  Raison       : Outil requis 'file_write' introuvable dans le Tool Registry
```

---

## TaskState — le cycle de vie d'une tâche

Chaque tâche soumise à un agent suit sa propre machine d'état :

```
   apollia-os run file-assistant "..."
            │
            ▼
       ┌──────────┐
       │ SUBMITTED│  Tâche enregistrée, en attente de capacité
       └────┬─────┘
            │  agent disponible (sémaphore acquis)
            ▼
       ┌──────────┐
       │ EXECUTING│  run() est en cours
       └────┬─────┘
            │
      ┌─────┴──────────────────────────┐
      │                                │                      │
      ▼                                ▼                      ▼
 ┌──────────┐                  ┌──────────────┐       ┌──────────┐
 │COMPLETED │                  │INPUT_REQUIRED│       │  FAILED  │
 └──────────┘                  └──────┬───────┘       └──────────┘
                                      │
                     apollia-os task resume <id> --approve/--reject
                                      │
                               ┌──────┴───────┐
                               │   EXECUTING  │  run() rappelé avec is_resumed=True
                               └──────┬───────┘
                                      │
                               ┌──────┴───────┐
                               │  COMPLETED   │
                               │  ou FAILED   │
                               └──────────────┘
```

### SUBMITTED

La tâche a été reçue par le runtime et enregistrée dans SQLite. Elle est en attente de capacité : si l'agent traite déjà `max_concurrent_tasks` tâches, celle-ci attend dans la file.

```bash
$ apollia-os task list
  TASK_ID     AGENT            STATUS      DURATION
  t-abc123    file-assistant   submitted   —
  t-xyz789    file-assistant   executing   1.3s
```

`t-abc123` est en file car `file-assistant` a une capacité de 1 et `t-xyz789` est en cours.

### EXECUTING

`run()` est en cours d'exécution. La tâche consomme le sémaphore de l'agent.

### COMPLETED

`run()` a retourné `status: "completed"`. Le résultat est disponible dans l'historique des tâches.

```bash
$ apollia-os task show t-xyz789
  Status   : completed
  Duration : 2.1s
  Steps    : 1
  Tools    : file_read, file_write
  Output   : "Résumé de /data/rapport.txt..."
```

### FAILED

`run()` a retourné `status: "failed"`, ou une exception Python non gérée a traversé `run()`, ou le `step_budget` a été épuisé.

```bash
$ apollia-os task show t-abc123
  Status   : failed
  Code     : FILE_NOT_FOUND
  Message  : Impossible de lire /data/inexistant.txt
```

### INPUT_REQUIRED

`run()` a retourné `status: "input_required"`. La tâche est suspendue en attendant une décision humaine. Le `context` fourni par l'agent est persisté dans SQLite.

```bash
$ apollia-os task list --status input_required
  TASK_ID     AGENT          STATUS           WAITING_SINCE
  t-def456    devis-agent    input_required   5m

$ apollia-os task show t-def456
  Prompt : "Confirmer l'envoi du devis (5100€) à dupont@sa.fr ?"
```

---

## Observer les états depuis la CLI

### Surveiller un agent en temps réel

```bash
$ apollia-os agent watch file-assistant
  [10:00:00] ProcessState: ACTIVE (0/1 tâches)
  [10:00:03] Task t-xyz789 : SUBMITTED → EXECUTING
  [10:00:05] Task t-xyz789 : EXECUTING → COMPLETED (2.1s, 3 tool calls)
  [10:00:05] ProcessState: ACTIVE (0/1 tâches)
```

### Historique des tâches

```bash
$ apollia-os task list
  TASK_ID     AGENT            STATUS      DURATION
  t-def456    file-assistant   completed   2.1s
  t-ghi012    file-assistant   failed      0.4s
  t-jkl345    devis-agent      input_required   —

# Filtrer par statut
$ apollia-os task list --status failed
$ apollia-os task list --status input_required
```

### L'audit trail

Chaque appel d'outil — quelle que soit l'issue de la tâche — est tracé dans `~/.apollia/audit.db` :

```bash
$ apollia-os audit --last 10
  HEURE          AGENT            TÂCHE    OUTIL         DURÉE   RÉSULTAT
  10:00:05       file-assistant   t-xyz789 file_write    12ms    ✔
  10:00:04       file-assistant   t-xyz789 file_read     8ms     ✔
  09:58:11       file-assistant   t-ghi012 file_read     6ms     ✗ NOT_FOUND
```

L'audit trail est indépendant de la mémoire de l'agent — il n'est jamais effacé automatiquement et sert à la traçabilité et au débogage.

---

## Récapitulatif

**ProcessState** décrit l'état du processus agent — est-il prêt à accepter des tâches ?

| État | Signification |
|---|---|
| `INITIALIZING` | Démarrage en cours |
| `ACTIVE` | Prêt, toutes les capacités disponibles |
| `DEGRADED` | Prêt, capacités réduites (outil optionnel manquant) |
| `STOPPING` | Arrêt en cours, drain des tâches actives |
| `STOPPED` | Arrêté (démarrage échoué ou arrêt propre terminé) |

**TaskState** décrit l'état d'une tâche individuelle — où en est-elle dans son traitement ?

| État | Signification |
|---|---|
| `SUBMITTED` | En attente de capacité |
| `EXECUTING` | `run()` en cours |
| `COMPLETED` | Terminée avec succès |
| `FAILED` | Terminée avec erreur |
| `INPUT_REQUIRED` | Suspendue, attend une décision humaine |
| `CANCELED` | Annulée par le runtime ou l'opérateur |
