# run() — exécuter une tâche

`run()` est le cœur de tout agent. C'est la méthode que le runtime appelle à chaque tâche reçue. Elle prend deux paramètres — `task` et `ctx` — et retourne un résultat structuré.

```python
async def run(self, task, ctx):
    ...
```

Cette section détaille la structure complète de `task` (ce que le runtime envoie) et les formats de retour (ce que l'agent doit retourner).

---

## La structure de task — AIPTask

`task` est un dictionnaire Python. Voici tous ses champs :

```python
async def run(self, task, ctx):
    task_id    = task["task_id"]              # str — UUID généré par le runtime
    context_id = task["context_id"]          # str — groupe de tâches liées (même conversation)
    parts      = task["input"]["parts"]      # list[dict] — l'entrée, voir ci-dessous
    history    = task.get("history", [])     # list[dict] — messages précédents du context
    timeout    = task.get("timeout_seconds") # int | None — timeout fixé par l'appelant

    # Human-in-the-Loop (voir plus bas)
    is_resumed     = task["is_resumed"]       # bool — True si reprise après approbation humaine
    input_response = task["input_response"]  # InputResponse | None — réponse humaine
```

### task_id et context_id

`task_id` est l'identifiant unique de cette tâche — vous devez le recopier dans votre retour. `context_id` regroupe plusieurs tâches d'une même session ou conversation : il est identique pour toutes les tâches d'un même échange. Utilisez-le pour retrouver l'historique en mémoire.

### Les parties d'entrée — AIPPart

L'entrée d'une tâche est une liste de **parties** typées. Trois types existent :

```python
# TextPart — texte brut
{"type": "text", "text": "Résume /data/rapport.txt"}

# DataPart — données JSON structurées
{"type": "data", "data": {"client": "Acme", "budget": 5000, "currency": "EUR"}}

# FilePart — fichier encodé en base64 ou référencé par URI
{"type": "file", "name": "brief.pdf", "mime_type": "application/pdf",
 "data": "<base64>", "uri": None}
```

Un message peut combiner plusieurs types. Pattern d'accès défensif :

```python
async def run(self, task, ctx):
    parts = task["input"]["parts"]

    # Extraire les parties par type
    text_parts = [p["text"] for p in parts if p.get("type") == "text"]
    data_parts = [p["data"] for p in parts if p.get("type") == "data"]
    file_parts = [p for p in parts if p.get("type") == "file"]

    # Accéder au premier texte
    user_text = text_parts[0] if text_parts else ""

    # Accéder aux données structurées
    data = data_parts[0] if data_parts else {}
    client = data.get("client", "inconnu")
```

### history — l'historique de contexte

`history` contient les messages précédents du `context_id` courant. Utile pour les agents conversationnels qui doivent se souvenir des échanges précédents sans passer par la mémoire persistante.

```python
history = task.get("history", [])
# Chaque entrée : {"role": "user"|"assistant", "content": "...", "timestamp": "..."}
```

> `history` contient uniquement les messages de la session en cours — pas la mémoire persistante de `ctx.memory`. Ce sont deux mécanismes orthogonaux : `history` pour la session, `ctx.memory` pour le long terme.

---

## Les formats de retour — AIPResult

L'agent doit retourner un dictionnaire Python ou utiliser les factory methods de `AIPResult`.

### Format dict (toujours valide)

```python
# Succès
return {
    "task_id": task["task_id"],    # obligatoire — recopier l'identifiant reçu
    "status": "completed",
    "output": [
        {"type": "text", "text": "Résultat de la tâche"},
    ],
}

# Échec
return {
    "task_id": task["task_id"],
    "status": "failed",
    "error": {
        "code": "FILE_NOT_FOUND",      # code machine — utilisé par les pipelines
        "message": "Le fichier /data/rapport.txt est introuvable",  # lisible
    },
}
```

### Factory methods AIPResult

Le runtime injecte automatiquement la classe `AIPResult` dans votre contexte d'exécution — aucun import requis.

```python
# Équivalent au dict "completed" ci-dessus
return AIPResult.completed("Résultat de la tâche")

# Équivalent au dict "failed" ci-dessus
return AIPResult.failed("FILE_NOT_FOUND", "Le fichier /data/rapport.txt est introuvable")
```

Les factory methods sont plus concises et garantissent que tous les champs obligatoires sont présents. Utilisez-les quand vous n'avez pas besoin de construire l'output part par part.

### Les quatre statuts

| Statut | Signification | Champ supplémentaire |
|---|---|---|
| `"completed"` | Tâche terminée avec succès | `output` |
| `"failed"` | Erreur non récupérable | `error` avec `code` et `message` |
| `"input_required"` | Tâche suspendue, attente humaine | `input_required_data` |
| `"canceled"` | Annulée par le runtime ou l'opérateur | — |

### Retourner plusieurs parties en output

```python
return {
    "task_id": task["task_id"],
    "status": "completed",
    "output": [
        {"type": "text", "text": "Résumé du rapport :"},
        {"type": "text", "text": summary_content},
        {"type": "data", "data": {"word_count": 342, "file_path": summary_path}},
    ],
}
```

L'output est une liste de parties — même format que l'entrée. Un pipeline ou un autre agent peut ainsi extraire les parties structurées (`type: "data"`) indépendamment du texte.

---

## Human-in-the-Loop — suspendre et reprendre

Certaines tâches requièrent une décision humaine avant de continuer : confirmer un envoi d'email, valider un montant, approuver une modification de fichier critique. L'AIP gère ce cas via le statut `input_required`.

### Premier appel — suspendre

```python
async def run(self, task, ctx):
    if not task["is_resumed"]:
        # Première exécution — générer et demander confirmation
        amount = task["input"]["parts"][0].get("data", {}).get("amount", 0)
        email  = task["input"]["parts"][0].get("data", {}).get("email", "")

        # Suspendre la tâche et notifier l'opérateur
        return AIPResult.input_required(
            prompt=f"Confirmer l'envoi du devis ({amount}€) à {email} ?",
            context={"amount": amount, "email": email},  # sérialisé dans SQLite
        )
    ...
```

Le runtime :
1. Persiste `prompt` et `context` dans SQLite
2. Passe la tâche en `status = input_required`
3. Notifie l'opérateur (canaux configurés : CLI, webhook, bureau)

### Deuxième appel — reprendre

```bash
$ apollia-os task resume <task-id> --approve
# ou
$ apollia-os task resume <task-id> --reject --reason "Montant trop élevé"
```

Le runtime rappelle `run()` avec `is_resumed=True` et `input_response` peuplé :

```python
async def run(self, task, ctx):
    if not task["is_resumed"]:
        ...  # voir ci-dessus

    # Reprise — lire la décision humaine
    ir = task["input_response"]

    if ir.approved:
        email  = ir.context["email"]    # restauré depuis SQLite
        amount = ir.context["amount"]
        # Procéder avec l'action confirmée
        await ctx.tools.call("smtp", {"to": email, "subject": f"Devis {amount}€", ...})
        return AIPResult.completed(f"Devis envoyé à {email}")
    else:
        reason = ir.reason or "refusé"
        return AIPResult.failed("REJECTED", f"Envoi annulé : {reason}")
```

### Les champs d'InputResponse

| Champ | Type | Description |
|---|---|---|
| `ir.approved` | `bool` | `True` si approuvé, `False` si rejeté |
| `ir.reason` | `str \| None` | Raison fournie par l'humain (`None` si approuvé) |
| `ir.context` | `dict` | Contexte exact passé à `input_required()` |
| `ir.responded_at` | `str` | Horodatage ISO 8601 de la décision |

### Appliquer à file-assistant

Supposons que `file-assistant` doit demander confirmation avant d'écraser un fichier résumé existant :

```python
async def run(self, task, ctx):
    ...
    # Si le fichier résumé existe déjà
    existing = await ctx.tools.call("file_read", {"path": summary_path})
    if not existing.get("error") and not task["is_resumed"]:
        # Fichier existant — demander confirmation
        return AIPResult.input_required(
            prompt=f"{summary_path} existe déjà. Écraser ?",
            context={"file_path": file_path, "summary_path": summary_path,
                     "new_summary": summary},
        )

    # Soit pas de fichier existant, soit l'humain a approuvé
    if task["is_resumed"]:
        if not task["input_response"].approved:
            return AIPResult.failed("CANCELED", "Écrasement refusé par l'utilisateur")
        # Restaurer depuis le contexte sauvegardé
        summary      = task["input_response"].context["new_summary"]
        summary_path = task["input_response"].context["summary_path"]

    await ctx.tools.call("file_write", {"path": summary_path, "content": summary})
    ...
```

---

## Bonnes pratiques pour run()

**Vérifier avant d'avancer** — chaque source d'erreur potentielle est vérifiée avant de passer à l'étape suivante. Ne jamais supposer que `ctx.llm` est disponible, que le fichier existe, ou que l'entrée est bien formée.

**Retourner des codes machine dans `error.code`** — les codes comme `FILE_NOT_FOUND`, `LLM_UNAVAILABLE`, `MISSING_INPUT` permettent aux pipelines et orchestrateurs de réagir programmatiquement aux échecs. Choisissez des codes stables et documentez-les.

**Recopier task_id** — toujours inclure `"task_id": task["task_id"]` dans le retour. Le runtime utilise ce champ pour corréler la réponse à la requête originale, surtout en mode concurrent.

**Ne pas lever d'exceptions non gérées** — si une exception Python traverse `run()`, le runtime la catchera et marquera la tâche comme `failed`, mais le message sera peu lisible. Préférez des retours `status: "failed"` explicites.
