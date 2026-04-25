# Suspend-Resume en Mode Direct

En Mode Direct, c'est l'agent Python qui contrôle le moment de la suspension. Il calcule, prépare les données, atteint un point de décision — puis retourne `AIPResult.input_required` pour remettre la décision à un humain.

---

## AIPResult.input_required

```python
return AIPResult.input_required(
    prompt="Confirmer l'envoi du devis de 12 400 € à Dupont SA ?",
    context={"amount": 12400, "client": "Dupont SA", "step": "validation"}
)
```

- `prompt` : le message affiché à l'opérateur pour prendre la décision
- `context` : un dict JSON sérialisable — l'état de l'agent au moment de la suspension, restitué intact à la reprise

`context` est la mémoire de travail de l'agent entre le moment de la suspension et la reprise. Mettez-y tout ce dont vous aurez besoin pour continuer sans recommencer depuis zéro.

---

## Ce qu'ORIA fait à la suspension

```
agent.run(task, ctx)
  └── AIPResult.input_required(prompt=..., context={...})

ORIA.execute_direct() :
  1. Persiste prompt + context → SQLite (task_hitl_state)
  2. Émet RuntimeEvent::TaskInputRequired { task_id, prompt, step_id: None }
  3. Enregistre un oneshot::Receiver dans PendingApprovals
  4. await rx  ← SUSPENSION PURE
       │
       └── POST /api/v1/tasks/{id}/resume
             └── ResumeHandler.resolve(task_id, approved, reason)
                   └── oneshot::Sender.send(response)

  Si approved=true :
    - Reconstruit AIPTask avec is_resumed=True + input_response peuplé
    - Rappelle agent.run(resumed_task, ctx)

  Si approved=false :
    - Retourne AIPResult::failed("REJECTED", reason)
    - run() n'est PAS rappelé
```

Pendant le `await rx`, aucun step n'est compté, aucun timeout ne s'écoule. La tâche est figée dans l'état `input_required`.

---

## Le pattern is_resumed

À la reprise, ORIA rappelle `agent.run` avec un `AIPTask` enrichi de deux champs :

```python
task.is_resumed      # True si c'est un rappel après décision HITL
task.input_response  # InputResponse avec approved, reason, context, responded_at
```

Le pattern standard pour un agent HITL :

```python
async def run(self, task, ctx):
    # Première exécution ou reprise ?
    if task.is_resumed:
        response = task.input_response
        if not response.approved:
            return AIPResult.failed(
                "REJECTED",
                f"Devis refusé : {response.reason}"
            )
        # Récupérer l'état sauvegardé au moment du suspend
        saved = response.context
        return await self._send_devis(saved["amount"], saved["client"], ctx)

    # Première exécution — calculer, puis demander validation
    amount = await self._calculate_total(task, ctx)
    client = task["input"]["text"]

    return AIPResult.input_required(
        prompt=f"Confirmer l'envoi du devis de {amount} € à {client} ?",
        context={"amount": amount, "client": client}
    )
```

**Règle clé** : toujours vérifier `task.is_resumed` en premier. Un agent qui ignore ce champ sera rappelé et recommencera le calcul depuis zéro — comportement incorrect.

---

## InputResponse — les champs disponibles

```python
class InputResponse:
    approved:     bool      # True = approuvé, False = rejeté
    reason:       str|None  # Raison si rejeté, None si approuvé
    context:      dict      # Dict JSON restitué depuis SQLite — état au moment du suspend
    responded_at: str       # Horodatage ISO 8601 de la décision
```

`context` est restitué tel quel depuis SQLite. Vérifiez vos types si vous y stockez des valeurs non-JSON-natives (`datetime`, `Decimal`, etc.) — ils seront sérialisés en chaînes.

---

## Reprendre via l'API

```bash
# Approuver
curl -X POST http://localhost:7771/api/v1/tasks/t-abc123/resume \
  -H "Content-Type: application/json" \
  -d '{"approved": true}'

# Rejeter avec raison
curl -X POST http://localhost:7771/api/v1/tasks/t-abc123/resume \
  -H "Content-Type: application/json" \
  -d '{"approved": false, "reason": "Montant trop élevé — renégocier"}'
```

Réponse :

```json
{"task_id": "t-abc123", "approved": true, "status": "working"}
```

La tâche repasse en `working` immédiatement après la décision.

---

## Suivre la suspension via SSE

```bash
curl -N http://localhost:7771/api/v1/tasks/t-abc123/stream
```

```
data: {"event":"task_started","task_id":"t-abc123"}
data: {"event":"step","step":1,"thought":"Calcul du montant..."}
data: {"event":"input_required","task_id":"t-abc123",
       "prompt":"Confirmer l'envoi du devis de 12 400 € à Dupont SA ?","step_id":null}
# — le flux reste ouvert, la tâche attend —
data: {"event":"task_resumed","task_id":"t-abc123","approved":true}
data: {"event":"step","step":2,"thought":"Envoi du devis..."}
data: {"event":"completed","result":{"status":"completed",...}}
```

`input_required` n'est pas un événement terminal — le flux reste ouvert jusqu'à la reprise et la complétion.

---

## TimeoutWatcher — annulation automatique

Le `TimeoutWatcher` scanne toutes les 60 secondes les tâches en état `input_required`. Celles qui dépassent le délai configuré (défaut : 24 heures) sont annulées automatiquement avec `status: "canceled"`.

Ce comportement garantit qu'aucune tâche ne reste suspendue indéfiniment si l'opérateur oublie de répondre.

Pour ajuster le délai dans `apollia.toml` :

```toml
[runtime]
input_required_timeout_hours = 48  # défaut : 24
```

---

## Multi-suspension

Un agent peut suspendre plusieurs fois dans la même exécution. Chaque suspension produit une entrée dans `task_approvals`. À chaque reprise, `task.input_response` contient la réponse de la **dernière** suspension — pas l'historique complet.

Pour un workflow avec plusieurs points de contrôle, utilisez `context` pour encoder le numéro d'étape :

```python
async def run(self, task, ctx):
    if task.is_resumed:
        step = task.input_response.context.get("step")
        if step == "validation_montant":
            return await self._etape_2(task.input_response.context, ctx)
        elif step == "validation_envoi":
            return await self._envoyer(task.input_response.context, ctx)

    # Étape 1 : calculer
    amount = await self._calculate(task, ctx)
    return AIPResult.input_required(
        prompt=f"Valider le montant : {amount} € ?",
        context={"step": "validation_montant", "amount": amount}
    )
```
