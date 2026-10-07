---
sidebar_position: 10
title: Mettre un workflow en pause pour une décision humaine
---

# Mettre un workflow en pause pour une décision humaine

Parfois une exécution ne doit pas décider seule : une règle manque, une valeur
est incertaine, une étape lourde de conséquences exige un feu vert. Un workflow
s'arrête sur une question avec `ctx.user.ask`, attend la réponse, et continue
avec elle. Ce guide couvre cet appel, la façon dont la question reçoit sa
réponse, et ce qui sollicite un humain sans aucun code de votre part.

Ceci est le côté auteur. Pour la vue de l'utilisateur dans l'application de
bureau, voir
[Approuver ou refuser une action](/operator-help/control/approve-or-reject-an-action).

## Poser une question depuis une étape

Dans une étape, `await ctx.user.ask(...)` arrête l'exécution jusqu'à la réponse
de l'utilisateur :

```python
from typing import TypedDict

from apollia import WorkflowCtx, workflow
from apollia.errors import QuestionDeclined

RULES = {"ACME": "suppliers"}


class Routing(TypedDict):
    vendor: str
    amount: float


@workflow(
    "route-invoice",
    "Decide where an invoice is filed, asking when no rule covers the vendor.",
    inputs=Routing,
    steps=["lookup", "decide"],
    action="read",
)
async def route_invoice(ctx: WorkflowCtx, inputs: Routing) -> dict[str, str]:
    async with ctx.step("lookup"):
        folder = RULES.get(inputs["vendor"])

    async with ctx.step("decide"):
        if folder is None:
            try:
                folder = await ctx.user.ask(
                    f"No rule for {inputs['vendor']} ({inputs['amount']:.2f}). "
                    "Where should it be filed?",
                    options=["to-review", "suppliers", "rejected"],
                    hint="Another folder",
                )
            except QuestionDeclined as declined:
                ctx.logger.info("no folder chosen: %s", declined.reason)
                folder = "to-review"

    return {"folder": folder}
```

La signature est `ask(question, *, options=None, hint=None)` et l'appel renvoie
la réponse sous forme de chaîne :

| Argument | Sens |
| --- | --- |
| `question` | Ce qui est demandé, tel que l'utilisateur le lit. |
| `options` | Les choix proposés, 20 au plus. |
| `hint` | Un texte d'invite pour la réponse libre. |

L'utilisateur peut toujours donner une réponse libre à la place d'un choix.
Vérifiez la réponse avant de brancher dessus : c'est l'une des `options` ou
n'importe quel texte.

## Ce que fait l'attente

L'exécution attend dans l'étape qui a posé la question. La question figure parmi
les décisions en attente de l'utilisateur, avec l'exécution, le workflow et
l'étape d'où elle vient. L'attente dure jusqu'à la réponse de l'utilisateur ou
l'arrêt de l'exécution :

- l'attente consomme le temps réel du budget de l'exécution ;
- arrêter l'exécution met fin à l'attente ;
- une exécution en attente quand le runtime s'arrête est mise en échec au
  démarrage suivant, comme toute exécution interrompue.

Rédigez une question claire. Sa qualité fait la qualité de la décision.

- Faible : `"Continue?"`
- Mieux : `"No rule for Acme Corp (1240.00). Where should it be filed?"`

Gardez les secrets et les données personnelles superflues hors d'une question :
elle est stockée et affichée.

## Répondre à la question

En ligne de commande, listez ce qui attend, puis répondez à un élément par son
identifiant :

```sh
apollia-os approvals pending
apollia-os approvals decide <id> answer --value "suppliers"
apollia-os approvals decide <id> refuse --reason "file it by hand this quarter"
```

`answer` remet `--value` à l'exécution, qui continue : `ask` la renvoie.
`refuse` décline la question : `ask` lève `QuestionDeclined`, qui porte la
`question` et la `reason`.

## Quand l'utilisateur décline

`QuestionDeclined` est à vous d'attraper. Attrapée, le workflow continue sans la
réponse, comme l'exemple le fait avec un dossier par défaut. Laissée sans
traitement, l'étape qui a posé la question échoue, et l'exécution avec elle.

## Tester les deux branches

Sous `MockContext`, `ctx.user` est un simulacre : `answers` associe une question
à sa réponse, et `declined` associe une question à un motif et fait lever `ask`.
Voir [Tester vos outils et vos workflows](/how-to/test-your-agents).

## Ce qui sollicite sans code

Deux pauses ne demandent aucun code dans un paquet.

Un agent en conversation interroge l'utilisateur par l'outil natif `ask_user`.
C'est le pendant de `ctx.user.ask` pour un agent : le modèle décide de poser la
question, et la réponse revient comme résultat de l'outil.

Un appel d'outil sensible demande une approbation. Chaque outil déclare une
famille d'action, et toute famille sauf `read` est sensible : avant qu'un tel
appel ne s'exécute, l'utilisateur est sollicité, sauf s'il l'a déjà autorisé. La
même commande `approvals decide` y répond, avec `once` pour cet appel, `always`
pour cet appel et tous les suivants du même agent dans la même famille, ou
`refuse`. Une politique d'organisation ou une règle de refus rejette l'appel
d'emblée, sans question. C'est pourquoi la famille que déclare un outil doit
dire ce qu'il fait réellement : voir [Écrire un outil](/how-to/write-a-worker).

<!-- claim:mcp-requires-approval-gates-task-path -->
Un serveur MCP déclaré comme exigeant une approbation (`requires_approval`)
n'ajoute aucune pause qui lui soit propre quand un agent appelle l'un de ses
outils. Ce qui sollicite avant l'appel est la règle ci-dessus : la gouvernance
de l'exécution, par famille d'action.

## Ce qu'un outil ne peut pas faire

`ctx.user` n'est lié que dans l'exécution d'un workflow. Un outil ne met rien en
pause : quand il lui manque quelque chose, il renvoie ce qu'il a trouvé ou lève
une `DomainError`, et l'agent qui l'a appelé décide s'il interroge l'utilisateur.

`NeedHumanInput`, `ctx.is_resumed` et `ctx.input_response` appartiennent à une
tâche qui se met en pause et qui est rejouée depuis le début à sa reprise. Ni un
outil ni un workflow ne sont repris ainsi : levée là, `NeedHumanInput` fait
échouer l'appel ou l'exécution comme toute autre exception.

## Voir aussi

- [Paliers d'autonomie](/explanation/autonomy-tiers) pour la place des
  approbations dans le modèle d'autonomie.
- Les commandes `approvals` dans la [référence CLI](/reference/cli).
- [Écrire un workflow](/how-to/write-a-director) pour les étapes où se loge une
  question.
