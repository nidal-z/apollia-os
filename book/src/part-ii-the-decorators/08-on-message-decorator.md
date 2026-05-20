# Le décorateur `@on_message`

`@on_message` marque la méthode qui pilote le **mode conversationnel** d'un agent, c'est-à-dire l'interaction libre avec un humain dans l'app Desktop ou dans le chat CLI. C'est le pendant naturel de `@skill` : `@skill` répond à des invocations machine (CLI, A2A) avec des payloads structurés ; `@on_message` répond à du texte humain en flux.

Un agent peut combiner les deux. Un assistant peut exposer un `@skill("inbox.summary")` pour les autres agents **et** un `@on_message` pour discuter avec l'utilisateur, sur la même classe.

---

## Exemple minimal

```python
from apollia import agent, on_message
from apollia.types import Ctx, Message

@agent(
    name="apollia-guide",
    version="0.1.0",
    description="Friendly product coach for new Apollia users.",
)
class ApolliaGuide:
    SYSTEM_PROMPT = "You are a helpful product coach for Apollia OS."

    @on_message
    async def chat(
        self,
        message: str,
        history: list[Message],
        ctx: Ctx,
    ) -> str:
        full = ""
        async for token in ctx.llm.stream(
            messages=[
                {"role": "system", "content": self.SYSTEM_PROMPT},
                *history,
                {"role": "user", "content": message},
            ],
        ):
            ctx.events.emit_token(token)
            full += token
        return full
```

À chaque tour de chat, le runtime appelle `agent.chat(message, history, ctx)`. La méthode décide elle-même comment construire la réponse : passer par `ctx.llm.stream()` (cas standard), interroger une mémoire, appeler des outils via `ctx.tools.call(...)`, ou orchestrer une boucle ReAct via `apollia.react(...)` (cf. [chapitre 4](../part-i-getting-started/04-quickstart-director.md)).

---

## Signature

`@on_message` n'accepte **aucun argument**. C'est le décorateur le plus simple du SDK.

```python
@on_message
async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str: ...
```

La méthode doit :

- être `async def` ; sinon `AgentConfigError` au load ;
- accepter `message: str`, `history: list[Message]` et `ctx: Ctx` ;
- retourner une `str` (la réponse finale, même si elle a déjà été streamée token par token).

### `message`

La nouvelle entrée utilisateur, déjà décodée en `str`.

### `history`

L'historique de la conversation sous forme de liste de `Message` (cf. `apollia.types.Message`). Chaque entrée a au moins un `role` (`"user"` ou `"assistant"`) et un `content`. Le runtime tronque l'historique selon la stratégie de la session (sliding window, summarization).

### `ctx`

Le `Ctx` complet, avec ses 14 services. En conversationnel, vous utiliserez surtout `ctx.llm` (génération), `ctx.events.emit_token(...)` (streaming UI), `ctx.memory` (rappels), parfois `ctx.tools` ou `ctx.a2a` (cf. [Partie III](../part-iii-the-ctx-protocol/10-ctx-overview.md)).

---

## Streaming

Le contrat de retour est **une string finale**, mais la valeur ajoutée du mode conversationnel est l'affichage incrémental. Émettez chaque token via `ctx.events.emit_token(token)` :

```python
@on_message
async def chat(self, message, history, ctx) -> str:
    full = ""
    async for token in ctx.llm.stream(messages=[...]):
        ctx.events.emit_token(token)
        full += token
    return full
```

L'app Desktop et la CLI affichent les tokens en temps réel ; la valeur retournée est consignée dans l'historique. Les détails du streaming sont au [chapitre 17](../part-iii-the-ctx-protocol/17-ctx-events-logger-budget.md).

---

## Combiner `@skill` et `@on_message`

Aucun conflit : ce sont des entrées orthogonales. La même classe peut servir deux usages.

```python
from apollia import agent, skill, on_message
from apollia.types import Ctx, Message

@agent(name="inbox", version="1.0.0", description="Email triage assistant.")
class Inbox:
    @skill("inbox.summary", description="Summarize unread emails.")
    async def summary(self, since: str, ctx: Ctx) -> dict:
        return {"emails": [...]}

    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        ...
```

`inbox.summary` est invocable depuis un autre agent ou la CLI ; `chat` reçoit les messages humains côté UI. Les deux partagent la mémoire, les datasources et les secrets de l'agent.

---

## Contraintes validées au load

`@on_message` lève `AgentConfigError` si :

- la méthode n'est pas `async def` ;
- la méthode n'est pas un callable.

`@agent` enforce en plus :

- **au plus un** `@on_message` par classe (deux ⇒ `AgentConfigError`) ;
- mutuelle exclusion avec `@orchestrated`.

Vous n'avez **pas** besoin de retomber sur l'absence de `@on_message` : si l'agent n'expose pas de handler conversationnel, le runtime renvoie une erreur claire à l'utilisateur qui tente de chatter avec lui. Inutile d'ajouter un stub vide.

---

## Anti-patterns

**Ne pas** rendre la méthode non-async :

```python
# CASSE au load
@on_message
def chat(self, message, history, ctx): ...   # ⇒ AgentConfigError
```

**Ne pas** déclarer plusieurs `@on_message` :

```python
@on_message
async def chat_v1(self, ...): ...

@on_message
async def chat_v2(self, ...): ...   # ⇒ AgentConfigError au @agent
```

**Ne pas** retourner autre chose qu'une `str`. Le runtime journalise la valeur en historique ; un objet non-sérialisable casse la session.

---

## ADRs

- [ADR-098](https://github.com/nidal-z/apollia-os/blob/main/docs/adr/ADR-098-apollia-agentkit-decorator-first.md) : Decorator-first
- [ADR-040](https://github.com/nidal-z/apollia-os/blob/main/docs/adr/ADR-040-onboarding-conversational-agent.md) : Pattern agent conversationnel (préservé)
