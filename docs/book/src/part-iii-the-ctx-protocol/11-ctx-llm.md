# `ctx.llm`

`ctx.llm` (Protocol `LlmProxy`) est l'unique surface d'accès au LLM. Quatre méthodes principales : `complete`, `chat`, `stream`, `embed`. Le backend utilisé est résolu par le LlmRouter du runtime (local llama.cpp, Ollama, Anthropic, OpenAI, Vertex), invisible côté agent sauf si l'agent veut forcer un backend.

---

## `complete(messages)` : appel one-shot

```python
response = await ctx.llm.complete(
    messages=[
        {"role": "system", "content": "You are a careful assistant."},
        {"role": "user", "content": "Summarize this transcript in 3 bullets."},
    ],
    temperature=0.3,
    max_tokens=500,
)

print(response.content)              # str : le texte final
print(response.latency_ms)            # int : ms de génération
print(response.usage.prompt_tokens)   # int
print(response.usage.completion_tokens)
print(response.usage.cost_usd)        # float : coût estimé
```

Signature complète :

```python
async def complete(
    self,
    messages: list[dict[str, Any]],
    *,
    backend: str | None = None,
    temperature: float = 0.7,
    max_tokens: int | None = None,
) -> LlmResponse: ...
```

Argument `backend` : laissez `None` pour utiliser le backend par défaut configuré au niveau session. Forcez (`"anthropic"`, `"openai"`, `"local"`, `"ollama"`) quand l'agent a un besoin spécifique (vision, contexte long, latence locale).

`LlmResponse` expose `content`, `latency_ms`, et `usage` (avec `prompt_tokens`, `completion_tokens`, `cost_usd`).

---

## `chat(system, user)` : raccourci deux messages

Si vous n'avez qu'un system prompt et un message utilisateur, le raccourci `chat` est plus lisible :

```python
response = await ctx.llm.chat(
    system="You are a French-to-English translator.",
    user="Le chat dort sur le tapis.",
    temperature=0.0,
)
return {"translation": response.content}
```

Sous le capot, `chat` assemble `[{"role": "system", ...}, {"role": "user", ...}]` et appelle `complete`. Pas de différence de coût ni de performance.

---

## `stream(messages)` : itération token par token

`stream` est une méthode **async** qui retourne un `AsyncIterator[str]`. Pattern : `await` la méthode pour obtenir l'itérateur, puis `async for` pour consommer les tokens.

```python
stream = await ctx.llm.stream(messages=[...])
async for token in stream:
    ctx.events.emit_token(token)
```

Cas d'usage typique : le mode conversationnel (`@on_message`). Le streaming UI passe par `ctx.events.emit_token(...)` qui pousse chaque chunk vers l'app Desktop ou la CLI. Le détail des événements est au [chapitre 17](17-ctx-events-logger-budget.md).

Annulation : si la tâche est annulée par l'utilisateur ou si le budget est épuisé, la prochaine itération lève une exception. Le runtime se charge de fermer proprement la connexion côté backend.

---

## `embed(text)` : vecteur d'embedding

```python
vec = await ctx.llm.embed(text="Le runtime Apollia est local-first.")
# vec : list[float] de longueur 768 (par défaut) ou 1024/1536 selon le backend
```

Le backend par défaut est un modèle GGUF local bundlé, qui n'envoie rien au réseau. Vous pouvez forcer un backend cloud si vous voulez la même métrique qu'un service tiers.

L'usage typique : peupler `ctx.memory` avec des entrées vectorisées pour rechercher par similarité plus tard. La [recherche FTS5](12-ctx-memory.md) reste l'option par défaut, l'embedding est utile pour les cas où la similarité sémantique compte plus que les mots-clés.

---

## Choisir le bon backend

`ctx.llm` ne sait pas, et n'a pas à savoir, lequel des backends est en jeu. Trois patterns sont courants :

- **Aucun argument `backend`.** Le LlmRouter choisit selon la configuration de la session. C'est l'option par défaut, à privilégier.
- **`backend="local"` explicite.** Quand l'agent tient à rester offline (souveraineté). Le runtime fail-fast si aucun modèle local n'est configuré.
- **`backend="anthropic"` ou autre cloud.** Quand l'agent a besoin d'une capacité spécifique (vision, contexte long). Pensez à déclarer la clé via `secrets=("anthropic_api_key",)` dans `@agent` (cf. [chapitre 16](16-ctx-secrets.md)).

---

## Quand utiliser `apollia.react` au lieu de `ctx.llm`

`ctx.llm.complete` est un appel atomique : un prompt, une réponse. Quand l'agent doit appeler des outils en boucle (raisonner, agir, observer, recommencer), passez par `apollia.react(ctx, ...)` (cf. [chapitre 14](14-ctx-a2a.md)). C'est la même chose qu'une boucle manuelle avec `ctx.llm`, mais le runtime gère le step budget, le formatage des tool calls, et l'arrêt sur convergence.

---

> **Référence technique :** la page wiki `Briques-LLM-Engine` détaillera la liste des backends, leurs caractéristiques, et la configuration *(wiki disponible prochainement)*.

---

## Anti-patterns

**Ne pas** appeler le SDK Anthropic / OpenAI en direct (`from anthropic import Anthropic`) depuis un agent. Vous perdez l'isolation, le budget, le cache, le routing, et la possibilité de basculer sur un backend local sans toucher au code.

**Ne pas** boucler en `while True` sur `ctx.llm` pour faire du tool calling à la main. Utilisez `apollia.react` (cf. [chapitre 14](14-ctx-a2a.md)) qui implémente la boucle proprement avec un step budget.

**Ne pas** stocker `response.content` dans une mémoire conversationnelle sans `ctx.memory.record` ou `ctx.memory.remember`. Sinon, l'historique vit dans une variable Python qui disparaît à la fin de la tâche.

**Ne pas** ignorer `response.usage.cost_usd`. Logguez-le via `ctx.logger.info("llm done", extra={"cost_usd": response.usage.cost_usd})` pour suivre la consommation en observabilité.

---

## ADRs

- `ADR-101` : Ctx exhaustif et typé
- `ADR-047` : Multi-LLM backend registry
- `ADR-057` : Prompt caching strategy
- `ADR-112` : Stream cleanup et rename

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
