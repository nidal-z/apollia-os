# ADR-105 - Events publics typés (`ctx.events`)

**Date :** 2026-05-19
**Statut :** Accepté
**Sprint :** Phase release v0.1.0 (refonte SDK)

---

## Contexte

L'observabilité des agents (UI desktop temps réel, logs structurés)
repose sur des événements émis par le code Python vers le runtime Rust,
puis broadcastés sur `EventBus`. Aujourd'hui ces events sont émis via
des méthodes nommées implicitement sur `RuntimeContext` :
`emit_token`, `emit_thought`, `emit_retry`, `emit_action_parse_error`.

**État observé au 2026-05-19** (audit `sdk/apollia/agents/react.py`,
`sdk/apollia/stubs/context.py`) :

- `react.py:187` définit un helper `_emit_safe(ctx, method, *args)` qui
  fait `getattr(ctx, method, lambda *a: None)(*args)` - i.e. **le SDK
  lui-même ne fait pas confiance** à la présence des méthodes
  d'événements sur `ctx`. C'est défensif contre les contexts de test
  qui n'implémentent pas tout.
- Aucune méthode `emit_*` n'est typée formellement dans
  `sdk/apollia/stubs/context.py` (recherchant `emit_` dans `stubs/` :
  4 mentions vagues dans des docstrings, aucune signature explicite).
- L'auteur d'agent qui veut émettre une "thought" custom doit deviner
  la signature (`emit_thought(text, step_num)` ? `emit_thought(text)` ?).
  Cherche dans `crates/apollia-aip/src/context.rs` pour la trouver -
  ce qui contredit l'idée que le SDK Python est self-contained.
- Côté runtime : 4 méthodes implémentées côté Rust (`emit_token`,
  `emit_thought`, `emit_retry`, `emit_action_parse_error`). Pas d'event
  `emit_skill_started` / `emit_skill_completed` / `emit_warning` /
  `emit_progress`, pourtant le UI builder mode (cf. mémoire
  `feedback_operator_builder_modes`) en aurait besoin pour une
  observabilité exhaustive ("plus transparent que Claude.ai").
- Pas de contrat sur le no-op gracieux : si le ctx est un mock test,
  les events doivent silencieusement disparaître sans crasher.

## Décision

**Nous adoptons un service `ctx.events` typé via `Protocol`, exposant
explicitement les événements publics que les agents peuvent émettre.
Tous les events sont no-op gracieux si le runtime n'est pas branché
(testing, mock). Le helper `_emit_safe` disparaît du SDK - remplacé par
le contrat formel.**

Surface publique :

```python
class EventsService(Protocol):
    def emit_token(self, delta: str) -> None:
        """Stream LLM token (typiquement appelé par `ctx.llm.stream` ou
        `ctx.react`). Émet vers l'UI desktop en temps réel."""

    def emit_thought(self, text: str, step: int | None = None) -> None:
        """Pensée intermédiaire (ReAct Reasoner step). Affichée dans
        l'UI builder mode."""

    def emit_action(self, name: str, args: dict, step: int | None = None) -> None:
        """Action choisie par le ReAct (tool/skill name + args)."""

    def emit_observation(self, result: object, step: int | None = None) -> None:
        """Résultat de l'action (input du Reasoner suivant)."""

    def emit_retry(self, step: int, reason: str, count: int) -> None:
        """Tentative de retry après erreur parse/transient."""

    def emit_action_parse_error(
        self,
        step: int,
        raw: str,
        fatal: bool = False,
    ) -> None:
        """LLM a produit un JSON action invalide."""

    def emit_progress(self, message: str, ratio: float | None = None) -> None:
        """Progression métier (ex. 'page 12/300 extraite'). Pour les
        skills long-running. `ratio` ∈ [0.0, 1.0] si connu."""

    def emit_warning(self, code: str, message: str, details: dict | None = None) -> None:
        """Warning non-bloquant (ex. rate-limit approchant). Affiché
        dans l'UI builder mode."""
```

Règles :

1. **Tous synchrones non-async** - l'émission événement est un
   `send` non-bloquant côté Rust (mpsc::Sender vers EventBus). Pas
   d'await côté Python.
2. **No-op gracieux** - si `ctx.events` est un `NullEventsService`
   (testing), toutes les méthodes retournent sans rien faire. Plus
   besoin de `getattr(..., lambda: None)` côté agent.
3. **Pas de méthode custom** - l'auteur n'invente pas ses propres
   events. Pour de la donnée métier qui doit voyager, utiliser
   `ctx.logger.info(...)` (ADR-106) avec extra fields structurés.
4. **Contrat versionné** - ajouter un event = mineur SemVer SDK ;
   renommer/supprimer = majeur. La liste reste maîtrisée (cible :
   ~10 events max).
5. **Le runtime DOIT exposer les 8 méthodes** - vérifié au load par le
   check Protocol (ADR-101). Une divergence = fail à l'import.

## Alternatives considérées

### Option A - Conserver le pattern `getattr` défensif (rejetée)

**Pour :** rétrocompat.
**Contre :** maintient la confusion runtime/test. Aucun typage IDE.
Auteur ne sait jamais quel event est émis vs ignoré.

### Option B - Un seul `ctx.events.emit(kind, **data)` générique (rejetée)

**Pour :** ultra-simple, ajouter un event ne touche pas le Protocol.
**Contre :** zéro autocomplete, zéro typage des payloads, zéro
discoverabilité. L'UI desktop doit deviner les `kind` en wild.

### Option C - Pub/sub (l'agent publie, le runtime souscrit) avec topic strings (rejetée)

**Pour :** flexible.
**Contre :** abstraction inutile pour notre usage (l'UI desktop est le
seul consommateur réel). Surface API plus large pour rien.

### Option retenue - Protocol typé, 8 events nommés, no-op gracieux

**Pour :** chaque event est documenté et typé, autocomplete IDE clair,
le mock testing implémente `NullEventsService` une fois pour toutes,
ajout d'un event = patch local (SDK + runtime + UI).
**Compromis acceptés :** liste fermée à 8 events en v1.0. Si un agent
veut émettre du custom, il passe par `ctx.logger.info(...)`. Acceptable.

## Conséquences

**Positives :**

- Élimination de `_emit_safe` et de tous les `getattr(ctx, "emit_X",
  lambda *a: None)` - code agent plus propre.
- L'UI desktop (builder mode) gagne 4 nouveaux events
  (`emit_action`, `emit_observation`, `emit_progress`, `emit_warning`)
  qui matérialisent la promesse "plus transparent que Claude.ai" (cf.
  mémoire `project_sprint42_frontend`).
- Mode operator (`feedback_operator_builder_modes`) reste minimaliste -
  il n'affiche que `emit_progress` + `emit_warning` (les events
  pertinents pour un humain non-builder).
- Mock testing trivial - `NullEventsService()` injecté dans les tests.
- Contract clair runtime ↔ SDK : ajout d'event = 3 fichiers patchés en
  parallèle (SDK Protocol + Rust impl + UI consumer).

**Négatives / Compromis :**

- L'auteur ne peut pas définir ses propres events typés. Pour 95 % des
  cas, `ctx.logger.info(...)` avec `extra={...}` couvre. Documenter le
  pattern.
- Le runtime Rust doit implémenter 4 nouveaux events (`emit_action`,
  `emit_observation`, `emit_progress`, `emit_warning`) - effort estimé
  0.5j sur LOT 8.
- L'UI desktop builder mode doit consommer ces nouveaux events
  (renderer, store SSE) - ~0.5j.

**À surveiller :**

- Volume d'events sur agents long-running (ex. veille-ia qui traite
  500 sources) : si l'EventBus sature, ajouter du throttling côté Rust
  per-event-type.
- Émergence d'events métier récurrents (ex. `emit_skill_started/ended`)
  - candidate à ajout en v1.1 si demande forte.
- Sémantique de `emit_progress(ratio)` quand l'agent ne connaît pas le
  total : documenter `ratio=None` comme "indéterminé".

## Principes architecturaux impactés

- **Principe #3 - Contrat minimal** : events sont opt-in (l'agent ne les
  émet que s'il le veut). Aucune obligation.
- **Principe #5 - Un acteur, une responsabilité** : `EventsService` =
  acteur EventBus côté Rust, sans état côté Python.
- **Principe #8 - CLI humaine, API machine** : events alimentent à la
  fois l'UI humaine (operator/builder) et l'API machine (HTTP SSE).

## Liens

- ADR-101 - `ctx` Protocol (ajoute `ctx.events`)
- ADR-106 - Logger structuré (escape hatch pour events custom)
- ADR-098 - Decorator-first (`ctx.events` exposé via Protocol cohérent)
- Mémoire `project_sprint42_frontend` - builder mode "plus transparent
  que Claude.ai" (consommateur principal de ces events)
