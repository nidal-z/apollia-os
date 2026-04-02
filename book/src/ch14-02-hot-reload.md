# Hot reload

Un trigger modifié prend effet immédiatement — sans redémarrer le runtime, sans interrompre les agents en cours d'exécution. C'est le hot reload : la capacité du `TriggerEngine` à recharger ses sources à chaud.

---

## Modifier un trigger sans interruption

Toute opération CRUD via l'API REST déclenche automatiquement un reload du `TriggerEngine` :

```bash
# Changer l'heure du cron — appliqué immédiatement
curl -X PUT http://localhost:7771/api/v1/triggers/rapport-hebdomadaire \
  -H "Content-Type: application/json" \
  -d '{
    "source": { "type": "cron", "schedule": "0 9 * * MON" },
    "on_busy": "drop"
  }'
# → TriggerEngine reload automatique
# → TriggersReloaded { count: 3 } émis sur EventBus
```

Le pattern est **écriture SQLite → engine.reload()** : la nouvelle définition est d'abord persistée, puis le reload s'applique. En cas de panne entre les deux étapes, la définition en SQLite sera appliquée au prochain démarrage du runtime.

---

## Ce qui se passe lors d'un reload

```
engine.reload(nouvelles_définitions)
  │
  ├── Pour chaque source modifiée ou supprimée :
  │     CancellationToken::cancel()
  │     timeout 2s → abort forcé si nécessaire
  │     JoinHandle dropped
  │
  ├── Pour chaque source nouvelle ou modifiée :
  │     spawn_cron() / spawn_interval() / spawn_file_watch() / ...
  │     nouveau JoinHandle enregistré
  │
  └── EventBus.emit(TriggersReloaded { count })
```

Les sources inchangées continuent de tourner sans interruption. Seules les sources affectées par la modification sont redémarrées.

---

## Reload manuel depuis la CLI

```bash
apollia-os trigger reload
# ✔ 3 triggers rechargés
```

Le reload manuel relit `triggers_def.db` entièrement et redémarre toutes les sources. Utile si vous avez modifié la base directement ou si vous suspectez un état incohérent.

---

## Activer et désactiver à chaud

`enabled: false` met un trigger en pause sans le supprimer ni le modifier. Le trigger reste en SQLite — vous pouvez le réactiver à tout moment.

```bash
# Désactiver (la source s'arrête immédiatement)
apollia-os trigger disable rapport-hebdomadaire
# ✔ rapport-hebdomadaire désactivé

# Réactiver (la source redémarre immédiatement)
apollia-os trigger enable rapport-hebdomadaire
# ✔ rapport-hebdomadaire activé

# Même résultat via l'API
curl -X PUT http://localhost:7771/api/v1/triggers/rapport-hebdomadaire \
  -H "Content-Type: application/json" \
  -d '{"enabled": false}'
```

Les événements `TriggerEnabled` et `TriggerDisabled` sont émis sur l'EventBus à chaque changement d'état.

---

## Consulter l'historique des fires

Chaque déclenchement (fire, skip, error) est persisté dans `trigger_history` :

```bash
# 20 derniers événements du trigger
apollia-os trigger logs rapport-hebdomadaire --last 20

# FIRED_AT              STATUS   TASK_ID      LATENCE
# 2026-04-07 08:00:02   fired    t-0042       12ms
# 2026-03-31 08:00:01   fired    t-0038       11ms
# 2026-03-24 08:00:03   skipped  —            — (agent WORKING)
# 2026-03-17 08:00:01   fired    t-0031       14ms
```

`LATENCE` est le délai entre le moment du fire et la soumission de la tâche au `TaskRouter` (`dispatch_ms` en SQLite).

---

## Les 6 événements runtime triggers

| Événement | Émis quand |
|---|---|
| `TriggerFired` | Tâche soumise au TaskRouter avec succès |
| `TriggerSkipped` | Fire ignoré (`on_busy: drop`) |
| `TriggerError` | Erreur de soumission (`on_busy: error`) |
| `TriggerEnabled` | Trigger activé |
| `TriggerDisabled` | Trigger désactivé |
| `TriggersReloaded` | Reload complet terminé, avec le nombre de sources actives |

Ces événements apparaissent dans le dashboard (`/api/v1/dashboard/stream` SSE) et dans les logs observabilité.

---

## Inspecter le statut en temps réel

```bash
apollia-os trigger status rapport-hebdomadaire

# ID      : rapport-hebdomadaire
# Agent   : rapport-agent
# Type    : cron (0 9 * * MON)
# Enabled : ✓
# Fires   : 42
# Skips   : 3
# Errors  : 0
# Dernier fire : 2026-04-07 09:00:02

# Mode JSON
apollia-os trigger list --json
```

---

## Récapitulatif des commandes CLI trigger

```bash
apollia-os trigger list                          # Tableau de tous les triggers
apollia-os trigger status <id>                   # Détail d'un trigger
apollia-os trigger fire <id>                     # Déclencher immédiatement
apollia-os trigger enable <id>                   # Activer
apollia-os trigger disable <id>                  # Désactiver
apollia-os trigger logs <id> [--last N]          # Historique SQLite
apollia-os trigger reload                        # Reload manuel complet
apollia-os trigger list --json                   # Sortie JSON (pipeline)
```
