---
sidebar_position: 5
title: Écrire un workflow
---

# Écrire un workflow

Un workflow est une suite d'étapes que vous écrivez en Python et dont vous
décidez l'ordre. Chaque étape confie un message à un agent, lance un autre
workflow, pose une question à l'utilisateur ou exécute du Python. Les étapes
sont déclarées avant le code : le workflow se lit, se lance depuis un
formulaire et se suit étape par étape sans que personne n'ouvre le module.

Écrivez un workflow quand vous connaissez les étapes. Quand elles dépendent de
ce que le modèle trouve en chemin, confiez la tâche à un agent et laissez-le
planifier :
[Faire planifier un agent d'abord](/how-to/run-an-orchestrated-agent).

Ce guide ajoute un workflow à un paquet qui déclare déjà un agent nommé
`invoice-clerk`. Pour créer un tel paquet, voir
[Écrire un outil](/how-to/write-a-worker).

## Déclarer les entrées et les étapes

Créez `workflows/reminders.py` dans le dossier du paquet :

```python
"""Workflow of the package invoices: one reminder per late supplier."""

from typing import Annotated, TypedDict

from apollia import LongText, WorkflowCtx, workflow
from apollia.errors import QuestionDeclined


class Reminders(TypedDict):
    month: Annotated[str, "Month to check, for example 2026-09."]
    tone: Annotated[LongText, "How the reminders should sound."]


@workflow(
    "monthly-reminders",
    "Find the late invoices of a month and draft one reminder per supplier.",
    inputs=Reminders,
    steps=["collect", "confirm", "draft"],
    action="read_personal",
)
async def monthly_reminders(ctx: WorkflowCtx, inputs: Reminders) -> dict[str, str]:
    async with ctx.step("collect"):
        late = await ctx.agents.run(
            "invoice-clerk",
            f"List the invoices of {inputs['month']} that are late, one per line.",
        )

    async with ctx.step("confirm"):
        try:
            answer = await ctx.user.ask(
                f"Draft reminders for these invoices?\n{late}",
                options=["Yes", "No"],
            )
        except QuestionDeclined:
            answer = "No"
        if answer != "Yes":
            return {"late": late, "reminders": ""}

    async with ctx.step("draft"):
        reminders = await ctx.agents.run(
            "invoice-clerk",
            f"Draft one reminder per supplier.\nTone: {inputs['tone']}\n{late}",
        )

    return {"late": late, "reminders": reminders}
```

Ce que prend le décorateur :

| Argument | Sens |
| --- | --- |
| `name` | Identifiant du workflow, `[a-z0-9_-]+`, unique dans le paquet. Le workflow s'adresse par `<paquet>/<name>`. |
| `description` | Ce que fait le workflow, affiché sur sa fiche. |
| `inputs` | Un `TypedDict`. Son schéma est le formulaire de lancement du workflow. |
| `steps` | Les noms des étapes, dans l'ordre, sans doublon. |
| `action` | La famille d'action la plus sensible que le workflow accomplit. |

Remarques sur le contrat :

- La fonction est `async def` et prend exactement deux paramètres,
  `(ctx, inputs)`.
- Chaque étape du corps s'écrit `async with ctx.step("nom"):`. Un nom que le
  décorateur n'a pas déclaré est refusé par une `AgentConfigError` : la liste
  que montre la fiche et le code ne peuvent pas diverger.
- Le runtime enregistre chaque étape quand elle s'ouvre, se ferme ou lève une
  erreur, avec son exécution.
- Renvoyez une valeur sérialisable en JSON.
- Une déclaration qui ne tient pas (un nom hors de l'alphabet, une famille
  inconnue, une liste d'étapes vide, une entrée d'un type non pris en charge)
  est refusée à l'import du module.

## Types d'entrée

Chaque entrée est l'un des champs que connaît le formulaire de lancement. Tout
autre type est refusé.

| Champ | Type Python | Ce que reçoit le workflow |
| --- | --- | --- |
| Texte | `str` | Une chaîne. |
| Texte long | `apollia.LongText` | Une chaîne. |
| Fichier | `apollia.FileInput` | Le chemin absolu du fichier. |
| Date | `datetime.date` | Une `date`. |
| Nombre | `int`, `float` | Un nombre. |
| Oui ou non | `bool` | Un booléen. |
| Choix | `Literal[...]` | L'une des valeurs listées. |
| Liste | `list[str]`, `list[int]`, `list[float]` | Une liste. |

Le formulaire suit l'ordre dans lequel le `TypedDict` déclare ses entrées.

## Ce qu'une étape peut faire

Le `ctx` d'un workflow porte quatre choses en plus du journal :

| Appel | Effet |
| --- | --- |
| `await ctx.agents.run(agent_id, message)` | Confie `message` à un agent installé comme premier tour d'une conversation neuve, sous les accès de cet agent, et renvoie sa réponse finale. |
| `await ctx.workflows.run(workflow_id, inputs)` | Lance un autre workflow et renvoie ce qu'il a renvoyé. `workflow_id` vaut `<paquet>/<workflow>`, ou le nom seul d'un workflow du même paquet. Trois niveaux d'imbrication au plus. |
| `await ctx.user.ask(question, options=..., hint=...)` | Arrête l'exécution sur une question jusqu'à la réponse de l'utilisateur. Voir [Mettre un workflow en pause pour une décision humaine](/how-to/human-in-the-loop). |
| `ctx.step(name)` | Ouvre une étape déclarée. |

Un agent ou un workflow lancé depuis une étape s'exécute imbriqué dans
l'exécution : il est arrêté avec elle et puise dans son budget.

`ctx.llm` et `ctx.tools` valent `None` dans un workflow : le modèle et les outils
se joignent par un agent. Une étape qui a besoin de raisonnement, ou d'un outil,
confie le message à un agent avec `ctx.agents.run`, et cet agent travaille avec
les outils qui lui ont été accordés.

## Décrire, construire et exécuter

```bash
python -m apollia describe invoices
apollia-os package build invoices
apollia-os package install invoices-1.2.0.apo
```

`describe` écrit l'entrée `[[workflows]]` de `apollia.toml` depuis le
décorateur : nom, module, description, famille d'action, schéma d'entrée et
étapes.

Lisez ensuite le workflow tel qu'Apollia le voit, puis lancez-le :

```bash
apollia-os workflow show invoices/monthly-reminders
apollia-os workflow run invoices/monthly-reminders \
  --input month=2026-09 --input tone="Polite and short." --wait
```

`--input` est répétable, et chaque valeur est lue selon le type que le workflow
déclare pour elle. Sans `--wait`, la commande affiche un identifiant
d'exécution et rend la main :

```bash
apollia-os workflow status <run-id>
apollia-os workflow logs invoices/monthly-reminders
apollia-os workflow cancel <run-id>
```

`workflow status` montre l'exécution, son résultat et chacune de ses étapes.
`workflow cancel` interrompt l'étape en cours, et les suivantes ne démarrent
jamais.

## Pour aller plus loin

- Tester le workflow avec un contexte simulé :
  [Tester vos outils et vos workflows](/how-to/test-your-agents).
- Lancer le workflow selon un horaire : proposez un déclencheur dans le
  [manifeste de paquet](/reference/package-manifest#triggers).
- Chaque sous-commande `workflow` figure dans la [référence CLI](/reference/cli).
