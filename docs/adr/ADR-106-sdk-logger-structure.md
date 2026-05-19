# ADR-106 — Logging structuré via `ctx.logger`

**Date :** 2026-05-19
**Statut :** Accepté
**Sprint :** Phase release v0.1.0 (refonte SDK)

---

## Contexte

Le logging côté agent Python est exposé aujourd'hui via une méthode
`ctx.log(level: str, message: str)` minimaliste sur `RuntimeContext`.

**État observé au 2026-05-19** :

- Signature actuelle : `ctx.log("info", "Some message")` — level en
  string (typo possible : `"infos"`, `"warn"` vs `"warning"`...), pas
  de structured fields, pas de hierarchical logger name.
- Côté Rust, `ctx.log` re-dispatche vers `tracing::info!`/`warn!`/`error!`
  selon le level — mais aucun champ structuré ne survit (le message est
  un blob string).
- Auteurs d'agent recourent souvent à `print(...)` (visible nulle part
  en prod) ou à `import logging; logger = logging.getLogger(__name__)`
  qui crée un logger Python standard non branché au tracing Rust. Les
  logs disparaissent.
- Pas de convention de naming — chaque agent invente son préfixe
  (`[WORKER] message`, `[VEILLE-IA]`, etc.). Pas filtrable par agent.
- Tracing Rust supporte parfaitement les `extra` fields structurés
  (`tracing::info!(query = %q, count = items.len(), "fetched")`) — la
  passerelle Python ne les expose pas.
- `logging` stdlib supporte aussi les `extra` fields via
  `logger.info("msg", extra={"key": "val"})` — pont naturel possible.

Le constat : on a un mécanisme côté Python (stdlib `logging` avec
`extra`) et un mécanisme côté Rust (tracing avec fields). Ils ne sont
pas reliés. Un événement structuré côté agent perd toute structure au
passage du bridge.

## Décision

**Nous adoptons `ctx.logger`, un `logging.Logger` stdlib pré-configuré
au nom `apollia.agent.<agent_name>`, dont les enregistrements sont pipés
vers `tracing` Rust via un handler custom qui préserve les `extra` fields
en tant que `tracing` fields structurés.**

Surface publique :

```python
# Dans le Ctx Protocol (ADR-101)
class Ctx(Protocol):
    logger: logging.Logger  # stdlib, déjà familier
    ...
```

Usage :

```python
@skill("fetch")
async def fetch(self, url: str, ctx) -> dict:
    ctx.logger.info("fetching", extra={"url": url})
    try:
        ...
    except SomeError as exc:
        ctx.logger.warning(
            "fetch_failed",
            extra={"url": url, "error_type": exc.__class__.__name__},
            exc_info=True,
        )
        raise DomainError("FETCH_FAILED", str(exc))
```

Détails d'implémentation :

1. **Nom hiérarchique** — `apollia.agent.veille-ia.web-search-worker`.
   Permet le filtrage CLI (`apollia logs --agent veille-ia`) et la
   configuration de log levels par agent.
2. **Handler custom `ApolliaTracingHandler`** — sous-classe de
   `logging.Handler` qui convertit chaque `LogRecord` en appel
   `tracing::event!` côté Rust via PyO3. Les `extra` fields deviennent
   des champs tracing structurés.
3. **Pré-configuration** — le SDK boundary configure ce handler au
   démarrage du runtime Python. L'auteur ne touche jamais à `logging`
   directly.
4. **Niveau par défaut** — `INFO`. Configurable par agent via
   `~/.apollia/config.toml` (`log_level.veille-ia = "DEBUG"`).
5. **Champs auto-ajoutés à chaque log** — `agent_id`, `task_id`,
   `step_id` (si dans une boucle ReAct). Le builder ne les passe pas
   manuellement.
6. **`stdout`/`stderr` capture** — par défaut, `print(...)` côté agent
   est capturé et redirigé vers `ctx.logger.info(...)` avec un préfixe
   `[stdout]`. Permet la migration progressive des `print` legacy
   sans perte.
7. **Pas d'async** — `logging` stdlib est synchrone. L'envoi vers
   tracing Rust est `mpsc::send` non-bloquant.

## Alternatives considérées

### Option A — Conserver `ctx.log(level, msg)` mais ajouter un `extra` (rejetée)

**Pour :** rétrocompat partielle.
**Contre :** maintient la sémantique "level string" propice aux typos.
Pas d'écosystème stdlib `logging` (filters, handlers tiers, etc.).
Maintien d'une API custom Apollia là où la stdlib suffit.

### Option B — Wrapper opinionné `ctx.logger` avec API custom
(rejetée)

**Pour :** signature contrôlée (`ctx.logger.info(message, **fields)`
au lieu de `extra={...}`).
**Contre :** réinvente `logging` mal. Les auteurs Python connaissent
déjà `logger.info(...)` avec `extra` — pas la peine de leur changer
leurs habitudes.

### Option C — `tracing` direct côté Python via crate
`opentelemetry-python` (rejetée)

**Pour :** standard moderne.
**Contre :** dépendance externe Python (viole principe #2). Setup
lourd pour gagner peu vs `logging` + handler custom.

### Option retenue — `logging.Logger` stdlib + handler custom vers tracing Rust

**Pour :** zéro nouvelle API à apprendre (`logging` est universel),
les `extra` fields se traduisent naturellement en tracing fields, le
naming hiérarchique permet du filtering granulaire, stdlib only.
**Compromis acceptés :** le bridge stdlib→tracing est custom (~80 LOC
côté SDK `_internal/log_bridge.py` + ~50 LOC côté Rust pour exposer
`tracing::event!` dynamique). Documenté.

## Conséquences

**Positives :**

- Auteurs Python utilisent leur idiom habituel (`ctx.logger.info(...)`,
  `logger.warning(...)`). Zéro friction.
- Les `extra` fields traversent le bridge sans perte. Logs filtrables
  par champ côté tracing-subscriber Rust.
- Naming hiérarchique = filtering CLI propre (`apollia logs --agent X`).
- `print()` capturé = migration sans perte des agents legacy
  (ce qui était un blackhole devient visible).
- Cohérence parfaite Python ↔ Rust : un log côté agent et un log côté
  runtime se cumulent dans le même flux tracing.
- Configuration runtime des log levels par agent (sans redémarrage si
  on implémente le reload — non-bloquant v1.0).

**Négatives / Compromis :**

- Le handler custom est code spécifique Apollia — à maintenir au fil
  des versions Python (3.10+ pour le moment, OK).
- Capture `print()` peut surprendre un auteur qui débogue avec
  `print("here")` et le voit dans son tracing instead of stdout.
  Documenter ; option `disable_print_capture` configurable en debug.
- Latence par log : un appel cross-bridge PyO3 par event. Mesurable
  mais négligeable (< 10 µs/log en bench). À surveiller pour les agents
  qui logguent en boucle serrée.

**À surveiller :**

- Volume de logs élevés (> 10k logs/s) — envisager batching côté
  handler.
- Demande pour OpenTelemetry compat post-v1.0 — design tracing fields
  pour faciliter un futur export OTLP.
- Émergence d'agents qui veulent du `logging.getLogger("foo")` custom
  (en plus du logger pré-configuré) — autoriser, le handler est attaché
  au root logger Apollia.

## Principes architecturaux impactés

- **Principe #2 — Zéro dépendance externe** : préservé strictement
  (`logging` stdlib uniquement côté Python).
- **Principe #8 — CLI humaine, API machine** : les logs sont consommés
  à la fois par les humains (`apollia logs`) et par les outils (export
  JSON / SSE).
- **Principe #5 — Un acteur, une responsabilité** : le bridge log =
  un acteur côté Rust qui reçoit les events et les push à tracing.

## Liens

- ADR-101 — `ctx` Protocol (ajoute `ctx.logger`)
- ADR-105 — `ctx.events` (events sémantiques typés, complémentaire
  des logs)
- ADR-100 — Exceptions au boundary (le boundary log les `UNHANDLED` via
  `ctx.logger.error`)
- ADR-014 — Bridge PyO3 async (l'extension log s'y greffe)
