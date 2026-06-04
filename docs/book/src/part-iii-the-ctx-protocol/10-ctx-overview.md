# Vue d'ensemble du protocole Ctx

À chaque invocation d'un agent, le runtime injecte un objet `ctx` dans la méthode appelée. C'est l'unique surface par laquelle l'agent accède au backend Apollia : LLM, mémoire, outils, autres agents, secrets, événements. Tout passe par là.

`Ctx` n'est pas une classe mais un `typing.Protocol`. L'agent dépend d'un contrat, pas d'une implémentation. En production, le runtime injecte un objet PyO3 piloté par le Rust ; en test, `apollia.testing.mock` injecte un faux ctx qui implémente la même surface (cf. [chapitre 24](../part-vi-testing/24-testing-isomorphic-mock.md)).

```python
from apollia import agent, skill
from apollia.types import Ctx

@agent(name="demo", version="0.1.0", description="Demo agent.")
class Demo:
    @skill("demo.do", description="Pattern d'utilisation de ctx.")
    async def do(self, query: str, ctx: Ctx) -> dict:
        response = await ctx.llm.chat(system="You are helpful.", user=query)
        ctx.logger.info("llm done", extra={"latency_ms": response.latency_ms})
        await ctx.memory.record(f"queried: {query}", importance=0.4)
        return {"answer": response.content}
```

---

## Les 14 services

`ctx` expose exactement 14 services. Aucun attribut « magique » au niveau racine en dehors de cette liste.

| Service | Type | Rôle | Chapitre |
|---|---|---|---|
| `ctx.llm` | `LlmProxy` | Génération, streaming, embeddings | [11](11-ctx-llm.md) |
| `ctx.memory` | `MemoryInterface` | Mémoire épisodique, sémantique, procédurale, FTS5 | [12](12-ctx-memory.md) |
| `ctx.tools` | `ToolProxy` | Outils natifs Apollia et serveurs MCP | [13](13-ctx-tools.md) |
| `ctx.a2a` | `A2AInterface` | Appel d'autres agents, discovery | [14](14-ctx-a2a.md) |
| `ctx.datasources` | `DatasourcesInterface` | YAML déclarés dans le manifeste | [15](15-ctx-datasources-templates.md) |
| `ctx.templates` | `TemplatesInterface` | Templates Jinja2 du package | [15](15-ctx-datasources-templates.md) |
| `ctx.secrets` | `SecretsInterface` | Lecture seule de credentials | [16](16-ctx-secrets.md) |
| `ctx.events` | `EventsInterface` | Streaming UI, observabilité ReAct | [17](17-ctx-events-logger-budget.md) |
| `ctx.logger` | `logging.Logger` | Logging stdlib piped vers Rust tracing | [17](17-ctx-events-logger-budget.md) |
| `ctx.budget` | `BudgetView` | Vue lecture seule du StepBudget | [17](17-ctx-events-logger-budget.md) |
| `ctx.notify` | `NotifyInterface` | Notifications desktop, webhook | [18](18-ctx-other-services.md) |
| `ctx.profile` | `ProfileInterface` | Profil utilisateur canonique | [18](18-ctx-other-services.md) |
| `ctx.workspace` | `WorkspaceContext` | Règles et sections d'`APOLLIA.md` | [18](18-ctx-other-services.md) |
| `ctx.stt` | `SttInterface` | Speech-to-Text | [18](18-ctx-other-services.md) |

À cela s'ajoute la fonction libre `apollia.react(ctx, system, user, tools=..., max_steps=...)` qui n'est pas un service de `ctx` mais utilise plusieurs services en interne (cf. [chapitre 14](14-ctx-a2a.md)).

---

## Pourquoi un Protocol

Trois bénéfices concrets :

1. **Autocomplete IDE.** Tapez `ctx.` dans n'importe quel IDE qui comprend Python : les 14 services apparaissent, chacun avec ses méthodes typées.
2. **`mypy --strict` passe** sur un agent moyen. Aucun `# type: ignore` ni `getattr(...)` défensif.
3. **Mock testing trivial.** Le mock fourni par `apollia.testing.mock` implémente le Protocol sans héritage. Vous pouvez aussi écrire votre propre `class FakeLlm` qui n'expose que les deux méthodes que votre test utilise.

Le runtime Rust est tenu d'exposer exactement ces 14 services. Toute dérive entre la définition Python et l'implémentation Rust est détectée au boot par le validateur (cf. [chapitre 27](../part-vii-tooling/27-apollia-inspect.md)).

---

## Catégoriser pour mémoriser

Les 14 services se rangent dans 5 catégories :

- **IA et raisonnement :** `ctx.llm`, plus la fonction libre `apollia.react`.
- **Mémoire et profil utilisateur :** `ctx.memory`, `ctx.profile`.
- **Outils et A2A :** `ctx.tools`, `ctx.a2a`.
- **Données et contenu de l'agent :** `ctx.datasources`, `ctx.templates`, `ctx.secrets`.
- **Observabilité et I/O annexes :** `ctx.events`, `ctx.logger`, `ctx.budget`, `ctx.notify`, `ctx.workspace`, `ctx.stt`.

Cette taxonomie est non-normative. Elle aide juste à savoir où chercher.

---

## Que faire si un service manque

Tous les services sont toujours injectés. Le cas où `ctx.notify` ou `ctx.stt` n'existerait pas n'arrive jamais en production : si le runtime ne pouvait pas fournir un service (configuration cassée, version incompatible), il refuserait de démarrer la tâche.

En test, un service peut être absent si votre mock customisé ne l'implémente pas. Dans ce cas, l'appel lève une `AttributeError` claire au runtime du test. Préférez `apollia.testing.mock(MyAgent)` qui injecte les 14 services en stubs.

---

## Anti-patterns

**Ne pas** stocker `ctx` dans un attribut de l'agent (`self.ctx = ctx`). Chaque invocation reçoit son propre `ctx` lié à la tâche en cours ; le réutiliser hors de la tâche d'origine produit des comportements indéterminés (budget incorrect, mémoire fuyant entre tâches, événements émis sur la mauvaise session).

**Ne pas** importer un service en haut du fichier (`from apollia.context.llm import LlmProxy`). Ces modules définissent uniquement le Protocol ; ils n'ont aucune implémentation. Utilisez les attributs de `ctx`.

**Ne pas** dupliquer une fonctionnalité que `ctx` expose déjà. Par exemple, ne ré-implémentez pas un cache LLM : `ctx.llm` peut s'appuyer sur le cache du runtime configuré globalement. De même pour la mémoire, la recherche, et l'A2A.

---

## ADRs

- `ADR-024` : Ctx exhaustif et typé via Protocol
- `ADR-023` : Decorator-first (le contrat qui injecte ctx)

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
