# ADR-112 - Suppression `LlmProxy.stream()` legacy, renommage `stream_complete` → `stream`

**Date :** 2026-05-19
**Statut :** Accepté
**Sprint :** Phase release v0.1.0 (refonte SDK)

---

## Contexte

Le service `ctx.llm` du SDK expose **deux APIs de streaming concurrentes**,
ce qui crée une confusion structurelle pour l'auteur d'agent.

**État observé au 2026-05-19** (`sdk/apollia/stubs/llm.py`) :

- Ligne 90 : `def stream(self, messages, **kwargs)` - héritage du
  sprint 14, retourne `list[str]` (collecte buffered). Docstring
  ligne 101 indique : "Prefer `stream_complete()` for real
  token-by-token streaming."
- Ligne 106 : `def stream_complete(self, messages, **kwargs)` - API
  moderne, retourne un async iterator de chunks (token-by-token).
- Aucune méthode `stream()` n'est plus utilisée par les agents
  bundled. Tous utilisent `stream_complete()` (4 occurrences dans
  `agents/`).
- `BaseReActAgent` (`sdk/apollia/agents/react.py:495`) check
  `hasattr(llm, "stream_complete")` pour autoriser le streaming -
  preuve que `stream()` est mort depuis longtemps.

Conséquences :

- Nouveau venu qui tape `ctx.llm.<tab>` voit `stream` et
  `stream_complete` - choisit `stream` par habitude (plus court),
  obtient un comportement buffered surprenant, debug pour rien.
- Doc inconsistante : le nom le plus court désigne la version moins
  bonne.
- Maintenance double (PyO3 binding + tests + stubs) pour une API
  zombie.

Deuxième problème adjacent - **auto-rewrite des actions shorthand** :

Dans `sdk/apollia/agents/react.py` (boucle ReAct), si le LLM produit un
JSON `{"action": "web_search", "query": "..."}` (shorthand où le nom de
tool est mis directement dans `action` au lieu de
`{"action": "tool_call", "tool": "web_search", "args": {...}}`), le SDK
**ré-écrit silencieusement** ce shorthand en forme canonique. Cette
indulgence semblait pratique au début, mais en pratique :

- Elle masque les bugs de prompt - l'auteur croit que son prompt est
  bon, alors qu'il devrait être plus strict avec le LLM.
- Elle complique le debug (la version réécrite n'est pas celle que
  le LLM a effectivement produite).
- Elle bypass la validation de schéma (le shorthand n'a pas d'`args`
  → rewrite injecte `{}` → tool appelé sans args → erreur métier
  cryptique au lieu d'une erreur claire).
- Aucun autre framework (LangChain, CrewAI, AutoGen) ne fait ce
  rewrite - c'est une particularité Apollia non-documentée.

## Décision

**Nous adoptons trois changements ciblés sur `ctx.llm` et la boucle
ReAct :**

1. **Suppression** de `LlmProxy.stream()` legacy (la version buffered).
2. **Renommage** de `stream_complete()` → `stream()` (la version
   moderne devient la méthode au nom canonique).
3. **Suppression** de l'auto-rewrite des actions shorthand dans la
   boucle ReAct - un shorthand devient une `ActionParseError` claire
   au lieu d'un magic-fix.

Surface cible :

```python
class LlmService(Protocol):
    async def complete(
        self,
        messages: list[LlmMessage],
        *,
        model: str | None = None,
        temperature: float | None = None,
        max_tokens: int | None = None,
        tools: list[ToolDescriptor] | None = None,
    ) -> str: ...

    async def stream(  # ← renommé depuis stream_complete
        self,
        messages: list[LlmMessage],
        *,
        model: str | None = None,
        temperature: float | None = None,
        max_tokens: int | None = None,
        tools: list[ToolDescriptor] | None = None,
    ) -> AsyncIterator[str]: ...

    async def embed(
        self,
        text: str | list[str],
        *,
        model: str | None = None,
    ) -> list[float] | list[list[float]]: ...
```

Pour le ReAct shorthand :

```python
# AVANT (sdk/apollia/agents/react.py)
def _parse_action(raw: str) -> dict:
    parsed = json.loads(raw)
    if "tool" not in parsed and "action" in parsed:
        # Magic fix shorthand
        parsed = {"action": "tool_call",
                  "tool": parsed["action"],
                  "args": parsed.get("args", {})}
    return parsed

# APRÈS
def _parse_action(raw: str) -> dict:
    parsed = json.loads(raw)
    if parsed.get("action") not in ("tool_call", "final_answer", "delegate"):
        raise ActionParseError(
            f"Unknown action '{parsed.get('action')}'. "
            f"Expected one of: tool_call, final_answer, delegate. "
            f"Raw LLM output: {raw[:200]}"
        )
    return parsed
```

L'`ActionParseError` est émise au runtime via `ctx.events.
emit_action_parse_error(step, raw, fatal=True)` (ADR-105) - visible
en UI builder mode, traçable, debuggable.

## Alternatives considérées

### Pour stream

**A. Garder les deux comme aliases** (rejetée)
**Pour :** zéro breaking change.
**Contre :** maintient la confusion. La docstring d'avertissement ne
suffit pas.

**B. Garder `stream_complete` comme nom canonique, supprimer `stream`**
(rejetée)
**Pour :** zéro renaming, juste suppression.
**Contre :** `stream_complete` est verbeux et inutilement précis (qu'est-
ce qui était "incomplete" ?). Cohérence ADR-101 (services nestés)
demande un nom court.

### Pour shorthand rewrite

**A. Garder le rewrite mais émettre un warning** (rejetée)
**Pour :** rétrocompat.
**Contre :** warning ignoré par défaut en prod. Bug reste latent.

**B. Configuration `strict_action_parsing: bool`** (rejetée)
**Pour :** flexibilité.
**Contre :** option = friction supplémentaire. Default doit être strict
en v1.

### Option retenue - Suppression franche + renommage

**Pour :** API minimaliste et claire (`ctx.llm.complete` + `.stream` +
`.embed` - 3 méthodes nommées idéalement), debug plus simple (un
shorthand est une erreur visible), suppression de code mort (~20 LOC
boucle ReAct + ~30 LOC stream legacy).
**Compromis acceptés :** breaking - tout agent qui appelait
`stream_complete()` doit renommer en `stream()`. Tout LLM prompt mal
calibré qui produisait du shorthand doit être renforcé. Effort
migration ~30 min sur les agents bundled (mécanique).

## Conséquences

**Positives :**

- `ctx.llm` exposé propre : 3 méthodes (`complete`, `stream`, `embed`).
- Suppression ~50 LOC totales (stream legacy + shorthand rewrite +
  tests associés).
- Bugs de prompt mis à nu - l'auteur voit que son prompt produit du
  shorthand et le corrige (au lieu que le SDK masque silencieusement).
- Cohérence avec frameworks modernes (LangChain `stream`, OpenAI SDK
  `stream=True`, etc.) - courbe d'apprentissage proche zéro.
- L'`ActionParseError` traverse `ctx.events` (ADR-105) → visible UI,
  debuggable.

**Négatives / Compromis :**

- Migration agents : `stream_complete` → `stream` (~4 occurrences).
  Mécanique.
- Migration prompts ReAct : si un prompt produit régulièrement du
  shorthand, l'agent va échouer plus souvent en v1.0 qu'en v0.4.
  L'auteur doit renforcer son prompt (ex. ajouter un exemple positif
  + négatif). Effort estimé < 1h par agent bundled.
- Pas de bouton "souple" pour des LLM faibles qui produisent du
  shorthand : ils échoueront. Documenter - c'est un bug à fixer côté
  prompt, pas côté SDK.

**À surveiller :**

- Taux d'`ActionParseError` post-release : si > 10 % d'invocations
  d'un agent bundled lèvent l'erreur, son prompt est probablement à
  retoucher (signalable).
- Émergence de besoins streaming custom (ex. yield partiel après
  parsing JSON delta) - v1.1 si demande.
- Modèles locaux faibles (Llama 3 8B) qui produisent souvent du
  shorthand : prévoir des exemples renforcés dans les prompts ReAct
  par défaut.

## Principes architecturaux impactés

- **Principe #3 - Contrat minimal** : API LLM réduite à 3 méthodes
  bien nommées.
- **Principe #4 - Fail fast** : shorthand devient une erreur visible,
  pas un magic fix masqué.
- **Principe #5 - Un acteur, une responsabilité** : le LLM service
  ne fait plus de "rewrite" - c'est strictement un proxy LLM, le
  parsing d'action revient au ReAct loop qui le fait explicitement.

## Liens

- ADR-101 - `ctx` Protocol (`ctx.llm` exposé)
- ADR-105 - `ctx.events` (action parse error émis ici)
- ADR-098 - Decorator-first (ReAct devient utility, sans ce magic fix)
- ADR-078 - Meta-LLM orchestrator (consommateur de stream/complete)
