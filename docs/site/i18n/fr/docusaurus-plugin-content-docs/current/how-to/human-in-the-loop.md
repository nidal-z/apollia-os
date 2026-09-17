---
sidebar_position: 10
title: Mettre un agent en pause pour une décision humaine
---

# Mettre un agent en pause pour une décision humaine

Parfois un agent ne doit pas décider seul : une action sensible exige un
feu vert, une règle manque, une valeur est incertaine. Apollia permet à
l'auteur de mettre en pause une tâche en cours d'exécution et d'exiger une
décision humaine avant qu'elle ne continue. Ce guide couvre la primitive
côté auteur, `NeedHumanInput`, et la façon dont la pause est résolue.

Ceci est le point de vue de l'auteur. Pour le point de vue de l'opérateur
qui approuve ou rejette depuis l'application desktop, voir l'aide
opérateur. Pour la façon dont les paliers d'autonomie évoluent quand une
approbation est requise, voir [Paliers d'autonomie](/explanation/autonomy-tiers).

## Lever `NeedHumanInput` pour mettre en pause

À l'intérieur d'un skill, levez `NeedHumanInput` quand une personne doit
trancher. Elle prend un `prompt` affiché à l'humain et un dict `context`
optionnel, persisté et restauré autour de la pause.

```python
from apollia import agent, skill, NeedHumanInput
from apollia.types import Ctx


@agent(name="invoice-router", version="0.1.0", description="Route invoices.")
class InvoiceRouter:
    @skill("invoice.route", description="Decide where to file an invoice.")
    async def route(self, vendor: str, amount: float, ctx: Ctx) -> dict:
        folder = await self._lookup_rule(vendor, ctx)
        if folder is None:
            raise NeedHumanInput(
                prompt=f"No rule for {vendor} ({amount:.2f}). Approve filing under 'to-review'?",
                context={"vendor": vendor, "amount": amount},
            )
        return {"folder": folder}
```

Le constructeur est `NeedHumanInput(prompt: str, context: dict | None = None, *,
payload: dict | None = None)`. C'est une sous-classe d'`AgentError`, importée
depuis la racine du paquet (`from apollia import NeedHumanInput`).

## Poser une question typée ou demander une approbation

Un prompt seul reçoit un oui ou un non. Un `payload` type la pause : l'opérateur
se voit proposer les réponses que vous attendez, et votre skill en reçoit une.
Il existe deux formes, toutes deux déclarées en `TypedDict` dans `apollia.hitl`.

Une question porte un `genre` parmi `choix`, `source`, `seuil`, `definition` et
`confirmation`, la `question` elle-même, et en option des `propositions` (chacune
avec un `id` et un `libelle`), `autre` (texte libre accepté), `portee` et
`memoire` :

```python
from apollia import NeedHumanInput
from apollia.hitl import QuestionPayload

question: QuestionPayload = {
    "genre": "choix",
    "question": "Which list should be purged?",
    "propositions": [
        {"id": "leads", "libelle": "Leads"},
        {"id": "clients", "libelle": "Clients"},
    ],
}
raise NeedHumanInput("Which list should be purged?", payload=question)
```

Une approbation nomme un `geste`, un `risque` parmi `low`, `medium` et `critical`,
et en option des lignes de `detail` et un `delai` en secondes :

```python
from apollia.hitl import ApprovalPayload

approval: ApprovalPayload = {
    "genre": "approbation",
    "geste": "crm/purge",
    "risque": "critical",
    "detail": ["list: clients"],
}
raise NeedHumanInput("Purge the list?", {"list": "clients"}, payload=approval)
```

Le runtime vérifie le payload au moment où le skill se met en pause. Un payload
qui ne se lit pas, un `genre` inconnu, une `question` vide, deux propositions de
même `id`, fait échouer la tâche avec le code `INVALID_INPUT_PAYLOAD`. Il n'est
jamais enregistré ni affiché.

`AIPResult.input_required(prompt, context, payload=...)` renvoyé par un skill met
la tâche en pause de la même façon.

## Ce que la pause déclenche

Quand un skill lève `NeedHumanInput`, le dispatcher la transforme en un résultat
de statut `input_required` qui porte le `prompt`, le `context` et le `payload`. Le
runtime suspend la tâche, persiste son état et la présente à l'opérateur. La tâche
attend : une minute ou une semaine, l'état reste en place jusqu'à la réponse d'un
humain.

Rédigez un prompt clair. Sa qualité conditionne celle de la décision.

- Faible : `"Continue?"`
- Mieux : `"No rule for 'Acme Corp' (1240.00). Approve filing under 'to-review'?"`

Ne mettez dans `context` ni secret ni donnée personnelle inutile. Il est
sérialisé, stocké et affiché dans l'interface.

## Résoudre la pause

Un opérateur voit les tâches en attente et y répond. Depuis la CLI :

```sh
# Lister les tâches en attente d'une décision humaine
apollia-os task list --pending-approval

# Approuver, ou rejeter avec une raison
apollia-os task resume <task-id> --approve
apollia-os task resume <task-id> --reject --reason "file it manually this quarter"

# Consulter les décisions déjà résolues
apollia-os task approvals
```

Par l'API, `GET /api/v1/tasks?status=input_required` liste chaque tâche en attente
avec son `agent`, son `skill`, sa date `created_at`, son `prompt` et son `payload` ;
`GET /api/v1/approvals/pending` liste les mêmes pauses avec leur `agent_name`, leur
`skill_id`, leur `prompt`, leur `context`, leur `payload` et leur `suspended_at`. Et
`POST /api/v1/tasks/{id}/resume` prend `{"approved", "reason", "answer"}`. La
réponse `answer` est vérifiée contre le payload en attente :

- un `choix`, une `source` ou une `definition` prend l'`id` d'une proposition, ou
  du texte libre quand `autre` est activé ;
- un `seuil` prend un nombre, une `confirmation` un booléen, ou l'`id` d'une
  proposition ;
- une approbation ne prend pas d'`answer` : `approved` est la décision ;
- une question approuvée exige une `answer`, une question refusée non.

Une réponse qui ne correspond pas est refusée en `422` avec le code
`INVALID_ANSWER`, et la tâche reste en pause : l'opérateur peut répondre à nouveau.

Une pause sans payload garde son comportement : la rejeter termine la tâche,
l'approuver la laisse continuer.

## Ce que votre skill reçoit à la reprise

Une tâche reprise exécute à nouveau votre skill depuis le début. Deux attributs
lui disent où il en est :

- `ctx.is_resumed` vaut `True` lors d'une reprise ;
- `ctx.input_response` vaut `None` au premier passage, et sinon un dict avec
  `approved`, `reason`, `answer`, `payload` (la pause à laquelle on répond) et
  `context` (celui que vous avez passé en vous mettant en pause).

```python
@skill("list.purge")
async def purge(self, ctx: Ctx) -> dict:
    response = ctx.input_response
    if response is None:
        raise NeedHumanInput("Which list should be purged?", payload=question)
    if response["payload"]["genre"] == "choix":
        chosen = response["answer"]
        raise NeedHumanInput(f"Purge {chosen}?", {"list": chosen}, payload=approval)
    if not response["approved"]:
        return {"purged": None, "reason": response["reason"]}
    return {"purged": response["context"]["list"]}
```

Un skill peut se mettre en pause autant de fois que nécessaire. Chaque reprise ne
rend que la réponse à la dernière pause : faites porter ce qu'une pause précédente
a appris par le `context` de la suivante, comme ci-dessus.

Une pause refusée qui porte un payload reprend le skill avec `approved` à `False`,
pour que le skill décide de ce que signifie un refus. Gardez idempotente la
condition qui déclenche une pause, puisque le skill est repris depuis le début.

Pour tester la branche de reprise sans runtime, `MockContext.resume_with(...)` met
un contexte simulé dans l'état d'une tâche reprise.

## Il n'existe pas de forme déclarative sur `@skill`

Lever `NeedHumanInput` est la seule porte qu'un agent ouvre depuis son propre code.
Le décorateur ne prend aucun argument d'approbation : ses paramètres sont
`skill_id`, `description`, `dangerous` et `examples`, et passer
`requires_approval=True` lève `TypeError: skill() got an unexpected keyword
argument 'requires_approval'` à l'import, si bien que l'agent ne se charge jamais.
`dangerous=True` est une métadonnée du manifeste : elle signale le skill à
l'outillage d'inspection et n'insère aucune pause.

<!-- claim:mcp-requires-approval-gates-task-path -->
Un serveur MCP déclaré comme exigeant une approbation (`requires_approval`), ou un
outil MCP que le manifeste de l'agent liste dans `tools_requiring_approval`, soumet
ses appels à une porte sur le chemin des tâches. Un appel depuis `ctx.tools.call`
met la tâche en pause sur une approbation dont le `geste` est `<serveur>/<outil>` et
dont le `detail` liste les arguments. Approuvé, l'appel s'exécute une fois à la
reprise ; refusé, `ctx.tools.call` lève `apollia.errors.ToolApprovalDenied`. Une
pause levée ainsi garde le `context` avec lequel l'exécution a repris : un skill
repris depuis sa propre pause relit son état quand il reprend depuis celle-ci ; à
une première exécution, ce contexte est vide. Interceptez le `NeedHumanInput` et
relevez-le avec votre propre `context` pour faire traverser autre chose.
L'approbation ne couvre que cet appel, avec ces arguments. Un agent exécuté depuis
une session de chat agent passe par la même porte ; l'assistant du chat libre n'y
passe pas, il demande avant tout appel d'outil qu'on ne lui a pas autorisé.

<!-- claim:mcp-gated-tool-does-not-block-install -->
Déclarer un tel outil dans `tools_required` ne demande rien de plus : l'agent
s'installe, et l'approbation est tenue appel par appel à l'exécution. Le
`dangerous_tools_allowed` du manifeste n'y joue aucun rôle.

## Voir aussi

- [Paliers d'autonomie](/explanation/autonomy-tiers) pour la façon dont les
  approbations requises s'articulent avec le modèle d'autonomie.
- Les commandes `task` dans la [référence CLI](/reference/cli).
- Le [contrat SDK / ctx](/reference/sdk) pour les surfaces qu'utilise un skill.
