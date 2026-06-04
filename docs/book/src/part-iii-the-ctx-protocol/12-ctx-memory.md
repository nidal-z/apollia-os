# `ctx.memory`

`ctx.memory` (Protocol `MemoryInterface`) est la mémoire persistante de l'agent. Trois familles : mémoire **épisodique** (événements horodatés), mémoire **sémantique** (paires clé-valeur), mémoire **procédurale** (recettes apprises). Plus une recherche full-text FTS5 sur l'épisodique.

Le stockage est local, dans une base SQLite scopée par `memory_namespace` de l'agent (cf. [chapitre 6](../part-ii-the-decorators/06-agent-decorator.md)). Aucune donnée ne quitte la machine.

Principe directeur (cf. principe #6) : la mémoire est à l'initiative de l'agent. Le runtime n'injecte jamais automatiquement un contexte mémoriel dans le prompt. C'est l'agent qui décide quoi rappeler, quand, et comment l'utiliser.

---

## Mémoire épisodique : `record`, `search`

Un événement horodaté avec une importance entre 0 et 1.

```python
event_id = await ctx.memory.record(
    "Utilisateur a demandé un résumé du rapport Q3.",
    importance=0.7,
    metadata={"file": "report-q3.pdf"},
)
```

Signature :

```python
async def record(
    self,
    content: str,
    *,
    importance: float = 0.5,
    task_id: str | None = None,
    metadata: dict[str, Any] | None = None,
    expires_in: timedelta | None = None,
) -> str:
```

Recherche full-text :

```python
hits = await ctx.memory.search("rapport Q3", limit=5)
# hits : list[dict] avec keys 'content', 'importance', 'timestamp', 'metadata'
```

Le tokenizer FTS5 est configuré `unicode61` (accents-insensible, tolère le français). Pour les recherches sémantiques par embedding, combinez avec `ctx.llm.embed(...)` et stockez le vecteur en `metadata`.

---

## Mémoire sémantique : `remember`, `recall`, `forget`

Une paire clé-valeur stable et déduplicable.

```python
await ctx.memory.remember(
    "user.preferred_language",
    "français",
    source="explicit_question",
    confidence=1.0,
)

lang = await ctx.memory.recall("user.preferred_language")  # str | None
```

Trois variantes de lecture :

- `recall(key) -> str | None` : la valeur brute.
- `recall_entry(key, injection_reason="...") -> dict | None` : l'entrée complète (valeur, source, confidence, timestamp). Le `injection_reason` est tracé pour l'observabilité.
- `recall_all(limit=100, injection_reason="...") -> list[dict]` : toutes les paires, paginées.

Et `forget(key)` pour supprimer.

> Note sur le naming : les clés `user.*` (par exemple `user.preferred_language`) sont par convention le **profil utilisateur** géré par `ctx.profile` (cf. [chapitre 18](18-ctx-other-services.md)). Pour la mémoire propre à l'agent, préfixez par votre domaine (`triage.last_run`, `meeting.last_summary`).

---

## Mémoire procédurale : `learn_procedure`, `recall_procedure`

Pour les recettes répétables : une liste d'étapes associée à un déclencheur en langue naturelle.

```python
await ctx.memory.learn_procedure(
    trigger="quand l'utilisateur dit 'résume ma boîte'",
    steps=[
        "appeler inbox.list_unread",
        "filtrer les emails à pièce jointe",
        "résumer chaque thread en 2 phrases",
        "présenter la liste par priorité décroissante",
    ],
)

procedures = await ctx.memory.recall_procedure(trigger="résume ma boîte")
# procedures : list[dict] avec 'steps', 'last_used', 'usage_count'
```

Cas d'usage : un agent qui apprend les habitudes de l'utilisateur sur la durée. Les étapes sont du texte libre, le LLM les ré-interprète au moment de l'usage.

---

## Recherche FTS5 : pratique

`search(query, limit=10)` est la voie rapide pour retrouver une trace épisodique :

```python
hits = await ctx.memory.search("réunion direction commerciale", limit=5)
for h in hits:
    print(h["timestamp"], h["importance"], h["content"][:80])
```

Le score de pertinence est implicite : les hits sont retournés par ordre de pertinence FTS5 décroissant, et l'importance enregistrée à `record` est un facteur secondaire.

Astuce de recall court : pour un agent conversationnel, recherchez sur le dernier message utilisateur **avant** d'appeler `ctx.llm`. Vous évitez des prompts à zéro contexte sans pour autant pré-charger la base entière.

---

## Export et import

Pour la portabilité et les sauvegardes (cf. ADR-010).

```python
snapshot = await ctx.memory.export()
# snapshot : dict JSON-sérialisable contenant épisodique + sémantique + procédurale

# plus tard, sur une autre machine ou après reset
await ctx.memory.import_data(snapshot)
```

L'import est additif : il ne supprime pas les entrées existantes. Pour repartir d'une base propre, supprimez le fichier SQLite côté runtime avant d'appeler `import_data`.

---

## Quand utiliser quoi

| Besoin | Outil |
|---|---|
| Tracer un événement utilisateur | `record` |
| Stocker un fait stable (préférence, paramètre) | `remember` |
| Retrouver un événement par mots-clés | `search` |
| Lire un fait nommé | `recall` ou `recall_entry` |
| Lister tous les faits | `recall_all` |
| Recette répétable | `learn_procedure` / `recall_procedure` |
| Profil utilisateur (`user.*`) | `ctx.profile` ([chapitre 18](18-ctx-other-services.md)) |
| Sauvegarder / migrer | `export` / `import_data` |

---

## Anti-patterns

**Ne pas** stocker des objets complexes par sérialisation manuelle (`pickle`, `repr`). Le storage est `str`. Utilisez du JSON sérialisé si vraiment besoin, et préférez des entrées atomiques.

**Ne pas** transformer `ctx.memory` en logger d'événements. C'est `ctx.logger` qui sert à tracer ce que fait l'agent. La mémoire sert à se souvenir de ce qui compte pour la prochaine tâche.

**Ne pas** appeler `recall_all()` puis filtrer en Python si vous savez ce que vous cherchez. Utilisez `recall(key)` direct ou `search(query)` avec un terme précis.

**Ne pas** écrire des clés sémantiques au préfixe `user.*` depuis un agent qui n'a pas `user_memory_write=True` dans son manifeste. Les écritures sont rejetées au runtime (cf. [chapitre 18](18-ctx-other-services.md)).

---

## ADRs

- `ADR-010` : Mémoire à l'initiative de l'agent (principe #6)
- `ADR-010` : Memory episodic consolidation
- `ADR-010` : Memory export/import format
- `ADR-010` : Memory namespace project-scoped
- `ADR-011` : User profile redesign

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
