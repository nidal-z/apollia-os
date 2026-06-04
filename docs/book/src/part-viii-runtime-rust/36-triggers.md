# Triggers

Les agents Apollia répondent à des invocations explicites : un humain envoie un message, un director délègue une tâche. Mais beaucoup de workflows réels sont pilotés par des **événements** : un fichier déposé, une heure atteinte, un webhook entrant.

Les triggers d'Apollia OS permettent de déclencher automatiquement un agent sans intervention humaine. Ils sont déclaratifs, persistés en SQLite, et modifiables à chaud sans redémarrer le runtime.

---

## Ce que fait un trigger

Un trigger surveille une source d'événements et, quand elle se déclenche, soumet une tâche à un agent via le `TaskRouter`. C'est tout : le trigger ne connaît pas l'agent, il ne sait pas ce qu'il fait, il se contente de déclencher.

```
Source d'événement               TriggerEngine          TaskRouter
─────────────────                ─────────────          ──────────
cron "0 8 * * MON"   ──fire──►  render template  ──►   agent.run()
~/imports/*.pdf créé  ──fire──►  render template  ──►   agent.run()
POST /webhooks/push   ──fire──►  render template  ──►   agent.run()
```

---

## Cinq sources d'événements

| Type | Déclenchement | Cas d'usage |
|---|---|---|
| `cron` | Expression cron 5 champs | Rapport hebdomadaire chaque lundi à 8h |
| `interval` | Durée périodique | Vérifier la boîte mail toutes les 30 minutes |
| `oneshot` | Date-heure ISO unique | Exécuter une migration le 2026-12-31 à 02:00 |
| `file_watch` | Création / modification / suppression | Traiter chaque facture déposée |
| `webhook` | `POST /webhooks/:id` signé HMAC-SHA256 | Déclencher un déploiement sur push GitHub |

---

## Créer un trigger

Les triggers se créent via l'API REST et sont persistés dans `triggers_def.db`. Pas de section TOML dans `apollia.toml`.

```bash
# Trigger cron : rapport chaque lundi à 8h
curl -X POST http://localhost:7771/api/v1/triggers \
  -H "Content-Type: application/json" \
  -d '{
    "id": "weekly-report",
    "agent": "report-agent",
    "enabled": true,
    "on_busy": "queue",
    "source": {"type": "cron", "schedule": "0 8 * * MON"},
    "input_template": "Generate the report for week {{week_iso}}"
  }'
```

Après création, le trigger est actif immédiatement, aucun redémarrage nécessaire.

Variante CLI :

```bash
apollia-os trigger create weekly-report \
  --agent report-agent \
  --kind cron \
  --detail "0 8 * * MON" \
  --input "Generate the report for week {{week_iso}}"
```

La CLI suit un schéma uniforme : `--kind <TYPE> --detail <VALUE>` quel que soit le type de trigger (`cron`, `interval`, `oneshot`, `file_watch`, `webhook`). `--detail` porte la valeur spécifique au type (expression cron, durée, date, glob, identifiant webhook).

---

## Politique `on_busy`

Que se passe-t-il si le trigger se déclenche pendant que l'agent est déjà occupé ?

| Politique | Comportement |
|---|---|
| `"queue"` | La tâche est soumise et mise en file d'attente, elle s'exécutera quand l'agent se libère |
| `"drop"` | Le fire est ignoré, `TriggerSkipped` est émis, `skip_count` incrémenté |

`queue` est adapté aux workflows où chaque déclenchement doit être traité. `drop` est adapté aux vérifications périodiques où sauter une occurrence est acceptable.

---

## File watch

```bash
apollia-os trigger create new-invoice \
  --agent invoice-router \
  --kind file_watch \
  --detail "~/imports/*.pdf" \
  --on-busy queue \
  --input "Process the new invoice at {{file_path}}"
```

Variables disponibles dans `input_template` pour file watch :

- `{{file_path}}` : chemin absolu du fichier détecté
- `{{file_name}}` : nom de fichier (sans dossier)
- `{{event_type}}` : `created`, `modified`, `removed`

Le file watcher utilise `notify` v6 côté Rust (inotify sur Linux, FSEvents sur macOS). Idempotent : un fichier créé puis modifié immédiatement compte comme deux fires.

---

## Webhook

```bash
apollia-os trigger create github-push \
  --agent ci-runner \
  --kind webhook \
  --detail github-push \
  --on-busy queue \
  --input "Push from {{headers.x-github-actor}}: {{body.ref}}"
```

Le secret HMAC-SHA256 est généré par le runtime à la création et exposé une seule fois dans la réponse JSON ; conservez-le pour signer les requêtes côté émetteur.

Le runtime expose alors `POST /webhooks/github-push` qui vérifie la signature HMAC-SHA256 en header `X-Apollia-Signature` avant de déclencher. Le payload JSON entrant est accessible dans `input_template` via `{{body.*}}` et `{{headers.*}}`.

---

## Cron, interval, oneshot

```bash
# Cron : 5 champs
apollia-os trigger create nightly --agent backup --kind cron --detail "0 2 * * *" --input "Run backup"

# Interval : durée
apollia-os trigger create inbox-check --agent mail-agent --kind interval --detail 30m --input "Check inbox"

# Oneshot : date-heure ISO 8601
apollia-os trigger create migration --agent migrator --kind oneshot --detail "2026-12-31T02:00:00Z" --input "Run migration"
```

Variables disponibles : `{{now_iso}}`, `{{date}}`, `{{time}}`, `{{week_iso}}`, `{{day_of_week}}`.

---

## Lister et inspecter

```bash
apollia-os trigger list

# ID                AGENT             TYPE       ENABLED  FIRES  SKIPS  LAST FIRE
# weekly-report     report-agent      cron       ✓        42     3      2026-05-19 08:00
# inbox-check       mail-agent        interval   ✓        1204   89     2026-05-20 14:32
# new-invoice       invoice-router    file_watch ✓        17     0      2026-05-20 11:15
# github-push       ci-runner         webhook    ✓        8      1      2026-05-19 16:47

apollia-os trigger status weekly-report
```

---

## Hot reload

Pour modifier un trigger sans redémarrer le runtime :

```bash
apollia-os trigger disable weekly-report
apollia-os trigger update weekly-report --kind cron --detail "0 9 * * MON"
apollia-os trigger enable weekly-report
```

Ou par patch direct via API :

```bash
curl -X PATCH http://localhost:7771/api/v1/triggers/weekly-report \
  -H "Content-Type: application/json" \
  -d '{"source": {"type": "cron", "schedule": "0 9 * * MON"}}'
```

Le `TriggerEngine` recharge la définition, arrête l'ancien handle, démarre le nouveau. Aucune interruption du runtime.

---

## Architecture interne

`TriggerEngine` est l'acteur Tokio dédié, démarré par le Supervisor avant l'APIServer. Chaque source est un `JoinHandle` indépendant qui envoie des `TriggerEvent` sur un canal `mpsc` vers l'engine central :

```
Supervisor
  └── TriggerEngine
        ├── CronTrigger      (JoinHandle : calcul next occurrence, tokio::sleep)
        ├── IntervalTrigger  (JoinHandle : tokio::interval)
        ├── OneshotTrigger   (JoinHandle : tokio::sleep_until)
        ├── FileWatchTrigger (JoinHandle : notify v6, bridge sync→async)
        └── Webhook          (route axum : POST /webhooks/:id)
              │
              ▼ mpsc<TriggerEvent>
        TriggerEngine
              │ InputTemplate.render() + OnBusyPolicy
              ▼
        TaskRouter → agent.run()
```

---

## Coût

Le `TriggerEngine` est conçu pour supporter des centaines de triggers actifs simultanément. Le coût mémoire est négligeable (un `JoinHandle` Tokio par trigger). Le coût CPU est nul tant qu'aucun trigger ne se déclenche.

Si vous avez des milliers de triggers (cas extrême), envisagez de regrouper par pattern : un trigger file_watch sur `~/imports/*` plutôt qu'un trigger par fichier prévu.

---

## ADRs

- `ADR-021` : Triggers TOML, HMAC, hot reload (note : la persistance TOML a été remplacée par SQLite v0.5+)
- `ADR-014` : Config opérateur en SQLite

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
