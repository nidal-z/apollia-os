# ADR-102 - API A2A unifiée (`ctx.a2a`)

**Date :** 2026-05-19
**Statut :** Accepté
**Sprint :** Phase release v0.1.0 (refonte SDK)

---

## Contexte

L'agent-to-agent communication (A2A) a été introduit incrémentalement, ce
qui a laissé **trois APIs concurrentes** sur `RuntimeContext` aujourd'hui
exposées au même niveau.

**État observé au 2026-05-19** (audit `sdk/apollia/stubs/context.py` et
`crates/apollia-aip/src/context.rs`) :

| API | Sémantique | Statut réel |
|---|---|---|
| `ctx.send(to_agent, message)` / `ctx.receive(timeout)` | Mailbox fire-and-forget | Présent dans les stubs, **aucun agent en production ne l'utilise** (grep dans `agents/`). Mal documentée (sémantique async floue : push vs poll ?) |
| `ctx.delegate(agent_id, subtask)` | Sub-task synchrone, raise exception sur erreur | Utilisée par `OrchestratedAgent` historiquement, **0 occurrence** dans les agents bundled non-orchestrated |
| `ctx.a2a_invoke(skill_id, payload)` / `ctx.a2a_discover(...)` / `ctx.a2a_list_skills()` | Skill-based, returns `AIPResult` sans raise | API moderne (post-sprint 38), utilisée par `agents/veille-ia/director.py` et tests A2A - **API préférée de facto** |

Conséquences :

- Trois manières d'appeler un autre agent ⇒ l'auteur choisit au hasard ou
  hérite d'un copier-coller. Sémantiques subtilement différentes (l'une
  raise, l'autre retourne, l'autre est async-only).
- `ctx.a2a_invoke` retourne un dict shape `AIPResult` que l'agent
  appelant doit ré-éplucher (`if result["status"] == "failed": ...`) -
  c'est le miroir exact du problème ADR-100 mais côté caller.
- `ctx.a2a_list_skills()` retourne une liste de skills mais aucun moyen
  d'en transformer un en "tool" pour `ctx.react` - l'auteur doit
  formater à la main le descriptif tool.
- Pas de notion de "skill_id propagé" propre au caller (cf. mémoire
  `project_a2a_skill_id_not_propagated`, résolue côté runtime - mais
  l'API caller n'a jamais été nettoyée).

Le pattern visé (FastAPI client, gRPC stub) consiste à exposer un objet
client typé avec quelques méthodes claires, et à éliminer les redondances.

## Décision

**Nous adoptons une API A2A unifiée sous `ctx.a2a` exposant exactement 4
méthodes : `invoke()`, `discover()`, `list_skills()`, `skill_as_tool()`.
Les APIs concurrentes `ctx.send` / `ctx.receive` / `ctx.delegate` /
`ctx.a2a_invoke` racine / `ctx.a2a_discover` racine / `ctx.a2a_list_skills`
racine sont supprimées sans shim.**

Surface complète :

```python
class A2AService(Protocol):
    async def invoke(
        self,
        skill_id: str,
        *,
        timeout: float | None = None,
        **kwargs,
    ) -> dict:
        """Invoque un skill A2A. Retourne le `data` du skill cible si
        completed. Raise DomainError("A2A_<CODE>", ...) si le skill cible
        a échoué - le caller reste idiomatique (cf. ADR-100)."""

    async def discover(
        self,
        skill_id: str,
    ) -> SkillDescriptor:
        """Retourne le descripteur du skill (manifest, input/output
        JSON Schema, agent fournisseur). Raise DomainError("A2A_NOT_FOUND",
        ...) si introuvable."""

    async def list_skills(
        self,
        *,
        agent: str | None = None,
    ) -> list[SkillDescriptor]:
        """Liste tous les skills disponibles (optionnellement filtrés par
        agent fournisseur)."""

    async def skill_as_tool(
        self,
        skill_id: str,
    ) -> ToolDescriptor:
        """Wrap un skill A2A en descripteur consommable par
        `ctx.react(tools=[...])`. Le LLM peut appeler le skill comme un
        tool natif."""
```

Points clés :

1. **`SkillDescriptor`** (dataclass publique) expose `id`, `agent`,
   `description`, `input_schema`, `output_schema` (issus de
   l'inférence ADR-099 du skill cible).
2. **`invoke()` se comporte en idiome caller** (ADR-100) - si le skill
   cible a `raise DomainError("CODE", msg)`, le caller voit
   `raise DomainError("A2A_CODE", msg)` (préfixe `A2A_` pour distinguer
   l'erreur locale vs distante). Si succès, retourne directement le
   dict métier.
3. **`skill_as_tool()`** est le pont vers ReAct : un agent director
   construit `tools = [await ctx.a2a.skill_as_tool("docx.extract"),
   await ctx.a2a.skill_as_tool("pdf.extract")]` et passe à
   `ctx.react(messages, tools=tools)`. Le LLM voit les skills comme
   des tools natifs ; le SDK route les calls.
4. **Suppression** :
   - `ctx.send` / `ctx.receive` → ADR-108 (dédié).
   - `ctx.delegate` → équivalent strict de `ctx.a2a.invoke(skill_id)`,
     supprimé sans remplacement (ADR-098 supprime `OrchestratedAgent`).
   - `ctx.a2a_invoke` / `ctx.a2a_discover` / `ctx.a2a_list_skills`
     racine → déplacés sous `ctx.a2a.*`.

## Alternatives considérées

### Option A - Conserver les 3 APIs en parallèle, deprecation seulement (rejetée)

**Pour :** zéro breaking change.
**Contre :** pérennise la confusion. Empêche l'introduction propre de
`skill_as_tool` qui aurait besoin de réutiliser un `discover`. Maintien
de 3× la surface à tester.

### Option B - Un seul appel `ctx.a2a(skill_id, **kwargs)` callable (rejetée)

**Pour :** ultra-minimaliste.
**Contre :** perte du `discover` et `list_skills` (utiles pour les
agents qui adaptent leur comportement au catalogue de skills). Mauvais
mapping IDE (un callable seul est moins explorable qu'un objet).

### Option C - Objets typés `A2AClient(target_agent)` (rejetée)

**Pour :** style "gRPC stub" - `client = ctx.a2a.client("docx-worker");
result = await client.extract(path=...)`.
**Contre :** demande de connaître l'agent fournisseur en hardcode →
casse la philosophie skill-based où le caller cible un `skill_id` sans
savoir qui le sert. Ré-introduit du couplage director↔worker.

### Option retenue - `ctx.a2a` avec 4 méthodes typées

**Pour :** surface stable et minimale, sémantique idiom caller (raise
sur erreur, return dict sur succès), `skill_as_tool` ouvre la voie
ReAct↔A2A unifié, alignement avec `Ctx` Protocol (ADR-101).
**Compromis acceptés :** breaking change total - tout director existant
réécrit (~10 occurrences `ctx.a2a_invoke` dans les agents bundled,
mécaniques).

## Conséquences

**Positives :**

- Une seule façon d'appeler un autre agent. Plus de question "send,
  delegate, ou invoke ?".
- Le caller bénéficie du pattern exceptions au boundary (ADR-100) - pas
  besoin de re-disséquer un `AIPResult` dict à chaque appel.
- `skill_as_tool()` débouche un cas d'usage majeur : un director ReAct
  qui découvre dynamiquement les workers et leur propose comme tools
  au LLM. Pattern utilisé en boucle dans `agents/veille-ia/director.py`,
  qui sera simplifié drastiquement (estimation -150 LOC dispatching).
- Le runtime Rust expose une seule surface (`A2AService`) - refactor
  PyO3 propre (LOT 7).
- `apollia inspect` peut afficher les skills disponibles via la même
  API que celle qu'utiliserait l'agent au runtime.

**Négatives / Compromis :**

- Migration `ctx.a2a_invoke → ctx.a2a.invoke` mécanique mais touchant
  ~10 occurrences.
- Les agents qui dépendaient de `ctx.delegate(...)` (exception-raising)
  doivent l'adopter via `ctx.a2a.invoke(...)` (qui lève aussi, cf.
  point 2 ci-dessus) - comportement préservé.
- `skill_as_tool` introduit une dépendance API entre `ctx.a2a` et
  `ctx.react` (les deux services se connaissent). Documenter le
  contrat `ToolDescriptor`.

**À surveiller :**

- Performance de `skill_as_tool` au moment de `discover` (un round-trip
  par tool exposé) - cacher localement la première fois.
- Pattern director ReAct + workers comme tools : si adoption forte,
  envisager un helper `ctx.a2a.discover_all_as_tools()` (sucre).
- Sémantique exception caller : si `DomainError("A2A_…")` paraît
  abscond, raffiner le mapping (préfixe + code original).

## Principes architecturaux impactés

- **Principe #3 - Contrat minimal** : 4 méthodes sous un namespace dédié.
- **Principe #5 - Un acteur, une responsabilité** : `A2AService` Python
  s'aligne sur l'acteur A2A côté Rust (`apollia-runtime::a2a`).
- **Principe #4 - Fail fast** : `discover` lève si skill inconnu - pas
  d'invocation aveugle.

## Liens

- ADR-101 - `ctx` Protocol exhaustif (cadre)
- ADR-098 - Decorator-first (`@skill` génère les skills consommés ici)
- ADR-099 - Signature inference (alimente `SkillDescriptor.input_schema`)
- ADR-100 - Exceptions au boundary (sémantique caller alignée)
- ADR-108 - Suppression mailbox `send/receive` (cousin direct)
- ADR-049 - A2A skill-based dispatch (si présent - concept fondateur)
- Mémoire `project_a2a_skill_id_not_propagated` (résolution backend déjà
  livrée, API frontend nettoyée ici)
