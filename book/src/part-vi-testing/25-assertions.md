# Assertions

Le module `apollia.testing` expose dix helpers d'assertion qui lèvent `AssertionError` avec un message clair. Ils inspectent les `AIPResult` dicts retournés par `invoke_skill` / `invoke_message`, et les services du `MockContext` pour vérifier les interactions.

Tous sont synchrones (pas d'`await`). Compatibles avec `pytest` directement.

---

## Vue d'ensemble

| Helper | Vérifie |
|---|---|
| `assert_result_completed(result, contains=...)` | `result["status"] == "completed"` (+ texte) |
| `assert_result_failed(result, code=...)` | `result["status"] == "failed"` (+ code d'erreur) |
| `assert_result_input_required(result)` | `result["status"] == "input_required"` |
| `assert_llm_called(ctx, times=...)` | `ctx.llm` a été appelé |
| `assert_tool_called(ctx, name, times=...)` | `ctx.tools.call(name, ...)` |
| `assert_skill_called(ctx, skill_id, times=...)` | `ctx.a2a.invoke(skill_id, ...)` |
| `assert_emitted_token(ctx, contains=...)` | `ctx.events.emit_token` |
| `assert_emitted_thought(ctx, contains=...)` | `ctx.events.emit_thought` |
| `assert_memory_recorded(ctx, key=...)` | `ctx.memory.record` ou `ctx.memory.remember` |
| `assert_template_rendered(ctx, name)` | `ctx.templates.render(name, ...)` |

Import :

```python
from apollia.testing import (
    mock,
    assert_result_completed,
    assert_result_failed,
    assert_result_input_required,
    assert_llm_called,
    assert_tool_called,
    assert_skill_called,
    assert_emitted_token,
    assert_emitted_thought,
    assert_memory_recorded,
    assert_template_rendered,
)
```

---

## Assertions sur `AIPResult`

```python
@pytest.mark.asyncio
async def test_pdf_read_handles_missing_file():
    agent, ctx = mock(PdfWorker)

    result = await agent.invoke_skill("pdf.read_text", path="/tmp/nope.pdf")

    assert_result_failed(result, code="FILE_NOT_FOUND")
```

`assert_result_failed(result)` vérifie que le status est `failed`. Le `code=` optionnel ajoute une vérification stricte sur `result["error"]["code"]`.

`assert_result_completed(result, contains=...)` accepte un substring que le texte agrégé doit contenir :

```python
result = await agent.invoke_message("Bonjour")
assert_result_completed(result, contains="Bonjour")
```

`assert_result_input_required(result)` vérifie une suspension HITL :

```python
result = await agent.invoke_skill("invoice.route", vendor="Acme", amount=1240.0)
assert_result_input_required(result)
```

---

## Assertions sur les interactions

### LLM

```python
@pytest.mark.asyncio
async def test_coach_calls_llm_once():
    agent, ctx = mock(Coach)
    ctx.llm.responses = [{"content": "..."}]

    await agent.invoke_message("Salut")

    assert_llm_called(ctx, times=1)
```

`times` est optionnel : sans, on vérifie juste « au moins un appel ».

### Outils natifs

```python
@pytest.mark.asyncio
async def test_audit_calls_bash_and_file_read():
    agent, ctx = mock(AuditAgent)
    ctx.tools.responses = {
        "bash_executor": {"stdout": "/tmp/a\n/tmp/b\n", "exit_code": 0},
        "file_read": {"content": "...", "size_bytes": 100},
    }

    await agent.invoke_skill("audit.report", root="/tmp")

    assert_tool_called(ctx, "bash_executor", times=1)
    assert_tool_called(ctx, "file_read", times=2)
```

### Skills A2A

```python
@pytest.mark.asyncio
async def test_director_calls_pdf_worker():
    agent, ctx = mock(ResearchDirector)
    ctx.a2a.responses = {"pdf.read_text": {"text": "...", "page_count": 5}}
    ctx.llm.run_tools_responses = ["Synthèse : ..."]

    await agent.invoke_message("Lis /tmp/report.pdf")

    assert_skill_called(ctx, "pdf.read_text")
```

### Events

```python
@pytest.mark.asyncio
async def test_coach_streams_tokens():
    agent, ctx = mock(Coach)
    ctx.llm.responses = [{"content": "Hello", "stream": ["Hel", "lo"]}]

    await agent.invoke_message("Hi")

    assert_emitted_token(ctx, contains="Hel")
```

> Note : le `MockLlmProxy.stream` simule le streaming en consommant la même `responses` queue. La mécanique exacte dépend du mock courant.

### Mémoire

```python
@pytest.mark.asyncio
async def test_onboarding_stores_user_name():
    agent, ctx = mock(Onboarding)
    ctx.llm.responses = [{"content": "Enchanté Alice !"}]

    await agent.invoke_message("Je m'appelle Alice")

    assert_memory_recorded(ctx, key="user.name")
```

Sans `key`, le helper vérifie qu'**au moins** une entrée épisodique a été enregistrée.

### Templates

```python
@pytest.mark.asyncio
async def test_digest_renders_template():
    agent, ctx = mock(VeilleIA)
    ctx.datasources.values = {"topics": [...], "sources": [...]}
    ctx.templates.templates = {"weekly-digest": "# Digest"}
    ctx.llm.responses = [{"content": "ok"}]

    await agent.invoke_skill("veille.format_digest", items=[], week=42)

    assert_template_rendered(ctx, "weekly-digest")
```

---

## Combiner plusieurs assertions

Un test d'agent typique vérifie plusieurs choses : que la réponse est `completed`, que le bon outil a été appelé, que la mémoire a été mise à jour, que les bons événements ont été émis.

```python
@pytest.mark.asyncio
async def test_audit_end_to_end():
    agent, ctx = mock(AuditAgent)
    ctx.tools.responses = {
        "bash_executor": {"stdout": "/tmp/a.log\n", "exit_code": 0},
        "file_read": {"content": "log content", "size_bytes": 11},
    }
    ctx.llm.responses = [{"content": "Audit complete: 1 file analyzed."}]

    result = await agent.invoke_skill("audit.report", root="/tmp")

    assert_result_completed(result, contains="Audit complete")
    assert_tool_called(ctx, "bash_executor", times=1)
    assert_tool_called(ctx, "file_read", times=1)
    assert_llm_called(ctx, times=1)
    assert_memory_recorded(ctx)
```

Chaque assertion lève `AssertionError` avec un message ciblé en cas d'échec, ce qui permet de localiser rapidement le problème dans la trace pytest.

---

## Quand écrire ses propres assertions

Les helpers couvrent les patterns courants. Pour les cas plus spécifiques, accédez directement aux attributs publics du `MockContext` :

```python
# Vérifier les arguments exacts d'un appel d'outil
assert ctx.tools.calls[0] == ("bash_executor", {"cmd": "find /tmp"})

# Vérifier le contenu d'une entrée mémoire
assert ctx.memory.episodes[0]["importance"] == 0.7

# Vérifier qu'une notification a été émise avec la bonne sévérité
assert ctx.notify.published[-1]["severity"] == "warning"
```

C'est moins lisible que les helpers, mais ça reste lisible et précis.

---

## Anti-patterns

**Ne pas** assertir sur des structures internes non documentées (`ctx.llm._private_state`). Restez sur les attributs publics du mock (`.responses`, `.calls`, `.episodes`, etc.).

**Ne pas** chaîner trop d'assertions dans un seul test. Un test = un comportement. Si vous avez 10 assertions, c'est probablement 3 ou 4 tests qui se cachent.

**Ne pas** dupliquer la configuration du mock dans chaque test. Une fixture `@pytest.fixture` qui produit `(agent, ctx)` pré-configuré pour le cas nominal réduit la duplication.

---

## ADRs

- `ADR-098` : Decorator-first (testabilité native)
- `ADR-101` : Ctx Protocol (interactions mockables)

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
