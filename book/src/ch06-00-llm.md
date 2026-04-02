# Le LLM

Dans `file-assistant`, vous avez utilisé `ctx.llm.chat()` pour résumer un fichier. Une ligne, une réponse. Mais le LLM peut faire beaucoup plus que répondre à un prompt : il peut **raisonner**, **décider**, et **agir** — en appelant des outils lui-même, en adaptant son plan au résultat de chaque action, en boucle.

Ce chapitre explique comment `ctx.llm` fonctionne, comment le configurer, et surtout comment utiliser la boucle ReAct pour construire des agents véritablement autonomes.

---

## ctx.llm — l'interface LLM

`ctx.llm` est injecté par le runtime dans chaque appel de `run()`. Il est `None` si aucun backend LLM n'est configuré — votre code doit gérer ce cas.

```python
async def run(self, task, ctx):
    if ctx.llm is None:
        return AIPResult.failed("LLM_UNAVAILABLE", "Aucun backend LLM configuré")

    # ctx.llm est disponible
    response = await ctx.llm.chat(
        system="Tu es un assistant expert.",
        user=task["input"]["parts"][0]["text"],
    )
    return AIPResult.completed(response.content)
```

---

## Les deux paradigmes d'utilisation

```
ctx.llm.chat()          → Une question, une réponse. Déterministe.
ctx.llm.run_tools()     → Le LLM décide des outils à utiliser et boucle.
                          Autonome. La vraie puissance des agents.
```

`chat()` et `complete()` sont les briques de base — vous contrôlez tout. `run_tools()` délègue le raisonnement au modèle — il pense, il agit, il observe, il recommence.

---

## Ce que vous allez apprendre

- **Section 1 — Les backends** : local (GGUF) ou cloud (Anthropic, OpenAI, Ollama), configuration, feature flags, multi-backend
- **Section 2 — L'API** : `chat()`, `complete()`, `stream()` — paramètres, résultats, observabilité des coûts
- **Section 3 — La boucle ReAct** : le concept, `run_tools()` en détail, et comment transformer `file-assistant` en agent autonome capable de gérer n'importe quel fichier sans instruction explicite
