# `ctx.events`, `ctx.logger`, `ctx.budget`

Trois services regroupés ici parce qu'ils servent l'observabilité de l'agent :

- `ctx.events` émet des événements typés visibles dans l'UI (streaming, raisonnement ReAct, retries).
- `ctx.logger` est un `logging.Logger` stdlib qui alimente le subscriber Rust `tracing`.
- `ctx.budget` expose le `StepBudget` en lecture seule (combien reste-t-il avant que le runtime coupe).

---

## `ctx.events` : événements typés

Quatre méthodes, et **seulement** ces quatre. Les événements de cycle de vie (`task_started`, `step_completed`, etc.) sont émis par le runtime, pas par l'agent.

```python
def emit_token(self, delta: str) -> None: ...
def emit_thought(self, text: str, *, step: int) -> None: ...
def emit_retry(self, *, step: int, reason: str, count: int) -> None: ...
def emit_action_parse_error(
    self,
    *,
    step: int,
    raw: str,
    fatal: bool = False,
) -> None: ...
```

### `emit_token(delta)` : streaming UI

Pousse un fragment de tokens vers l'app Desktop ou la CLI interactive. Le cas typique est `@on_message` qui boucle sur `ctx.llm.stream` :

```python
stream = await ctx.llm.stream(messages=[...])
async for chunk in stream:
    ctx.events.emit_token(chunk)
```

L'UI reçoit chaque chunk au fil de l'eau.

### `emit_thought(text, step)` : raisonnement ReAct

À utiliser quand l'agent veut tracer un raisonnement intermédiaire. `apollia.react` émet déjà un `emit_thought` à l'entrée de boucle. Vous pouvez en émettre d'autres pour documenter une décision :

```python
ctx.events.emit_thought("J'identifie 3 mentions du concurrent X dans le digest.", step=2)
```

Visible dans les dashboards d'observabilité, utile pour comprendre après coup pourquoi l'agent a pris telle branche.

### `emit_retry(step, reason, count)`

Trace une tentative refaite après échec :

```python
try:
    result = await ctx.tools.call("web_read", input={"url": url})
except Exception as exc:
    ctx.events.emit_retry(step=current_step, reason=str(exc), count=1)
    result = await ctx.tools.call("web_read", input={"url": url})  # second essai
```

### `emit_action_parse_error(step, raw, fatal)`

À utiliser quand le parsing d'une réponse LLM échoue (JSON mal formé, action inconnue). `apollia.react` l'émet automatiquement quand le LLM produit une réponse mal structurée.

### Pas d'événement custom

La liste est **fermée**. Si vous voulez tracer une information métier supplémentaire, utilisez `ctx.logger.info(...)` avec un `extra={...}`. Les `extra` arrivent dans `tracing` comme des champs structurés, exploitables côté observabilité.

---

## `ctx.logger` : logging stdlib

Un `logging.Logger` du module stdlib, configuré pour piper les records dans le subscriber `tracing` Rust.

```python
ctx.logger.info("starting cycle", extra={"topics_count": 12, "sources_count": 38})
ctx.logger.warning("rate limit approached", extra={"backend": "anthropic", "rate": 0.9})
ctx.logger.error("scrape failed", extra={"url": url, "error_code": "TIMEOUT"})
```

Trois règles :

1. **Pas de format string.** Préférez `info("event name", extra={...})`. Les champs structurés sont récupérables dans les dashboards.
2. **Niveaux standards** : `debug`, `info`, `warning`, `error`. Pas de niveaux custom.
3. **`extra` est sérialisé.** Restez en types primitifs (str, int, float, bool, list, dict). Pas d'objets Python opaques.

Le nom du logger est calculé automatiquement (`apollia.agent.<name>`). Vous n'avez pas besoin de `getLogger(...)`.

---

## `ctx.budget` : vue lecture seule du StepBudget

Le runtime applique des limites globales à chaque tâche : nombre de steps, nombre d'appels d'outils, temps wall-clock. Ces limites sont des garde-fous non négociables (principe #7). L'agent ne peut pas les contourner. Il peut en revanche les **consulter** :

```python
@property
def steps_remaining(self) -> int: ...

@property
def tool_calls_remaining(self) -> int: ...

@property
def elapsed_seconds(self) -> float: ...

@property
def wall_clock_remaining(self) -> float | None: ...
```

Usage typique : décider de la profondeur d'une analyse en fonction du budget restant.

```python
if ctx.budget.steps_remaining < 5:
    ctx.logger.info("budget low, switching to fast path")
    return await self._fast_summary(items, ctx)
return await self._deep_analysis(items, ctx)
```

`wall_clock_remaining` est `None` si la tâche n'a pas de timeout wall-clock configuré.

### Pas d'écriture

Le Protocol n'expose aucune méthode pour ajuster le budget. Si vous voulez plus de budget pour une tâche donnée, c'est dans `@agent(step_budget={"max_steps": 60})` au manifest level (cf. [chapitre 6](../part-ii-the-decorators/06-agent-decorator.md)), pas via `ctx.budget`.

---

## Quand utiliser quoi

| Besoin | Service |
|---|---|
| Streamer la réponse LLM token par token | `ctx.events.emit_token` |
| Tracer un raisonnement ou une décision pour les dashboards | `ctx.events.emit_thought` |
| Tracer une donnée métier structurée pour les logs | `ctx.logger.info(..., extra=...)` |
| Signaler une erreur applicative | `ctx.logger.error(..., extra=...)` puis `raise DomainError(...)` |
| Adapter la profondeur selon le budget restant | `ctx.budget.steps_remaining` |

---

## Anti-patterns

**Ne pas** émettre des centaines d'événements `emit_token` ou `emit_thought` dans une boucle serrée. L'EventBus est performant mais pas illimité. Aggrégez quand c'est possible (un `emit_thought` par étape ReAct, pas par sous-action).

**Ne pas** utiliser `print(...)` ou `logging.getLogger(__name__)` à la place de `ctx.logger`. Ces calls ne sont pas wirés au subscriber Rust et n'apparaîtront pas dans les dashboards.

**Ne pas** stocker `ctx.budget` dans une variable et le réutiliser plus tard. La vue évolue à chaque appel d'outil et à chaque step LLM. Lisez-le au moment où vous en avez besoin.

**Ne pas** essayer d'émettre un événement custom (`ctx.events.emit_my_thing`). Le Protocol ne le permet pas. Passez par `ctx.logger.info(..., extra={...})`.

---

## ADRs

- `ADR-012` : Observabilité complète et timeline
- `ADR-020` : EventBus + Tauri events
- `ADR-024` : SDK events types publics
- `ADR-024` : SDK logger structure

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
