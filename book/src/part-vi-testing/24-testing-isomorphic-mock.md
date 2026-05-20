# `apollia.testing.mock`

Tester un agent Apollia ne demande pas de démarrer le runtime Rust, ni d'avoir un LLM connecté, ni de toucher au filesystem. La fonction `apollia.testing.mock(MyAgent)` crée une instance de votre agent reliée à un `MockContext` qui implémente les 14 services du Protocol `Ctx`. Vous pré-configurez les services, vous appelez votre skill ou `@on_message`, vous assertez. Tout vit en mémoire.

C'est ce qu'on appelle un test **isomorphique** : la surface vue par l'agent en test est identique à celle vue en production. Le code de l'agent est inchangé.

---

## Pattern minimal

```python
import pytest
from apollia.testing import mock, assert_result_completed
from pdf_worker import PdfWorker


@pytest.mark.asyncio
async def test_pdf_read_text_returns_text():
    agent, ctx = mock(PdfWorker)

    result = await agent.invoke_skill("pdf.read_text", path="/tmp/some.pdf")

    assert_result_completed(result)
    assert "text" in result["data"]
```

Trois étapes :

1. `mock(PdfWorker)` retourne le tuple `(agent_instance, ctx)`. L'instance est dotée de deux helpers : `invoke_skill(skill_id, **payload)` et `invoke_message(message, history=None)`.
2. Vous appelez la skill avec `invoke_skill`. Sous le capot, le boundary du SDK est utilisé exactement comme en production. Validation de signature, exceptions trappées, AIPResult produit.
3. Vous assertez sur l'AIPResult retourné et / ou sur les interactions enregistrées par `ctx`.

---

## Pré-configurer les services

`MockContext` instancie 14 mocks (un par service). Chacun expose des attributs publics que vous pouvez ajuster avant l'appel.

### `ctx.llm`

```python
agent, ctx = mock(Coach)
ctx.llm.responses = [{"content": "Bonjour, comment puis-je vous aider ?"}]

result = await agent.invoke_message("Salut")

# La 1ère réponse a été consommée. Si la méthode appelle complete() une 2e fois,
# il faut une 2e response dans la liste.
assert_result_completed(result)
assert ctx.llm.call_count == 1
```

`responses` est une queue FIFO. Chaque appel à `complete()` ou `chat()` consomme la prochaine entrée.

Pour `run_tools` (utilisé par `apollia.react`), il y a une queue séparée :

```python
ctx.llm.run_tools_responses = ["Voici ma réponse finale."]
```

### `ctx.tools`

```python
ctx.tools.responses = {
    "file_read": {"content": "Hello world", "size_bytes": 11},
    "bash_executor": {"stdout": "ok\n", "exit_code": 0},
}

result = await agent.invoke_skill("audit.report", root="/tmp")

# Vérifier les appels enregistrés
assert ctx.tools.calls == [
    ("bash_executor", {"cmd": "find /tmp -type f"}),
    ("file_read", {"path": "/tmp/file1.txt"}),
]
```

### `ctx.a2a`

```python
ctx.a2a.responses = {
    "pdf.read_text": {"text": "PDF content here", "page_count": 5},
}

result = await agent.invoke_skill("report.generate", path="/tmp/report.pdf")

assert ctx.a2a.invoke_calls == [
    ("pdf.read_text", {"path": "/tmp/report.pdf"}),
]
```

### `ctx.datasources` et `ctx.templates`

```python
ctx.datasources.values = {
    "competitors": {"names": ["AcmeCorp", "BetaInc"]},
    "topics": {"entries": ["pricing", "features"]},
}
ctx.templates.templates = {
    "digest": "Hello {{ name }}, your digest is ready.",
}

result = await agent.invoke_skill("veille.run_cycle")
```

### `ctx.secrets`

```python
ctx.secrets.values = {"openweather_api_key": "sk-test"}
# Maintenant ctx.secrets.get("openweather_api_key") renvoie "sk-test"
# ctx.secrets.get("unknown") renvoie None
```

### `ctx.memory`

`MockMemory` enregistre les `record`, `remember`, `recall` dans `ctx.memory.episodes`, `ctx.memory.store`, et `ctx.memory.operations`.

### `ctx.events`, `ctx.budget`, `ctx.profile`, `ctx.workspace`, `ctx.stt`, `ctx.notify`

Tous instanciés avec des valeurs par défaut. Vous les ajustez si votre test en a besoin :

```python
ctx.profile.values = {"user.name": "Alice", "user.preferred_language": "fr"}
ctx.workspace.rules_text = "# Project rules\n- Always be polite."
ctx.budget.steps_remaining_value = 3
```

---

## Tester un `@on_message`

```python
@pytest.mark.asyncio
async def test_coach_responds_in_french():
    agent, ctx = mock(Coach)
    ctx.llm.responses = [{"content": "Bonjour ! Je peux vous expliquer le pattern Director."}]

    result = await agent.invoke_message("Comment fonctionne le pattern Director ?")

    assert_result_completed(result, contains="Director")
    assert ctx.memory.episodes  # Le coach a enregistré la trace.
```

`invoke_message(message, history=None)` dispatche vers le handler `@on_message`. La string retournée par le handler est emballée en `AIPResult.completed`.

---

## Tester un director qui utilise `apollia.react`

```python
@pytest.mark.asyncio
async def test_director_uses_pdf_worker():
    agent, ctx = mock(ResearchDirector)
    ctx.llm.run_tools_responses = ["Voici l'analyse du PDF : ..."]

    result = await agent.invoke_message("Analyse /tmp/report.pdf")

    assert_result_completed(result, contains="analyse")
    # Vérifier que react a bien été invoqué
    assert len(ctx.llm.run_tools_calls) == 1
    call = ctx.llm.run_tools_calls[0]
    assert call["max_iterations"] == 10
```

Quand l'agent appelle `apollia.react(ctx, ...)`, sous le capot c'est `ctx.llm.run_tools(...)` qui est invoqué. Pré-configurer `ctx.llm.run_tools_responses` permet de simuler la réponse finale du ReAct.

---

## Tester un `@orchestrated`

Le moteur ORIA tourne côté Rust, donc en mock isomorphe il n'est pas activé. Vous testez surtout le hook `on_plan_complete` directement :

```python
@pytest.mark.asyncio
async def test_briefing_on_plan_complete_concatenates_texts():
    agent, ctx = mock(Briefing)
    step_results = {
        "step_1": {"text": "Context section."},
        "step_2": {"text": "Key facts section."},
        "step_3": {"text": "Open questions section."},
    }

    final = agent.on_plan_complete(step_results)

    assert "Context" in final
    assert "Key facts" in final
    assert "Open questions" in final
```

Pour un test end-to-end d'un orchestré (avec moteur ORIA), il faudra un test d'intégration qui démarre le runtime. Hors scope de `apollia.testing.mock`.

---

## Anti-patterns

**Ne pas** essayer de monkey-patcher `ctx` après l'appel à `mock`. Le tuple retourné est déjà câblé. Configurez les services en lecture / écriture des attributs publics (`.responses`, `.values`, `.templates`).

**Ne pas** importer `MockLlmProxy` séparément pour le passer manuellement à l'agent. Le `mock()` factory câble tout, c'est le pattern recommandé.

**Ne pas** appeler `agent.read_text(...)` directement (la méthode). Le boundary est court-circuité, la validation de signature et la trappe des exceptions ne s'appliquent pas. Utilisez `agent.invoke_skill("pdf.read_text", ...)`.

---

## ADRs

- `ADR-098` : Decorator-first (testabilité = bénéfice direct)
- `ADR-101` : Ctx Protocol (mockable trivialement)

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
