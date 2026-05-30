# Trois types de mémoire

Pour activer la mémoire persistante, ajoutez `memory_namespace` au manifest :

```python
def manifest(self):
    return {
        "name": "file-assistant",
        "version": "1.1.0",
        "description": "Assistant fichier avec mémoire",
        "tools_required": ["file_read", "file_write"],
        "memory_namespace": "file-assistant-memory",  # ← active ctx.memory
        "max_concurrent_tasks": 1,
        "step_budget": 15,
    }
```

Sans ce champ, `ctx.memory` est `None`. Avec ce champ, le runtime ouvre (ou crée) le fichier SQLite `~/.apollia/memory/file-assistant-memory.db` et expose `ctx.memory` dans `run()`.

---

## Working memory — la mémoire temporaire

La working memory n'est pas vraiment un "type" de mémoire — ce sont simplement les variables Python dans le scope de `run()`. Elles existent pendant l'exécution de la tâche et disparaissent quand `run()` retourne.

```python
async def run(self, task, ctx):
    # Working memory : variables locales Python
    file_path = None
    file_content = ""
    summary = ""

    # ... logique de l'agent ...
    # Ces variables sont détruites après return
```

Utilisez la working memory pour les données temporaires — résultats intermédiaires, brouillons, calculs en cours. Aucune configuration requise, aucun coût SQLite.

---

## Mémoire épisodique — les événements

La mémoire épisodique enregistre **ce qui s'est passé** avec un horodatage et un score d'importance.

### Enregistrer un épisode

```python
episode_id = await ctx.memory.record(
    content="Résumé de /data/rapport_Q3.txt généré — 342 lignes, 5 sections",
    importance=0.7,                     # 0.0 à 1.0 — influence le ranking des recherches
    task_id=task["task_id"],            # lie l'épisode à la tâche
    metadata={                          # dict optionnel — enrichissement structuré
        "file_path": "/data/rapport_Q3.txt",
        "summary_path": "/data/rapport_Q3_summary.txt",
        "word_count": 2847,
    }
)
# episode_id : str — identifiant unique de cet épisode
```

**Le score d'importance** guide les recherches futures : un épisode avec `importance=0.9` ressort en tête des recherches même si son texte correspond moins bien au terme cherché qu'un épisode `importance=0.3`. Utilisez des valeurs élevées pour les événements significatifs (erreurs critiques, décisions importantes) et des valeurs faibles pour le bruit de fond.

### Consulter l'historique

```python
# Récupérer les 20 épisodes les plus récents
recent = await ctx.memory.history(limit=20)

# Depuis une date spécifique
from datetime import datetime
recent = await ctx.memory.history(limit=10, since=datetime(2026, 1, 1))

# Chaque entrée :
for ep in recent:
    print(f"{ep['created_at']} — {ep['content'][:80]}")
```

### Extension de file-assistant — éviter de résumer deux fois

```python
async def run(self, task, ctx):
    file_path = self._extract_path(user_text)

    # Vérifier si ce fichier a déjà été résumé récemment
    if ctx.memory:
        past = await ctx.memory.search(
            f"résumé {file_path}",
            sources=["episodic"],
            limit=1,
        )
        if past and past[0]["score"] > 0.8:
            cached = past[0]["content"]
            return {
                "task_id": task["task_id"],
                "status": "completed",
                "output": [{"type": "text", "text": f"(En cache)\n\n{cached}"}],
            }

    # ... résumer normalement ...

    # Mémoriser après génération
    if ctx.memory:
        await ctx.memory.record(
            content=f"Résumé de {file_path} : {summary[:500]}",
            importance=0.6,
            task_id=task["task_id"],
            metadata={"file_path": file_path},
        )
```

### TTL — expiration automatique

```python
from datetime import timedelta

await ctx.memory.record(
    content="Alerte : quota LLM à 80%",
    importance=0.9,
    expires_in=timedelta(hours=24),  # s'efface automatiquement après 24h
)
```

---

## Mémoire sémantique — les faits

La mémoire sémantique stocke des **faits durables** sous forme de paires clé → valeur. Idéale pour les configurations, préférences, et informations métier stables.

### Stocker un fait

```python
await ctx.memory.remember(
    key="client.dupont_sa.budget_max",
    value=15000,
    confidence=1.0,        # 0.0 à 1.0 — fiabilité de l'information
    source=task["task_id"] # traçabilité — qui a créé ce fait ?
)
```

La **confidence** joue le rôle de protection contre les écrasements : un fait avec `confidence=0.9` ne sera pas remplacé par un nouveau fait avec `confidence=0.5`. Les données saisies manuellement par l'utilisateur (`confidence=0.95`) ne sont jamais écrasées par des inférences automatiques (`confidence=0.5`).

### Récupérer un fait

```python
budget = await ctx.memory.recall("client.dupont_sa.budget_max")
# budget : la valeur stockée (Python natif — int, str, dict...) ou None si absent
```

### Mettre à jour et supprimer

```python
# Mettre à jour un fait existant (upsert)
await ctx.memory.remember(
    key="client.dupont_sa.budget_max",
    value=20000,
    confidence=1.0,
)

# Supprimer un fait obsolète
await ctx.memory.forget("client.dupont_sa.old_contact_email")
```

### Exemples de clés sémantiques pour file-assistant

```python
# Préférences de résumé par type de fichier
await ctx.memory.remember("config.summary_length.pdf", "détaillé")
await ctx.memory.remember("config.summary_length.txt", "concis")
await ctx.memory.remember("config.language", "français")

# Puis dans run()
lang = await ctx.memory.recall("config.language") or "français"
length = await ctx.memory.recall(f"config.summary_length.{ext}") or "concis"
```

---

## Mémoire procédurale — les workflows

La mémoire procédurale stocke des **séquences d'actions** associées à un déclencheur. Utile pour les agents qui apprennent à reconnaître et reproduire des workflows efficaces.

### Apprendre une procédure

```python
await ctx.memory.learn_procedure(
    trigger="résumer un rapport financier",
    steps=[
        "Lire les 50 premières lignes pour identifier la structure",
        "Extraire les sections 'Résultats' et 'Recommandations'",
        "Générer un résumé en 5 phrases maximum",
        "Inclure les chiffres clés (revenus, marge, variation)",
    ]
)
```

### Rappeler une procédure

```python
steps = await ctx.memory.recall_procedure("résumer un rapport financier")
# steps : list[dict] — chaque dict contient trigger, steps, created_at
# retourne [] si aucune procédure ne correspond (jamais None)

if steps:
    # steps est une list[dict] — extraire les étapes de la procédure la plus récente
    proc_steps = steps[0]["steps"]   # list[str]
    guidance = "\n".join(f"{i+1}. {s}" for i, s in enumerate(proc_steps))
    response = await ctx.llm.chat(
        system=f"Suis cette procédure :\n{guidance}",
        user=file_content,
    )
```

La mémoire procédurale est particulièrement utile en mode orchestré (chapitre 9), où ORIA peut apprendre des workflows et les réutiliser sans les recoder.

---

## Récapitulatif — quand utiliser quoi

| Situation | Type | Méthode |
|---|---|---|
| Données temporaires dans `run()` | Working | Variables Python locales |
| "J'ai traité ce fichier le 15/03" | Épisodique | `ctx.memory.record` |
| "Le budget d'Acme est 15 000 €" | Sémantique | `ctx.memory.remember` |
| "Pour ce type de tâche, voici les étapes" | Procédurale | `ctx.memory.learn_procedure` |
| "Qu'est-ce qui parle de Dupont SA ?" | Recherche | `ctx.memory.search` |
