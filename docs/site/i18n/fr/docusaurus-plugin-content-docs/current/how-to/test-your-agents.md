---
sidebar_position: 9.5
title: Tester vos outils et vos workflows
---

# Tester vos outils et vos workflows

Apollia propose deux niveaux de test. Les tests unitaires exercent un outil ou
un workflow en local avec un contexte simulé : ils s'exécutent en quelques
millisecondes, sans runtime ni modèle. Les évaluations de suite exécutent des
tâches complètes contre un runtime actif et notent les résultats. Utilisez les
tests unitaires pour la logique et les contrats, les évaluations pour le
comportement de bout en bout et le suivi des régressions.

Ceci est un guide pratique. Il suppose que vous avez déjà écrit un outil ou un
workflow ; sinon, voir [Écrire un outil](/how-to/write-a-worker) et
[Écrire un workflow](/how-to/write-a-director).

## Tests unitaires avec `apollia.testing`

Un outil et un workflow sont de simples fonctions `async def`, et un test les
attend telles qu'elles sont écrites. `MockContext` est le `ctx` qu'elles
reçoivent : il simule chaque service du contrat sauf `ctx.mail`. Vous réglez ce
que renvoient les services, vous attendez la fonction, et vous vérifiez le
résultat et les appels qu'elle a faits.

## Tester un outil

Prenez l'outil de [Écrire un outil](/how-to/write-a-worker), qui compte les pages
d'un PDF et lève une erreur typée quand le fichier manque :

```python
import pytest

from apollia import DomainError
from apollia.testing import MockContext
from tools.pdf_reader import count_pages


@pytest.mark.asyncio
async def test_count_pages_refuses_a_missing_file():
    # GIVEN a mocked context and a path that does not exist
    ctx = MockContext()

    # WHEN the tool runs
    with pytest.raises(DomainError) as raised:
        await count_pages(ctx, path="/nowhere/report.pdf")

    # THEN it fails with the code a caller can branch on
    assert raised.value.code == "FILE_NOT_FOUND"
```

`@tool` renvoie la fonction qu'il décore : le test l'appelle donc avec `ctx` et
les entrées en arguments nommés, comme le fait le runtime.

## Tester un workflow

Un workflow confie ses étapes à des agents, à d'autres workflows et à
l'utilisateur. Sous `MockContext`, chacun d'eux est un simulacre que vous
réglez, et les étapes ne rapportent rien. Prenez le workflow de
[Écrire un workflow](/how-to/write-a-director) :

```python
import pytest

from apollia.testing import MockContext
from workflows.reminders import monthly_reminders

INPUTS = {"month": "2026-09", "tone": "Polite and short."}
LATE = "ACME, invoice 42, due 2026-09-10"


@pytest.mark.asyncio
async def test_reminders_are_drafted_once_confirmed():
    # GIVEN an agent that lists one late invoice and a user who says yes
    ctx = MockContext()
    ctx.agents.answers = {"invoice-clerk": LATE}
    ctx.user.answers = {f"Draft reminders for these invoices?\n{LATE}": "Yes"}

    # WHEN the workflow runs
    result = await monthly_reminders(ctx, INPUTS)

    # THEN the agent ran twice, to collect and to draft, and the user was asked
    assert [agent for agent, _ in ctx.agents.run_calls] == ["invoice-clerk"] * 2
    assert len(ctx.user.ask_calls) == 1
    assert result["late"] == LATE


@pytest.mark.asyncio
async def test_nothing_is_drafted_when_the_user_declines():
    # GIVEN a user who declines the question
    ctx = MockContext()
    ctx.agents.answers = {"invoice-clerk": LATE}
    ctx.user.declined = {f"Draft reminders for these invoices?\n{LATE}": None}

    # WHEN the workflow runs
    result = await monthly_reminders(ctx, INPUTS)

    # THEN the run stops after the question, with no reminder
    assert len(ctx.agents.run_calls) == 1
    assert result["reminders"] == ""
```

Ce que prend et ce qu'enregistre chaque simulacre :

| Service | Régler | Relire |
| --- | --- | --- |
| `ctx.agents` | `answers`, un dictionnaire d'identifiant d'agent vers sa réponse. Un agent sans réponse renvoie une chaîne vide. | `run_calls`, une liste de `(agent_id, message)`. |
| `ctx.workflows` | `results`, un dictionnaire d'identifiant de workflow vers son résultat. Un workflow sans résultat renvoie `None`. | `run_calls`, une liste de `(workflow_id, inputs)`. |
| `ctx.user` | `answers`, un dictionnaire de question vers sa réponse ; `declined`, un dictionnaire de question vers un motif, qui fait lever `QuestionDeclined` à `ask`. | `ask_calls`, une liste de `(question, options, hint)`. |
| `ctx.llm` | `responses`, une liste FIFO de `{"content": ...}` ; chaque appel dépile la suivante. | `assert_llm_called(ctx, times=...)`. |
| `ctx.tools` | `responses`, un dictionnaire de nom d'outil vers son résultat. | `assert_tool_called(ctx, name, times=...)`. |
| `ctx.memory` | Enregistre et rappelle en mémoire vive. | `assert_memory_recorded(ctx, ...)`. |

Les décorateurs se comportent sous un test comme sous le runtime : une étape que
le décorateur n'a pas déclarée est refusée, et une entrée de date donnée sous la
forme `YYYY-MM-DD` est lue comme une `date`. Lancez ces tests avec `pytest`,
comme n'importe quels tests Python.

`MockContext` simule plus de services que le runtime n'en attache aujourd'hui à
un outil ou à un workflow : `ctx.tools` vaut `None` dans les deux, et `ctx.llm` dans un outil. Un test qui
passe par eux prouve la logique de votre fonction, pas que le service est
joignable là où la fonction s'exécute.

## Evals de suite avec `apollia-os eval run`

Une suite d'evals est un fichier TOML de tâches, chacune exécutée une ou
plusieurs fois contre le daemon en cours d'exécution, puis vérifiée par des
assertions. Utilisez-la pour détecter des régressions et pour noter des
comportements que les tests unitaires ne peuvent pas évaluer, comme la qualité
d'une réponse.

Le format de la suite se compose d'un `name` et d'une liste de `[[tasks]]` :

```toml
name = "smoke"

[[tasks]]
id = "write-report"
prompt = "Write a one-line report to /tmp/report.txt and print done."
runs = 4
agent = "writer"
assertions = [
  { type = "file_exists", path = "/tmp/report.txt" },
  { type = "regex", on = "result", pattern = "done" },
]
```

- `runs` vaut 3 par défaut. `agent` désigne l'agent ciblé (ou passez `--agent`
  en ligne de commande comme valeur par défaut pour les tâches qui l'omettent).
- Le `type` d'assertion est l'un de `exit_code` (`equals`), `file_exists`
  (`path`), `regex` (`on = "stdout"` ou `"result"`, plus `pattern`), et
  `llm_judge` (`rubric`).
- `llm_judge` est analysé mais jamais évalué. La commande construit
  le lanceur d'évaluation sans routeur de juge, et une assertion qui ne peut pas
  être vérifiée échoue au lieu de compter comme réussie : toute assertion
  `llm_judge` rend donc `llm judge not evaluated: this runner has no judge
  router` et fait rougir sa tâche. Configurer un backend LLM n'y change rien.
  Laissez `llm_judge` hors de toute suite qui sert de critère de blocage.

Exécutez une suite contre un daemon actif, puis relisez un résultat antérieur :

```sh
apollia-os eval run ./smoke.toml
apollia-os eval report ./smoke.results.jsonl
```

`eval run` écrit une ligne JSONL par exécution et affiche un résumé : taux de
réussite, durée réelle p50 et p95, et coût total. Les compteurs d'étapes et
d'appels d'outils sont bien rapportés, mais ils ne sont pas encore fiables : ne
vous en servez pas comme critère de blocage.

La forme de suite ci-dessus est illustrative ; le dépôt ne fournit pas de suite
prête à l'emploi à copier. Chaque option est documentée dans la [référence
CLI](/reference/cli), chaque champ d'une suite dans le
[schéma de suite d'évaluation](/reference/eval-suites), et les formes de service
figurent dans le [contrat SDK / ctx](/reference/sdk).

## Que choisir

- Contrat et logique (bon payload, bon outil, bonne erreur typée) : tests
  unitaires.
- Comportement de bout en bout, qualité et suivi des régressions entre les
  exécutions : evals.

La plupart des paquets ont besoin des deux : des tests unitaires rapides en CI
à chaque changement, et une suite d'evals exécutée contre un runtime avant une
release.
