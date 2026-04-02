# ctx.llm : chat, complete, stream

`ctx.llm` expose trois méthodes d'appel direct au LLM : `chat()` pour le cas le plus courant, `complete()` pour les conversations multi-tours, et `stream()` pour les réponses longues. La quatrième méthode, `run_tools()`, est couverte dans la section suivante.

---

## chat() — le cas courant

Un system prompt, un message utilisateur, une réponse. C'est 80% des cas.

```python
response = await ctx.llm.chat(
    system="Tu es un assistant expert en synthèse de documents. "
           "Produis un résumé en 5 phrases maximum.",
    user=file_content,
    backend="anthropic",   # optionnel — override du backend par défaut
)
```

**L'objet `response` :**

```python
response.content          # str — le texte généré
response.usage.prompt_tokens      # int — tokens du prompt
response.usage.completion_tokens  # int — tokens de la réponse
response.usage.cost_usd           # float | None — coût USD (None pour les backends locaux)
response.latency_ms               # int — latence en millisecondes
```

**Vérifier le coût avant d'appeler :**

Pour les agents à haute fréquence sur des fichiers volumineux, il peut être utile de tronquer l'entrée pour maîtriser les coûts :

```python
# Limiter le contenu envoyé au LLM
MAX_CHARS = 8000
if len(file_content) > MAX_CHARS:
    file_content_for_llm = file_content[:MAX_CHARS] + "\n\n[... tronqué ...]"
else:
    file_content_for_llm = file_content

response = await ctx.llm.chat(system=SYSTEM_PROMPT, user=file_content_for_llm)
```

---

## complete() — les conversations multi-tours

Quand vous avez besoin de passer un historique de messages ou de construire des échanges complexes :

```python
response = await ctx.llm.complete([
    {"role": "system",    "content": "Tu es un assistant commercial expert en devis."},
    {"role": "user",      "content": "Voici le brief client : " + brief_text},
    {"role": "assistant", "content": "J'ai analysé le brief. Le budget est de 5000€."},
    {"role": "user",      "content": "Génère maintenant le devis détaillé."},
])

print(response.content)
```

`complete()` retourne le même objet `response` que `chat()`. La liste de messages suit la convention `role`/`content` standard.

**Pattern d'historique progressif :**

```python
async def run(self, task, ctx):
    messages = [
        {"role": "system", "content": "Tu es un analyste financier."},
    ]

    # Ajouter le contexte mémoriel si disponible
    if ctx.memory:
        results = await ctx.memory.search("rapport financier", limit=2)
        for r in results:
            messages.append({"role": "system",
                              "content": f"Contexte mémorisé : {r['content']}"})

    # Ajouter la requête utilisateur
    user_input = task["input"]["parts"][0]["text"]
    messages.append({"role": "user", "content": user_input})

    response = await ctx.llm.complete(messages)
    return AIPResult.completed(response.content)
```

---

## stream() — les réponses longues

Pour les réponses qui peuvent prendre plusieurs secondes (rapports détaillés, analyses longues), le streaming permet de retourner les tokens au fur et à mesure :

```python
chunks = await ctx.llm.stream([
    {"role": "system", "content": "Génère un rapport détaillé."},
    {"role": "user",   "content": "Analyse ce fichier de 500 lignes : " + content},
])

# chunks : list[str] — liste de tokens textuels
full_response = "".join(chunks)
```

`stream()` retourne toujours une `list[str]`. Si le backend ne supporte pas le streaming nativement (certains backends locaux), un seul chunk contenant la réponse complète est retourné — le code de l'agent ne change pas.

---

## Choisir un backend spécifique pour un appel

Si plusieurs backends sont configurés, vous pouvez choisir pour un appel individuel :

```python
# Utiliser le backend local pour les résumés rapides
quick_summary = await ctx.llm.chat(
    system="Résume en 2 phrases.",
    user=file_content[:2000],
    backend="local",
)

# Utiliser Claude pour l'analyse approfondie
deep_analysis = await ctx.llm.chat(
    system="Analyse en profondeur les implications financières.",
    user=file_content,
    backend="anthropic",
)
```

`ctx.llm.default_backend` retourne le nom du backend par défaut configuré sur le runtime — utile pour les logs.

---

## Gérer ctx.llm is None

`ctx.llm` est `None` quand aucun backend LLM n'est disponible. Deux comportements sont possibles selon le contexte de votre agent :

**Refuser la tâche** (comportement recommandé si le LLM est indispensable) :

```python
if ctx.llm is None:
    return AIPResult.failed("LLM_UNAVAILABLE",
                            "Ce agent nécessite un backend LLM configuré.")
```

**Mode dégradé** (si le LLM est optionnel) :

```python
if ctx.llm is None:
    # Retourner le contenu brut sans résumé
    return AIPResult.completed(
        f"Contenu de {file_path} (résumé indisponible — LLM non configuré) :\n\n{file_content[:1000]}"
    )
```

Le runtime émet automatiquement un `RuntimeEvent::AgentDegraded` sur l'EventBus quand `ctx.llm` est `None` pour un agent qui en a besoin — visible dans `apollia-os status`.

---

## Récapitulatif de l'API

```python
# Chat simple (80% des cas)
response = await ctx.llm.chat(system="...", user="...")

# Multi-tour
response = await ctx.llm.complete([
    {"role": "system",    "content": "..."},
    {"role": "user",      "content": "..."},
    {"role": "assistant", "content": "..."},
    {"role": "user",      "content": "..."},
])

# Streaming
chunks = await ctx.llm.stream([{"role": "user", "content": "..."}])
full  = "".join(chunks)

# Accéder aux métadonnées
response.content
response.usage.prompt_tokens
response.usage.completion_tokens
response.usage.cost_usd        # None pour les backends locaux
response.latency_ms

# Informations sur le backend actif
ctx.llm.default_backend        # str — nom du backend par défaut
```
