# Capstone : director et résultat

Les trois workers tournent. Reste à les orchestrer. C'est le rôle du `meeting-director` : un agent conversationnel qui prend la requête du commercial en langue naturelle, et appelle les workers via `apollia.react`.

---

## Le director

### `meeting-director/director.py`

```python
from apollia import agent, on_message, react
from apollia.types import Ctx, Message


SYSTEM_PROMPT = """\
You are a meeting preparation assistant. The user is a sales rep about to
meet a prospect. Your job: build a structured briefing in markdown.

Workflow:

1. Parse the user request: company name + meeting time (e.g., "tomorrow 10:00").
2. Use `web.research.company` to gather general info.
3. Use `web.research.signals` to fetch 3 to 5 recent news signals.
4. Use `crm.lookup.account` to fetch contacts from CRM. If it fails, continue without.
5. For the most relevant contact (DSI, CEO, decision-maker), call
   `crm.lookup.history` to fetch interaction history.
6. Call `prep.build_brief` with the aggregated payload to render the
   final markdown.
7. Optionally call `prep.format_questions` for 3 to 5 open questions.

Be efficient. Aim for 6 to 8 tool calls in total, no more. If a worker
fails, log the failure with `emit_thought` and continue with what you
have. The brief must always be produced.
"""


@agent(
    name="meeting-director",
    version="0.1.0",
    description="Prepare a commercial meeting briefing.",
)
class MeetingDirector:
    @on_message
    async def chat(
        self,
        message: str,
        history: list[Message],
        ctx: Ctx,
    ) -> str:
        ctx.events.emit_thought("Director starting", step=0)

        return await react(
            ctx,
            system=SYSTEM_PROMPT,
            user=message,
            tools=[
                ctx.a2a.skill_as_tool("web.research.company"),
                ctx.a2a.skill_as_tool("web.research.signals"),
                ctx.a2a.skill_as_tool("web.research.linkedin"),
                ctx.a2a.skill_as_tool("crm.lookup.account"),
                ctx.a2a.skill_as_tool("crm.lookup.history"),
                ctx.a2a.skill_as_tool("prep.build_brief"),
                ctx.a2a.skill_as_tool("prep.format_questions"),
            ],
            max_steps=12,
        )
```

Environ 45 lignes. La logique est dans le `system_prompt` et dans la séquence d'outils exposés à `apollia.react`. Le code Python ne fait que câbler.

---

## Lancer le director

Installation :

```bash
apollia agent install ./agents/meeting-director/director.py
apollia agent list
# meeting-director  0.1.0  (conversational)  [0 skills, on_message]
# web-research      0.1.0  worker            [3 skills]
# crm-lookup        0.1.0  worker            [2 skills]
# meeting-prep      0.1.0  worker            [2 skills]
```

Chat :

```bash
apollia chat meeting-director
> Prépare-moi le RDV avec Acme Corp demain à 10h
```

Le director :

1. Émet `emit_thought("Director starting")` pour ouvrir la trace.
2. Entre dans `apollia.react`.
3. Le LLM voit les 7 outils, choisit `web.research.company` en premier.
4. Le runtime invoque le worker `web-research`, qui appelle `web_search` puis `web_read` (sandboxés).
5. La réponse remonte, le LLM décide de l'étape suivante.
6. Six ou sept tours plus tard, le LLM appelle `prep.build_brief` avec tout ce qu'il a collecté.
7. La réponse finale (le markdown) est retournée au chat.

---

## Observabilité

Pendant l'exécution, plusieurs surfaces de trace sont à votre disposition.

### Le chat lui-même

L'app Desktop ou la CLI interactive affiche les étapes intermédiaires si vous êtes en mode développeur (`--debug`). Chaque `emit_thought` apparaît dans la timeline.

### `apollia task trace`

Après le run, vous pouvez consulter la trajectoire complète :

```bash
apollia task list --last 1
# t-abc123  meeting-director  completed  14.2s

apollia task trace t-abc123
# step 1 : LLM thought : "Parsing request : Acme Corp, tomorrow 10:00"
# step 2 : A2A web.research.company input={"company_name": "Acme Corp"}
# step 3 : A2A web.research.signals input={"company_name": "Acme Corp", "max_signals": 5}
# step 4 : A2A crm.lookup.account input={"company_name": "Acme Corp"}
# step 5 : A2A prep.build_brief input={...}
# step 6 : final answer (1834 chars markdown)
```

Chaque étape est tracée avec ses inputs / outputs. C'est précieux pour debugger un brief médiocre : on voit exactement quel worker a renvoyé quoi.

### `ctx.logger` dans chaque worker

Les workers peuvent logguer leur travail :

```python
ctx.logger.info(
    "signals collected",
    extra={"company": company_name, "count": len(signals), "filtered_out": len(results) - len(signals)},
)
```

Les logs sont consultables via `apollia audit --task t-abc123` ou dans la timeline du Desktop.

### `ctx.budget`

Si le director boucle plus que prévu, le step budget se vide. Le runtime coupe automatiquement quand `max_steps` est atteint. Vous pouvez aussi adapter dynamiquement :

```python
if ctx.budget.steps_remaining < 3:
    ctx.events.emit_thought("Budget low, switching to fast path", step=current_step)
    # Fast path : skip optional workers
```

---

## Tester le director

Le director est un peu particulier à tester : il enchaîne des appels A2A et un LLM. `apollia.testing.mock` gère ça via la queue `run_tools_responses`.

```python
import pytest
from apollia.testing import (
    mock,
    assert_result_completed,
    assert_emitted_thought,
)
from director import MeetingDirector


@pytest.mark.asyncio
async def test_director_completes_with_full_pipeline():
    agent, ctx = mock(MeetingDirector)

    # Le runtime LLM (run_tools) retourne la réponse finale du ReAct
    ctx.llm.run_tools_responses = [
        "# RDV Acme Corp, demain 10:00\n\n## L'entreprise\nAcme Corp est..."
    ]

    result = await agent.invoke_message("Prépare-moi le RDV avec Acme Corp demain à 10h")

    assert_result_completed(result, contains="Acme Corp")
    assert_emitted_thought(ctx, contains="Director starting")


@pytest.mark.asyncio
async def test_director_passes_full_tool_list_to_react():
    agent, ctx = mock(MeetingDirector)
    ctx.llm.run_tools_responses = ["short response"]

    await agent.invoke_message("Test")

    # Verify the tools list passed to run_tools
    assert len(ctx.llm.run_tools_calls) == 1
    tools = ctx.llm.run_tools_calls[0]["tools"]
    tool_names = [t["name"] for t in tools]
    assert "web.research.company" in tool_names
    assert "crm.lookup.account" in tool_names
    assert "prep.build_brief" in tool_names
```

L'enchaînement A2A réel (avec les workers qui répondent vraiment) est testé séparément par une eval suite (cf. [chapitre 26](../part-vi-testing/26-eval-suites.md)) qui passe par un LLM réel et des workers réellement déployés.

---

## Aller plus loin

Une fois ce capstone fonctionnel, plusieurs extensions naturelles :

**Trigger calendrier.** Ajoutez un trigger qui, chaque matin à 7h, regarde le calendrier du commercial et déclenche automatiquement le director pour chaque RDV du jour. Vous récupérez les briefs dans votre boîte mail avant d'arriver au bureau (cf. [chapitre 36](../part-viii-runtime-rust/36-triggers.md)).

**Workers supplémentaires.** Un `social-listening-worker` qui scanne Twitter / Mastodon. Un `industry-news-worker` qui filtre par secteur. Un `competitor-mapping-worker` qui détecte les mentions de vos concurrents dans les news. Le director les expose via `skill_as_tool`, le LLM décide quand les appeler.

**Personnalisation par commercial.** Stockez dans `ctx.profile` les préférences (sources préférées, format de brief favori) et lisez-les en début de chaque skill. Un brief court pour le commercial pressé, un brief détaillé pour celui qui prépare longuement.

**Apprentissage continu.** Après chaque RDV, le commercial donne un feedback (« ce brief était utile / inutile »). Le director enregistre dans `ctx.memory` les patterns qui ont marché et adapte le prompt.

---

## Récap

Vous avez vu :

- Un projet multi-agent réel, end-to-end.
- Un director conversationnel orchestrant trois workers via `apollia.react`.
- Trois workers spécialisés avec leurs TypedDict, leurs `Annotated`, leurs `DomainError`.
- Le gating strict (datasources, templates, secrets).
- Les tests isomorphiques pour chaque agent.
- L'observabilité via `ctx.events`, `ctx.logger`, `apollia task trace`.

C'est le pattern Apollia complet. Vous pouvez maintenant écrire vos propres projets de prestation : audit qualité, suivi commercial, veille concurrentielle, automatisation comptable, briefing exécutif. Le runtime fait le reste.

---

## Pour finir le book

- [Annexe B (Glossaire)](../annexes/B-glossary.md) pour la terminologie complète.
- [Annexe C (Principes)](../annexes/C-principles.md) pour les 8 principes architecturaux.
- [Annexe F (ADRs)](../annexes/F-adr-index.md) pour l'index des décisions architecturales.
- [Annexe G (FAQ)](../annexes/G-faq.md) pour les questions récurrentes.
