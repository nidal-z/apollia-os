# ADR-098 - Apollia AgentKit : decorator-first, agent unifié

**Date :** 2026-05-19
**Statut :** Accepté
**Sprint :** Phase release v0.1.0 (refonte SDK)

---

## Contexte

Le SDK Python `apollia` (v0.4.0) repose sur une **hiérarchie d'héritage à 4
classes** introduite incrementalement depuis le sprint 12. Chaque classe parente
ajoute son propre cycle de vie, son propre dispatch, sa propre manière
d'exprimer un manifeste, et son propre vocabulaire d'erreurs. L'écosystème
agents en a hérité d'une dette manifeste.

**État observé au 2026-05-19 :**

- `sdk/apollia/agents/react.py` - **676 LOC** pour `BaseReActAgent` (boucle
  Observer-Reasoner-Actor, gestion d'erreurs JSON, streaming optionnel,
  fallbacks `getattr(ctx, "emit_thought", lambda *a: None)` pour les events,
  budget steps interne).
- `sdk/apollia/agents/worker.py` - **125 LOC** pour `WorkerAgent` qui
  redéclare un dispatch `op → handler` via `register_skill()` et un
  `manifest()` que chaque sous-classe doit redéfinir.
- `sdk/apollia/agents/conversational.py` - **126 LOC** pour
  `ConversationalAgent` qui dupplique la boucle LLM mais sans ReAct, avec un
  `system_prompt` géré différemment.
- `sdk/apollia/agents/orchestrated.py` - **103 LOC** pour `OrchestratedAgent`
  qui ne fait que wrapper `ctx.a2a_invoke()` mais introduit pourtant un cycle
  de vie supplémentaire (`on_subtask_failed`, `on_plan_step_done`).
- **Total héritage : 1 030 LOC** pour un comportement que les agents en
  production utilisent à ~20 % (la majorité du code branché est de la
  glue défensive type `getattr` ou `try/except` pour absorber les
  divergences inter-classes).

Mesure côté agents bundled :

- `agents/veille-ia/workers/web-search-worker.py` : **~1 100 LOC** dont
  **~210 LOC de boilerplate dispatch** (`if op == "...": return ...`),
  réécrites quasi à l'identique dans `entity-extraction-worker.py`,
  `synthesis-worker.py`, `pdf-extract-worker.py`, etc. Soit **~1 050 LOC
  dupliqués** sur 5 workers du repo.
- L'auteur d'agent doit choisir sa classe parente **avant** d'avoir écrit
  une ligne de logique métier. La frontière entre `WorkerAgent`,
  `BaseReActAgent` et `ConversationalAgent` n'est documentée que dans le
  wiki (`Briques-SDK.md`) et reste systématiquement source de confusion.

Une revue du code montre que les 4 classes partagent en réalité **un seul
contrat utile au runtime** : exposer un `agent` au module avec une méthode
`async run(task, ctx)`. Tout le reste est du sucre cosmétique enrobé dans de
l'héritage, qui complique la testabilité (mock d'une méthode parente),
empêche la composition (un agent ne peut pas être à la fois `Worker` ET
`Conversational`), et rend les futurs ajouts de capabilities (ex. orchestré
multi-skill) impossibles sans 5° classe.

## Décision

**Nous adoptons un design decorator-first avec un seul décorateur de classe
`@agent`, complété par des décorateurs additifs de méthode (`@skill`,
`@on_message`, `@orchestrated`), et nous supprimons l'intégralité de la
hiérarchie `BaseReActAgent` / `WorkerAgent` / `ConversationalAgent` /
`OrchestratedAgent` sans shim de rétrocompat.**

Concrètement :

1. **`@agent(name, version, ...)`** est l'unique décorateur de classe. Il
   construit le `__apollia_manifest__` au load time (introspection des
   méthodes décorées), instancie la classe, expose l'instance au module
   (`module.agent = instance`), et installe le dispatcher boundary qui trappe
   les exceptions et formate les retours en `AIPResult` (cf. ADR-100,
   ADR-109).
2. **`@skill(id, description=None)`** décore une méthode async pour la
   déclarer comme skill A2A invocable. La signature de la méthode EST le
   schéma I/O (cf. ADR-099).
3. **`@on_message`** décore la méthode qui pilote le mode conversationnel
   (interaction libre avec un humain). Un agent peut combiner `@skill` ET
   `@on_message` sans gymnastique.
4. **`@orchestrated`** décore la méthode chef-d'orchestre d'un agent
   director (qui appelle `ctx.a2a.invoke(...)` pour déléguer à des workers).
   Pas de classe parente requise.
5. **ReAct devient une utility runtime** - plus une classe parente.
   `ctx.react(messages, tools=..., max_steps=...)` lance une boucle
   Observer-Reasoner-Actor déterministe gérée par le SDK. L'agent reste
   maître de l'enchaînement (il peut faire deux `ctx.react()` consécutifs,
   ou pas du tout, ou en imbriquer un dans un `@skill`).

Exemple cible (worker multi-skill complet) :

```python
from apollia import agent, skill, DomainError

@agent(name="docx-worker", version="1.0.0")
class DocxWorker:
    @skill("extract_text")
    async def extract(self, path: str, ctx) -> dict:
        if not path.endswith(".docx"):
            raise DomainError("UNSUPPORTED", f"Not a docx: {path}")
        return {"text": await self._read(path, ctx)}

    @skill("count_pages")
    async def count(self, path: str, ctx) -> dict:
        return {"pages": 12}
```

Soit **~15 LOC pour 2 skills** vs ~400 LOC sous l'ancienne hiérarchie.

## Alternatives considérées

### Option A - Conserver `BaseReActAgent` comme parente unique (rejetée)

**Pour :** moins de breaking changes, courbe d'apprentissage incrémentale,
réutilise le code existant de la boucle ReAct.
**Contre :** force tous les agents (même un worker qui ne fait que parser
un PDF) à hériter d'une boucle LLM qu'ils n'utilisent pas. La méthode
`run()` reste l'unique entry point - pas de multi-skill natif sans
recoder le dispatch côté sous-classe. Ne résout pas le problème de fond :
1 000+ LOC d'héritage défensif persistent.

### Option B - Garder l'héritage mais en faire des mixins composables (rejetée)

**Pour :** un agent pourrait combiner `WorkerMixin` + `ConversationalMixin`,
le run-time choisit le bon dispatcher selon la présence de méthodes.
**Contre :** la cognition côté auteur empire (l'ordre des mixins compte, le
MRO Python est piège), et le SDK doit toujours implémenter 4 dispatchers
internes. La duplication LOC ne disparaît pas - elle migre dans les mixins.
Aucun gain de testabilité.

### Option C - DSL externe (YAML + bouts de code Python) à la LangFlow (rejetée)

**Pour :** zéro Python à écrire pour les cas simples, "no-code".
**Contre :** trahit le principe #3 (contrat minimal duck-typing) et la
philosophie "des vrais agents, pas des flowcharts". Casse l'autocomplete
IDE, force un parser custom, et complique la review de code (audit
sécurité = relire du YAML).

### Option retenue - decorator-first, agent unifié

**Pour :** un seul concept côté auteur (`@agent` + décorateurs additifs),
introspection statique (au load time) ⇒ `apollia inspect` peut tout
afficher (cf. ADR-110) sans démarrer le runtime, composition naturelle
(un agent peut avoir N skills + un on_message + un orchestrated), aucune
hiérarchie ⇒ tests unitaires triviaux (`DocxWorker()` instancié comme une
classe normale en test).
**Compromis acceptés :** breaking change total - aucun agent existant ne
fonctionne sans réécriture. Documentation à refaire de zéro. Skills
`apollia-agent-forge` et `apollia-worker-forge` à refondre (cf. LOT 12 du
plan).

## Conséquences

**Positives :**

- Volume code par agent : un worker multi-skill passe de ~1 100 LOC à
  **~150 LOC** (mesuré sur le portage cible de `web-search-worker.py`).
- Zéro classe parente ⇒ tests Python = `pytest` standard, pas de mock de
  `BaseReActAgent`.
- Composition libre : un même agent peut exposer 3 skills A2A ET un
  on_message ET orchestrer 2 workers. La hiérarchie ne le permettait pas.
- L'introspection statique (`agent.__apollia_manifest__`) devient la source
  de vérité du manifest, alimente `apollia inspect` (ADR-110), et permet
  au runtime Rust de valider la conformité au load sans démarrer Python.
- Le SDK retire 1 030 LOC d'héritage `agents/*.py`. La logique ReAct
  reste, mais déménage dans `_internal/react_loop.py` et n'est plus
  exposée comme classe publique.
- Cohérence avec les frameworks modernes (FastAPI `@app.get`, Pydantic
  `@validator`, dataclasses `@dataclass`) - courbe d'apprentissage proche
  de zéro pour un dev Python contemporain.

**Négatives / Compromis :**

- Tous les 10 agents bundled (`veille-ia`, `onboarding-agent`,
  `apollia-guide`, `markdown-summarizer`, `code-review-multi`, etc.)
  doivent être réécrits (LOT 13 du plan). Effort estimé 4-5j cumulés.
- Pas de mécanisme d'extension par sous-classe (`class MyWorker(WorkerAgent)`
  ne fonctionne plus). Si un besoin de "framework dérivé" émerge
  (ex. `BaseRAGAgent`), il faudra concevoir un mécanisme de mixin
  decorator-based, pas via héritage.
- La validation runtime est plus déclarative (au load) qu'imperative - un
  bug de signature `@skill` ne fail plus à l'exécution mais au load, ce
  qui change l'UX d'erreurs (alignement avec principe #4).

**À surveiller :**

- Adoption auteurs externes (post-release) : si l'absence d'héritage
  déroute, prévoir un guide "venant de LangChain / CrewAI" dans le book.
- Émergence d'un besoin de hooks `before_skill` / `after_skill` -
  envisager des décorateurs `@before(skill_id)` plutôt que des méthodes
  parentes.
- Coût de l'introspection au load : sur 100 agents installés simultanément,
  mesurer le `import time`. Si > 500 ms, paresser l'instanciation.

## Principes architecturaux impactés

- **Principe #3 - Contrat minimal** : renforcé. L'agent est désormais une
  classe + des décorateurs ; plus aucune méthode parente à connaître.
- **Principe #4 - Fail fast** : renforcé. Le manifest est construit au
  load Python - toute incohérence (signature invalide, skill_id en
  doublon, mauvais type d'annotation) lève à l'import, pas au premier
  appel.
- **Principe #2 - Zéro dépendance externe** : préservé. Les décorateurs
  sont implémentés en stdlib pur (`functools`, `inspect`, `typing`).

## Liens

- ADR-099 - Signature inference comme schéma I/O (compagnon natif)
- ADR-100 - Exceptions typées au boundary
- ADR-101 - `ctx` exhaustif via `Protocol`
- ADR-102 - API A2A unifiée
- ADR-107 - `@agent` instancie et expose `agent` au module
- ADR-109 - `AIPResult` interne au SDK
- ADR-110 - `apollia inspect` CLI
- ADR-014 - Bridge PyO3 async (préservé, alimente le boundary)
- ADR-083 - Trust model agents Python (alignement signature validation)
