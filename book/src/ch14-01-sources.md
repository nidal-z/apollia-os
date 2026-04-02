# Cron, FileWatch, Webhook

Cinq types de sources permettent de déclencher un agent automatiquement. Cette section explique quand et comment utiliser chacun, avec un exemple concret.

> **Référence technique :** [Briques-Triggers](https://github.com/nidal-z/apollia-os/wiki/Briques-Triggers) — variables de template, endpoints REST, schéma SQLite, comportements `on_busy`.

---

## Cron — expression planifiée

```bash
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
    "input_template": "Génère le rapport de la semaine {{week_iso}} ({{date_iso}})"
  }'
```

Le champ `schedule` accepte les expressions cron 5 champs standard :

```
┌──── minute (0-59)
│ ┌──── heure (0-23)
│ │ ┌──── jour du mois (1-31)
│ │ │ ┌──── mois (1-12)
│ │ │ │ ┌──── jour de la semaine (0-6, SUN=0 ou MON-SUN)
│ │ │ │ │
0 8 * * MON   → chaque lundi à 8h00
0 */6 * * *   → toutes les 6 heures
30 9 1 * *    → le 1er de chaque mois à 9h30
0 0 * * *     → tous les jours à minuit
```

Les variables `{{week_iso}}`, `{{date_iso}}`, `{{fired_at}}` et `{{trigger_id}}` sont disponibles dans `input_template`. La liste complète est dans [Briques-Triggers](https://github.com/nidal-z/apollia-os/wiki/Briques-Triggers).

---

## Interval — périodicité simple

```bash
curl -X POST http://localhost:7771/api/v1/triggers \
  -H "Content-Type: application/json" \
  -d '{
    "id": "check-inbox",
    "agent": "mail-agent",
    "enabled": true,
    "on_busy": "drop",
    "source": {
      "type": "interval",
      "every": "30m"
    },
    "input_template": "Vérifie la boîte mail et traite les nouveaux messages"
  }'
```

Formats acceptés pour `every` : `"300s"`, `"30m"`, `"1h"`, `"6h"`, `"1d"`.

`"drop"` est recommandé pour les vérifications périodiques — si l'agent est encore en train de traiter la dernière vérification, sauter l'occurrence est préférable à empiler des tâches.

---

## Oneshot — exécution unique

```bash
curl -X POST http://localhost:7771/api/v1/triggers \
  -H "Content-Type: application/json" \
  -d '{
    "id": "migration-q2",
    "agent": "migration-agent",
    "enabled": true,
    "on_busy": "queue",
    "source": {
      "type": "oneshot",
      "at": "2026-04-15T02:00:00Z"
    },
    "input_template": "Exécute la migration de base de données Q2"
  }'
```

Le trigger se déclenche une seule fois à l'heure spécifiée, puis passe en `enabled: false` automatiquement. Utile pour planifier des opérations de maintenance ponctuelles.

---

## FileWatch — surveillance de dossier

```bash
curl -X POST http://localhost:7771/api/v1/triggers \
  -H "Content-Type: application/json" \
  -d '{
    "id": "import-factures",
    "agent": "ocr-agent",
    "enabled": true,
    "on_busy": "queue",
    "source": {
      "type": "file_watch",
      "path": "~/factures/entrant/",
      "events": ["create"]
    },
    "input_template": "Traite la facture : {{filepath}}"
  }'
```

Le champ `events` accepte `"create"`, `"modify"`, et `"delete"`. `~` dans `path` est résolu au démarrage du trigger.

Les variables disponibles (`{{filepath}}`, `{{filename}}`, `{{size_bytes}}`, `{{file_event}}`, `{{fired_at}}`) sont documentées dans [Briques-Triggers](https://github.com/nidal-z/apollia-os/wiki/Briques-Triggers).

**Pointer vers un pipeline** au lieu d'un agent :

```bash
curl -X POST http://localhost:7771/api/v1/triggers \
  -H "Content-Type: application/json" \
  -d '{
    "id": "import-factures-pipeline",
    "pipeline": "traitement-facture",
    "enabled": true,
    "on_busy": "queue",
    "source": {
      "type": "file_watch",
      "path": "~/factures/entrant/",
      "events": ["create"]
    },
    "input_template": "{{filepath}}"
  }'
```

`agent` et `pipeline` sont mutuellement exclusifs — l'API retourne une erreur si les deux sont présents ou si aucun n'est fourni.

---

## Webhook — appel externe signé

```bash
curl -X POST http://localhost:7771/api/v1/triggers \
  -H "Content-Type: application/json" \
  -d '{
    "id": "github-push",
    "agent": "deploy-agent",
    "enabled": true,
    "on_busy": "drop",
    "source": {
      "type": "webhook",
      "secret": "un-secret-robuste-de-32-caracteres-minimum"
    },
    "input_template": "Déploie la branche : {{webhook_body}}"
  }'
```

Le secret doit faire au moins 32 caractères — la validation échoue avant l'écriture SQLite si cette contrainte n'est pas respectée.

### Appeler le webhook

```bash
# Calculer la signature HMAC-SHA256
BODY='{"ref": "refs/heads/main"}'
SECRET="un-secret-robuste-de-32-caracteres-minimum"
SIG=$(echo -n "$BODY" | openssl dgst -sha256 -hmac "$SECRET" | cut -d' ' -f2)

# Appeler le webhook
curl -X POST http://localhost:7771/webhooks/github-push \
  -H "X-Apollia-Signature: sha256=$SIG" \
  -H "Content-Type: application/json" \
  -d "$BODY"
```

### Séquence de validation

```
POST /webhooks/github-push
  1. TriggerEngine existe ?                      → sinon 503
  2. trigger_id "github-push" existe ?           → sinon 404
  3. HMAC-SHA256(secret, body) calculé
  4. Comparaison constant_time_eq avec header    → sinon 401
  5. TriggerEvent envoyé sur le canal mpsc       → 200 OK
```

La comparaison est en temps constant (`constant_time_eq`) pour éviter les attaques par timing. La variable `{{webhook_body}}` contient le corps brut de la requête POST.

---

## Déclencher manuellement (test)

```bash
# Déclencher immédiatement sans attendre l'événement source
apollia-os trigger fire rapport-hebdomadaire
# ✔ trigger rapport-hebdomadaire déclenché → task t-abc123
```

Utile pour tester un nouveau trigger sans attendre la prochaine occurrence cron ou le prochain fichier.

---

> **Référence complète :** [Briques-Triggers](https://github.com/nidal-z/apollia-os/wiki/Briques-Triggers) — toutes les variables de template par source, endpoints REST (`POST /api/v1/triggers`, `PUT`, `DELETE`, `GET`, `POST /webhooks/{id}`), schéma SQLite, et sémantique `on_busy` (queue / drop / fail).
