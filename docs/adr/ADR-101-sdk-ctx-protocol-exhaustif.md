# ADR-101 - `ctx` exhaustif et typé via `Protocol`

**Date :** 2026-05-19
**Statut :** Accepté
**Sprint :** Phase release v0.1.0 (refonte SDK)

---

## Contexte

Le `ctx` injecté dans les agents Python est aujourd'hui un objet PyO3 opaque
exposé par `crates/apollia-aip/src/context.rs` (struct `RuntimeContext`).
Côté SDK, des stubs typing existent dans `sdk/apollia/stubs/` pour aider
l'IDE - mais ils sont **fragmentés**, **incomplets**, et **divergent du
runtime** :

**État observé au 2026-05-19** (audit `sdk/apollia/stubs/`) :

- `context.py` (**230 LOC**) liste ~25 méthodes/propriétés au niveau
  racine de `ctx` : `llm`, `memory`, `tools`, `notify`, `profile`,
  `stt`, `send`, `receive`, `delegate`, `a2a_invoke`, `a2a_discover`,
  `a2a_list_skills`, `log`, `emit_token`, `emit_thought`, `emit_retry`,
  `emit_action_parse_error`, `step_budget`, `task_id`, `agent_id`,
  `cancel_event`, `workspace_dir`, etc. Tout est plat - `ctx.send` et
  `ctx.a2a_invoke` côtoient `ctx.llm` (qui lui est nested).
- `llm.py` (136 LOC), `memory.py` (132 LOC), `tools.py` (43 LOC),
  `notify.py` (38 LOC), `profile.py` (72 LOC), `stt.py` (42 LOC) :
  6 fichiers, 6 protocoles distincts. Pas de fichier pour `a2a`,
  `datasources`, `templates`, `secrets`, `events`, `logger`, `budget`,
  `workspace` - ces surfaces existent côté runtime mais ne sont pas
  typées côté SDK.
- L'auteur d'agent qui tape `ctx.` dans son IDE voit ~25 entrées sans
  hiérarchie ni catégorisation. La majorité (`emit_*`, `log`, `send`,
  `receive`) sont du sucre runtime, pas du métier.
- `ctx.datasources` et `ctx.templates` **n'existent pas** au runtime
  (cf. ADR-103) - l'auteur d'un agent doit `ctx.tools.invoke("file_read",
  path="datasources/foo.yaml")` puis parser le YAML soi-même.
- Le runtime expose des champs (ex. `ctx.workspace_dir: Path`,
  `ctx.cancel_event: Event`) qui ne sont pas dans les stubs ⇒
  l'IDE les flag undefined alors qu'ils marchent.
- Plusieurs `getattr(ctx, "emit_thought", lambda *a: None)` dans
  `sdk/apollia/agents/react.py:187` (`_emit_safe`) prouvent que le SDK
  lui-même ne fait pas confiance à la disponibilité des attributs.
- `mypy --strict` ne passe pas sur un agent moyen - trop de
  `getattr` + `# type: ignore`.

Le problème de fond : le SDK et le runtime ont divergé. Il n'existe **aucune
source de vérité unique** pour la surface de `ctx`. Chaque ajout côté
Rust se traduit par un patch à 3 endroits (struct PyO3, stub typing,
documentation wiki) - et le 2e ou le 3e est souvent oublié.

## Décision

**Nous adoptons un `Ctx` Protocol unifié exposant 100 % du backend Apollia
via 14 services nestés typés. Le Protocol est l'unique source de vérité
côté SDK ; le runtime Rust (`RuntimeContext`) DOIT exposer exactement les
mêmes attributs. Toute divergence est un fail-fast au load (cf. ADR-110).**

Surface cible (`apollia.types.Ctx`, dispo via `from apollia import Ctx` et
détecté par convention de paramètre `ctx`) :

```python
class Ctx(Protocol):
    # --- IA & raisonnement ---
    llm: LlmService              # complete, stream, embed
    react: ReactService          # ctx.react(messages, tools=..., max_steps=...)

    # --- Mémoire & profil ---
    memory: MemoryService        # remember/recall/search/forget + export/import
    profile: ProfileService      # ctx.profile.name / .role / .get / .set / .all

    # --- Outils & A2A ---
    tools: ToolsService          # invoke, list, describe
    a2a: A2AService              # invoke, discover, list_skills, skill_as_tool

    # --- Données & contenu ---
    datasources: DatasourcesService  # get(name), list()
    templates: TemplatesService      # render(name, **vars), list()
    secrets: SecretsService          # get(key) read-only

    # --- Observabilité ---
    events: EventsService        # emit_thought, emit_token, emit_retry, …
    logger: logging.Logger       # ctx.logger.info(...) structuré
    budget: BudgetService        # remaining, consumed, max
    notify: NotifyService        # desktop / webhook

    # --- I/O annexes ---
    stt: SttService              # transcribe
    workspace: WorkspaceService  # path, project_id, agent_id, scratch_dir
```

14 services nestés. Tous typés via `Protocol`. Aucun attribut public sur
`ctx` racine en-dehors de ces 14 ⇒ surface API drastiquement réduite et
catégorisée.

Règles d'implémentation :

1. **`Protocol` (PEP 544) + `runtime_checkable`** - autocomplete IDE
   native, `mypy --strict` passe, structural typing (le mock testing
   ADR - LOT 10 - n'a qu'à exposer les méthodes utilisées).
2. **Source de vérité côté SDK** - les 14 protocoles vivent dans
   `sdk/apollia/types/ctx.py` (et services dans `sdk/apollia/types/services/`).
   Le runtime Rust valide au load qu'il expose ces 14 attributs ;
   `apollia inspect` (ADR-110) fait la même vérification.
3. **Pas d'attribut "magique"** au niveau racine. Tout passe par un des
   14 services. `ctx.workspace_dir` devient `ctx.workspace.path`,
   `ctx.task_id` devient `ctx.workspace.task_id`, `ctx.cancel_event`
   devient `ctx.budget.is_cancelled()`.
4. **Stabilité** - la liste des 14 services et leurs méthodes publiques
   font partie du contrat versionné SemVer du SDK. Ajout = mineur,
   suppression/renommage = majeur.

## Alternatives considérées

### Option A - Dict-like `ctx["llm"].complete(...)` (rejetée)

**Pour :** flexible, runtime introspection facile.
**Contre :** zéro autocomplete IDE, zéro typage, l'auteur cherche les
clés disponibles dans la doc plutôt que dans son IDE.

### Option B - ABC (Abstract Base Class) au lieu de Protocol (rejetée)

**Pour :** typage explicite (l'auteur doit hériter).
**Contre :** force le mock testing à hériter aussi (rend la testabilité
lourde). Pas de structural typing.

### Option C - Garder le `ctx` plat actuel et ajouter les nouveaux
services au même niveau (rejetée)

**Pour :** moins de breaking change.
**Contre :** la surface plate devient ingérable (>40 entrées). Ne résout
pas la cognition.

### Option retenue - `Ctx` Protocol avec 14 services nestés

**Pour :** unique source de vérité, autocomplete catégorisé
(`ctx.<tab>` affiche 14 services bien nommés), `mypy --strict`
satisfait, mockable trivialement en test, validation au load (le runtime
ne match pas le Protocol ⇒ fail-fast), surface stable versionnée.
**Compromis acceptés :** breaking change total - tout agent doit
réécrire `ctx.send(...)` en `ctx.a2a.invoke(...)`, `ctx.log("info", ...)`
en `ctx.logger.info(...)`, etc. Mais les renames sont mécaniques
(LOT 13).

## Conséquences

**Positives :**

- Surface `ctx` catégorisée et explorable - un dev débutant trouve les
  capabilities sans documentation externe.
- `mypy --strict` passe sur un agent moyen - zéro `# type: ignore`
  attendus dans les agents bundled post-LOT 13.
- Le runtime Rust et le SDK ont **une seule** source de vérité (le
  Protocol). Toute dérive est détectée au load.
- Mock testing trivial : `class FakeCtx` qui n'implémente que les
  services utilisés satisfait `Protocol`.
- Évolution future : ajouter `ctx.video` ou `ctx.calendar` se fait en 1
  endroit (nouveau service + extension protocol).
- Documentation auto-générable (sphinx-autodoc ou simple introspection)
  produira une référence cohérente à partir des Protocols.

**Négatives / Compromis :**

- Plus grande surface API totale exposée (14 services × ~5 méthodes
  moyenne = ~70 méthodes documentées) - mais catégorisée vs.
  ~25 méthodes plates aujourd'hui.
- Les agents existants utilisent tous les attributs plats (`ctx.send`,
  `ctx.log`, etc.) - migration entièrement mécanique mais touchant 10
  agents.
- Le bridge Rust (`crates/apollia-aip/src/context.rs`) doit être
  refactorisé pour exposer les 14 services nestés (LOT 4). Effort estimé
  2-3j.

**À surveiller :**

- Croissance du nombre de services au-delà de 14 - si on dépasse 20,
  introduire des "domaines" (ex. `ctx.ai`, `ctx.data`, `ctx.io`).
- Coût de l'introspection au load pour vérifier la conformité Protocol
  (négligeable attendu).
- Adoption auteurs : si la migration de `ctx.send` à `ctx.a2a.invoke`
  passe difficilement, prévoir un script `apollia migrate-ctx` (LOT 13
  utility).

## Principes architecturaux impactés

- **Principe #3 - Contrat minimal** : nuancé. Le contrat de `run()` reste
  `async def run(task, ctx)` ; ce qu'on enrichit c'est la **surface
  typée** de `ctx`, pas le contrat d'invocation.
- **Principe #4 - Fail fast** : renforcé. Divergence Rust/SDK détectée
  au load.
- **Principe #5 - Un acteur, une responsabilité** : aligné. Chaque
  service `ctx.X` correspond à une crate / acteur Tokio côté Rust.

## Liens

- ADR-098 - Decorator-first (utilise `ctx` partout)
- ADR-102 - A2A unifiée (ctx.a2a)
- ADR-103 - Datasources & templates runtime (ctx.datasources, ctx.templates)
- ADR-104 - Secrets read-only (ctx.secrets)
- ADR-105 - Events publics typés (ctx.events)
- ADR-106 - Logger structuré (ctx.logger)
- ADR-014 - Bridge PyO3 async (refactor RuntimeContext)
- ADR-066 - Memory export/import (intégré dans ctx.memory)
- ADR-087 - User profile redesign (intégré dans ctx.profile)
- ADR-082 - Tool governance (alimente ctx.tools)
