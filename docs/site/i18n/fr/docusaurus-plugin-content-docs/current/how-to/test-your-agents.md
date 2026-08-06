---
sidebar_position: 9
title: Testez vos agents
---

# Testez vos agents

Apollia propose deux niveaux de test. Les tests unitaires exercent un seul skill ou
message en local (in-process) avec un contexte simulé, donc ils s'exécutent en
quelques millisecondes, sans daemon ni modèle. Les evals de suite exécutent des
tâches complètes contre un daemon actif et notent les résultats, y compris avec un
juge LLM. Utilisez les tests unitaires pour la logique et les contrats, les evals
pour le comportement de bout en bout et le suivi des régressions.

Ceci est un guide pratique. Il suppose que vous avez déjà écrit un agent ; si ce
n'est pas le cas, consultez [Écrire un worker](/how-to/write-a-worker).

## Tests unitaires avec `apollia.testing`

Le harnais `apollia.testing` fait passer un skill ou un message par le même
chemin de dispatch que celui utilisé par le runtime, mais en local et face à un
`ctx` simulé. `mock(YourAgent)` retourne l'instance de l'agent ainsi qu'un
`MockContext` dont les 14 surfaces de service sont toutes simulées. Vous mettez
en file d'attente ce que les services doivent retourner, invoquez un skill, puis
vérifiez le résultat ainsi que les appels effectués par l'agent.

Le contrat `Ctx` complet compte 15 services : la surface mail (`ctx.mail`) n'est
pas simulée. Un agent qui utilise `ctx.mail` a besoin d'un test d'intégration
contre un runtime réel plutôt que d'un test unitaire avec `MockContext`.

Prenons un skill qui résume un texte avec le LLM et écrit le résumé dans un
fichier :

```python
from apollia import agent, skill
from apollia.types import Ctx


@agent(name="summarizer", version="0.1.0", description="Summarize and save text.")
class Summarizer:
    @skill("doc.summarize", description="Summarize text and write it to a file.")
    async def summarize(self, text: str, out_path: str, ctx: Ctx) -> dict:
        response = await ctx.llm.complete(
            messages=[{"role": "user", "content": f"Summarize:\n{text}"}],
        )
        await ctx.tools.call("file_write", {"path": out_path, "content": response.content})
        return {"summary": response.content, "path": out_path}
```

Un test unitaire correspondant, sous forme GIVEN / WHEN / THEN :

```python
import pytest

from apollia.testing import (
    mock,
    assert_result_completed,
    assert_llm_called,
    assert_tool_called,
)
from summarizer import Summarizer


@pytest.mark.asyncio
async def test_summarize_writes_file():
    # GIVEN un agent simulé avec une réponse LLM préenregistrée et un outil fichier simulé
    agent, ctx = mock(Summarizer)
    ctx.llm.responses = [{"content": "A short summary."}]
    ctx.tools.responses = {"file_write": {"ok": True}}

    # WHEN le skill s'exécute
    result = await agent.invoke_skill(
        "doc.summarize", text="a long document", out_path="/tmp/out.txt"
    )

    # THEN il se termine, et il a utilisé le LLM puis l'outil fichier
    assert_result_completed(result, contains="summary")
    assert_llm_called(ctx, times=1)
    assert_tool_called(ctx, "file_write", times=1)
```

Points clés du harnais :

- `agent, ctx = mock(YourAgent)` construit l'instance ainsi qu'un `MockContext`.
- Pilotez un skill avec `await agent.invoke_skill(skill_id, **kwargs)`, ou un
  gestionnaire conversationnel avec `await agent.invoke_message(message, history=...)`.
  Les deux retournent le même dict `AIPResult` que celui produit par le runtime.
- Mettez en file les réponses du LLM dans `ctx.llm.responses` (une liste FIFO) ;
  chaque appel à `complete` ou `chat` en dépile la suivante. Simulez les outils
  avec `ctx.tools.responses`, indexé par nom d'outil. `ctx.memory` enregistre et
  rappelle des éléments en mémoire.
- Vérifiez le résultat avec `assert_result_completed(result, contains=...)`,
  `assert_result_failed(result, code=...)` et
  `assert_result_input_required(result)`. Vérifiez ce que l'agent a fait avec
  `assert_llm_called`, `assert_tool_called`, `assert_skill_called`,
  `assert_memory_recorded`, `assert_template_rendered`, `assert_emitted_token`
  et `assert_emitted_thought`.

Comme le harnais réutilise les fonctions de dispatch de production, la
validation des payloads, l'injection de `ctx`, la coercition des valeurs de
retour et la gestion des erreurs typées se comportent exactement comme sous le
daemon. Exécutez ces tests avec `pytest`, comme n'importe quel test Python.

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
  { type = "llm_judge", rubric = "The report is a single clear sentence." },
]
```

- `runs` vaut 3 par défaut. `agent` désigne l'agent ciblé (ou passez `--agent`
  en ligne de commande comme valeur par défaut pour les tâches qui l'omettent).
- Le `type` d'assertion est l'un de `exit_code` (`equals`), `file_exists`
  (`path`), `regex` (`on = "stdout"` ou `"result"`, plus `pattern`), et
  `llm_judge` (`rubric`).
- `llm_judge` note la sortie par rapport à la grille d'évaluation en utilisant
  la route rapide de votre routeur LLM configuré, à température 0. Si aucun
  backend n'est disponible, le juge est ignoré plutôt que de faire échouer
  l'exécution.

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
CLI](/reference/cli), et les formes de service contre lesquelles votre agent
effectue ses assertions figurent dans le [contrat SDK / ctx](/reference/sdk).

## Que choisir

- Contrat et logique (bon payload, bon outil, bonne erreur typée) : tests
  unitaires.
- Comportement de bout en bout, qualité et suivi des régressions entre les
  exécutions : evals.

La plupart des agents ont besoin des deux : des tests unitaires rapides en CI
à chaque changement, et une suite d'evals exécutée contre un daemon avant une
release.
