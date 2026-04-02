# Recherche FTS5

La recherche est le cœur du Memory Engine. Savoir mémoriser c'est bien — pouvoir retrouver ce qu'on a mémorisé c'est ce qui rend la mémoire utile.

---

## ctx.memory.search() — la recherche hybride

`search()` effectue une recherche plein texte sur la mémoire épisodique et sémantique simultanément, classée par score BM25 :

```python
results = await ctx.memory.search(
    "rapport trimestriel Acme",   # requête en texte libre
    limit=5,                      # optionnel — défaut: 10, max: 50
    sources=["episodic"],         # optionnel — filtrer par type
    min_importance=0.5,           # optionnel — seuil d'importance minimum
)

for r in results:
    print(f"[{r['score']:.2f}] ({r['source']}) {r['content'][:100]}")
```

**Champs de chaque résultat :**

| Champ | Type | Description |
|---|---|---|
| `content` | `str` | Texte mémorisé |
| `score` | `float` | Score BM25 de pertinence (0.0 → 1.0) |
| `source` | `str` | `"episodic"` ou `"semantic"` |
| `key` | `str \| None` | Clé (mémoire sémantique uniquement) |
| `created_at` | `str \| None` | Horodatage ISO 8601 |
| `metadata` | `dict \| None` | Métadonnées stockées avec l'épisode |

---

## Pourquoi FTS5 — tokenizer unicode61

La recherche FTS5 d'Apollia OS utilise le tokenizer `unicode61`, choisi spécifiquement pour le contexte PME français.

Sans `unicode61`, chercher `"reunion"` ne retrouve pas un épisode contenant `"réunion"`. Chercher `"societe"` rate `"société"`. Dans un contexte professionnel francophone, les accents sont omniprésents — les ignorer dégrade significativement la qualité des recherches.

Avec `unicode61`, la recherche est insensible aux accents :

```python
# Ces requêtes retournent les mêmes résultats
await ctx.memory.search("reunion budget")
await ctx.memory.search("réunion budget")
await ctx.memory.search("Réunion Budget")  # insensible à la casse aussi
```

---

## Opérateurs FTS5

`search()` supporte les opérateurs FTS5 pour des requêtes précises :

```python
# Recherche exacte d'une phrase
await ctx.memory.search('"devis refusé"')

# ET implicite — les deux mots doivent être présents
await ctx.memory.search("devis Acme")

# OU explicite
await ctx.memory.search("devis OR facture")

# Exclusion
await ctx.memory.search("devis NOT brouillon")

# Préfixe — tous les mots commençant par "rapport"
await ctx.memory.search("rapport*")

# Proximité — "devis" et "refusé" dans un rayon de 3 mots
await ctx.memory.search("devis NEAR/3 refusé")
```

---

## Dégradation gracieuse vers l'embedding vectoriel

FTS5 est excellent pour la recherche par mots-clés. Mais il a une limite : il ne comprend pas la sémantique. Chercher `"chiffre d'affaires"` ne retrouve pas un épisode qui parle de `"revenus"` ou `"CA"` si ces mots n'y apparaissent pas.

L'embedding vectoriel résout ce problème — au prix d'un modèle d'IA local.

Apollia OS adopte une stratégie de **dégradation gracieuse** :

```
Niveau 1 (défaut, toujours disponible)
└── FTS5 + BM25
    → Zéro dépendance, très efficace pour les PME

Niveau 2 (opt-in — modèle GGUF de 22 Mo)
└── FTS5 + BM25 + sqlite-vec (all-MiniLM-L6-v2)
    → Recherche sémantique locale, 384 dimensions, multilingue

Niveau 3 (opt-in avancé — Ollama installé)
└── FTS5 + BM25 + Ollama (nomic-embed-text-v1.5)
    → Meilleure qualité sémantique, 768 dimensions
```

L'agent n'a rien à changer dans son code — `ctx.memory.search()` utilise automatiquement le niveau le plus élevé disponible. Configuration dans `apollia.toml` :

```toml
[memory]
embedding_strategy = "auto"       # "fts_only" | "local_gguf" | "ollama" | "auto"
gguf_model_path    = ""           # chemin vers le fichier .gguf si local_gguf
ollama_url         = ""           # URL Ollama si ollama
```

En mode `auto`, le runtime détecte ce qui est disponible au démarrage.

---

## ctx.memory.search() vs l'outil memory_search

Deux façons d'accéder à la recherche mémoire depuis un agent :

**`ctx.memory.search()`** — l'interface haute niveau de `MemoryInterface`. Disponible si `memory_namespace` est défini. Recommandée pour la grande majorité des cas.

**L'outil `memory_search`** — invocable via `ctx.tools.call("memory_search", {...})`. Disponible depuis la boucle ReAct `ctx.llm.run_tools()`, où le LLM décide lui-même quand chercher. Utile quand vous déléguez la décision de recherche au modèle.

```python
# Scénario : laisser le LLM décider quand chercher en mémoire
result = await ctx.llm.run_tools(
    messages=[
        {"role": "system", "content": "Tu peux chercher dans ta mémoire si utile."},
        {"role": "user",   "content": "Que sais-tu sur le budget Acme ?"},
    ],
    tools=[{
        "name": "memory_search",
        "description": "Cherche dans la mémoire persistante de l'agent.",
        "parameters": {
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer"},
            },
            "required": ["query"],
        },
    }],
    max_iterations=3,
)
```

---

## Pattern de recherche contextuelle

Le pattern le plus courant : enrichir chaque tâche avec le contexte mémoriel pertinent avant de traiter.

```python
async def run(self, task, ctx):
    user_input = task["input"]["parts"][0]["text"]

    # 1. Chercher le contexte pertinent AVANT de traiter
    memory_context = ""
    if ctx.memory:
        results = await ctx.memory.search(user_input, limit=3)
        if results:
            memory_context = "\n".join(
                f"- [{r['source']}] {r['content'][:150]}"
                for r in results
            )

    # 2. Traiter avec le contexte
    system_prompt = "Tu es un assistant expert."
    if memory_context:
        system_prompt += f"\n\nContexte mémoriel :\n{memory_context}"

    response = await ctx.llm.chat(system=system_prompt, user=user_input)

    # 3. Mémoriser le résultat APRÈS traitement
    if ctx.memory:
        await ctx.memory.record(
            content=f"Q: {user_input[:100]} → R: {response.content[:200]}",
            importance=0.6,
            task_id=task["task_id"],
        )

    return {
        "task_id": task["task_id"],
        "status": "completed",
        "output": [{"type": "text", "text": response.content}],
    }
```

---

## Recherche depuis la CLI

Pour déboguer ou explorer une mémoire sans passer par un agent :

```bash
# Recherche directe dans un namespace
$ apollia-os memory search file-assistant-memory "rapport financier"
  [0.92] (episodic)  2026-03-15 · Résumé de /data/rapport_Q3.txt généré — 342 lignes
  [0.78] (episodic)  2026-03-10 · Résumé de /data/rapport_Q2.txt généré — 289 lignes
  [0.61] (semantic)  config.summary_length.pdf → "détaillé"

# Statistiques du namespace
$ apollia-os memory inspect file-assistant-memory
  Namespace   : file-assistant-memory
  Fichier     : ~/.apollia/memory/file-assistant-memory.db (1.2 MB)
  Embedding   : fts_only
  Épisodes    : 47 (0 expirés)
  Sémantique  : 3 clés
  Procédures  : 1
```
