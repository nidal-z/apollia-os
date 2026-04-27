# Triggers

Les pipelines et les agents répondent à des invocations explicites — un humain envoie une requête, un Director délègue une tâche. Mais la plupart des workflows opérationnels réels sont pilotés par des événements : un fichier déposé dans un dossier, une heure atteinte, un appel webhook entrant.

Les **triggers** d'Apollia OS permettent de déclencher automatiquement un agent ou un pipeline sans intervention humaine. Ils sont déclaratifs, persistés en SQLite, et modifiables à chaud sans redémarrer le runtime.

---

## Ce que fait un trigger

Un trigger surveille une source d'événements et, quand elle se déclenche, soumet une tâche à un agent ou un pipeline via le `TaskRouter`. C'est tout — le trigger ne connaît pas l'agent, il ne sait pas ce qu'il fait, il se contente de déclencher.

```
Source d'événement               TriggerEngine          TaskRouter
─────────────────                ─────────────          ──────────
cron "0 8 * * MON"   ──fire──►  render template  ──►   agent.run()
~/imports/*.pdf créé  ──fire──►  render template  ──►   pipeline.run()
POST /webhooks/push   ──fire──►  render template  ──►   agent.run()
```

---

## Cinq sources d'événements

| Type | Déclenchement | Cas d'usage |
|---|---|---|
| `cron` | Expression cron 5 champs | Rapport hebdomadaire chaque lundi à 8h |
| `interval` | Durée périodique | Vérifier la boîte mail toutes les 30 minutes |
| `oneshot` | Date-heure ISO unique | Exécuter une migration le 2026-04-15 à 02:00 |
| `file_watch` | Création/modification/suppression de fichier | Traiter chaque facture déposée dans un dossier |
| `webhook` | `POST /webhooks/:id` signé HMAC-SHA256 | Déclencher un déploiement sur push GitHub |

---

## Créer un trigger

Les triggers se créent via l'API REST et sont persistés dans SQLite (`triggers_def.db`). Il n'y a plus de section `[[triggers]]` dans `apollia.toml`.

```bash
# Trigger cron — rapport chaque lundi à 8h
curl -X POST http://localhost:7771/api/v1/triggers \
  -H "Content-Type: application/json" \
  -d '{
    "id": "rapport-hebdomadaire",
    "agent": "rapport-agent",
    "enabled": true,
    "on_busy": "queue",
    "source": {
      "type": "cron",
      "schedule": "0 8 * * MON"
    },
    "input_template": "Génère le rapport de la semaine {{week_iso}}"
  }'
```

Après création, le trigger est actif immédiatement — aucun redémarrage nécessaire.

---

## La politique on_busy

Que se passe-t-il si le trigger se déclenche alors que l'agent est déjà occupé ?

| Politique | Comportement |
|---|---|
| `"queue"` | La tâche est soumise et mise en file d'attente — elle s'exécutera quand l'agent se libère |
| `"drop"` | Le fire est ignoré, `TriggerSkipped` est émis, `skip_count` est incrémenté |

`"queue"` est adapté aux workflows où chaque déclenchement doit être traité. `"drop"` est adapté aux vérifications périodiques où sauter une occurrence est acceptable.

---

## Lister et inspecter les triggers

```bash
apollia-os trigger list

# ID                    AGENT           TYPE       ENABLED  FIRES  SKIPS  LAST FIRE
# rapport-hebdomadaire  rapport-agent   cron       ✓        42     3      2026-03-08 08:00
# check-inbox           mail-agent      interval   ✓        1204   89     2026-03-09 14:32
# import-csv            import-agent    file_watch ✓        17     0      2026-03-09 11:15
# github-push           deploy-agent    webhook    ✓        8      1      2026-03-08 16:47

apollia-os trigger status rapport-hebdomadaire
```

---

## Architecture — le TriggerEngine

`TriggerEngine` est l'acteur Tokio numéro 6 dans le Supervisor, démarré avant l'API Server. Chaque source est un `JoinHandle<>` indépendant qui envoie des `TriggerEvent` sur un canal `mpsc` vers l'engine central :

```
Supervisor
  └── TriggerEngine (acteur 6)
        ├── CronTrigger      (JoinHandle — calcul next occurrence, tokio::sleep)
        ├── IntervalTrigger  (JoinHandle — tokio::interval)
        ├── OneshotTrigger   (JoinHandle — tokio::sleep_until)
        ├── FileWatchTrigger (JoinHandle — notify v6, bridge sync→async)
        └── Webhook          (route axum — POST /webhooks/:id)
              │
              ▼ mpsc<TriggerEvent>
        TriggerEngine
              │ InputTemplate.render() + OnBusyPolicy
              ▼
        TaskRouter → agent.run() ou pipeline.run()
```

---

## Ce que vous allez apprendre

- **Section 1 — Cron, FileWatch, Webhook** : configurer chaque type de source, les variables de template disponibles, la validation HMAC-SHA256 des webhooks, les commandes CLI
- **Section 2 — Hot reload** : modifier un trigger sans redémarrer, activer/désactiver à chaud, consulter l'historique des fires, le comportement interne de reload
