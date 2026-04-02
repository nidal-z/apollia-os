# Mémoire utilisateur

Les sessions de chat ont une durée de vie limitée. Mais certaines informations méritent de persister au-delà : les préférences de l'utilisateur, son contexte métier, ses habitudes de travail. Sans mémoire transversale, l'assistant repose à chaque nouvelle session des questions déjà posées.

La **mémoire utilisateur globale** d'Apollia OS résout ce problème avec un namespace dédié (`__user__`) dans le `MemoryManager`, partagé entre toutes les sessions et tous les agents.

---

## Le namespace `__user__`

Le `MemoryManager` (chapitre 5) organise les données par namespaces — un agent a son propre espace mémoire. Le namespace `__user__` est spécial : il est transversal. Toutes les sessions chat y lisent, et plusieurs sources peuvent y écrire.

```
MemoryManager
  ├── csv-data-worker/   ← mémoire propre à l'agent
  ├── pdf-worker/        ← mémoire propre à l'agent
  └── __user__/          ← mémoire utilisateur globale
        ├── "préfère les réponses en bullet points"
        ├── "travaille principalement sur des données ventes EMEA"
        ├── "utilise Python 3.11, Linux Debian"
        └── "langue préférée : français"
```

---

## Quatre sources de confiance

Chaque entrée mémoire est stockée avec un score de confiance. Ce score influence la probabilité que l'entrée soit injectée dans le system prompt.

| Source | Score | Exemple |
|---|---|---|
| `user_explicit` | 0.95 | L'utilisateur dit "retiens que je préfère..." |
| `onboarding` | 0.90 | Réponses au formulaire d'onboarding initial |
| `chat_inference` | 0.50 | Le LLM infère une préférence depuis la conversation |
| `agent_observation` | 0.50 | Un agent Python observe un pattern comportemental |

`user_explicit` a le score le plus élevé car l'intention est explicite. `chat_inference` et `agent_observation` sont au seuil de fiabilité — l'inférence peut se tromper.

---

## Injection dans le system prompt

À chaque message, le `ChatSessionManager` construit le contexte LLM ainsi :

```
system_prompt (fixe, défini à la création)
  +
mémoire utilisateur (top K entrées par score de confiance)
  +
résumé de l'historique ancien (si fenêtre glissante active)
  +
N derniers messages (fenêtre glissante, défaut : 20)
  +
message courant
```

L'injection mémoire est **non-déterministe** : le LLM décide s'il utilise les informations mémorisées, il n'y est pas contraint. Une préférence mémorisée ne force pas un comportement — elle informe.

---

## Fenêtre glissante et résumé (ADR-039)

Les conversations longues produisent des historiques qui dépassent la fenêtre de contexte du LLM. Apollia OS applique une stratégie à deux niveaux :

### Fenêtre glissante

Seuls les **N derniers messages** (défaut : 20) sont inclus directement dans le contexte. Les messages plus anciens sont "oubliés" du contexte courant.

### Résumé automatique

Quand la fenêtre glisse (le message 21 arrive, le message 1 sort), le runtime génère un **résumé LLM** des messages qui sortent et le stocke dans `chat_sessions.summary`. Ce résumé est inclus dans tous les contextes suivants à la place des messages détaillés.

```
Messages 1-20   → résumé : "L'utilisateur a analysé le fichier ventes.csv,
                             identifié 3 anomalies sur la région EMEA,
                             demandé un export Excel filtré."
Messages 21-40  → résumé mis à jour
Messages 41+    → dans le contexte courant
```

Le résultat : les informations clés des échanges anciens persistent dans le contexte, sans dépasser la fenêtre du LLM.

Pour configurer la fenêtre dans `apollia.toml` :

```toml
[chat]
context_window_messages = 20   # défaut : 20
```

---

## Écrire dans la mémoire utilisateur

### Via l'API (explicite)

```bash
curl -X POST http://localhost:7771/api/v1/memory \
  -H "Content-Type: application/json" \
  -d '{
    "namespace": "__user__",
    "content": "Préfère les réponses structurées avec des titres et bullet points.",
    "source": "user_explicit",
    "confidence": 0.95
  }'
```

### Depuis un agent Python (observation)

```python
async def run(self, task, ctx):
    # L'agent observe un pattern et le mémorise
    await ctx.memory.store(
        namespace="__user__",
        content="Travaille principalement avec des fichiers CSV encodés latin-1.",
        source="agent_observation",
        confidence=0.5,
    )
    # ... suite du traitement
```

### Inférence automatique en session chat

Quand le LLM détecte une préférence dans la conversation (`"réponds toujours en français"`, `"je travaille sur Linux"`), le runtime peut créer automatiquement une entrée `chat_inference`. Ce comportement est configurable :

```toml
[chat]
auto_memory_inference = true   # défaut : true
inference_confidence  = 0.5    # défaut : 0.5
```

---

## Inspecter la mémoire utilisateur

```bash
# Lister toutes les entrées
curl "http://localhost:7771/api/v1/memory?namespace=__user__"

# Recherche sémantique
curl "http://localhost:7771/api/v1/memory/search?namespace=__user__&q=préférences+affichage"
```

```json
[
  {
    "id": "mem-001",
    "namespace": "__user__",
    "content": "Préfère les réponses structurées avec des titres et bullet points.",
    "source": "user_explicit",
    "confidence": 0.95,
    "created_at": "2026-04-01T10:00:00Z"
  },
  {
    "id": "mem-002",
    "namespace": "__user__",
    "content": "Travaille principalement avec des fichiers CSV encodés latin-1.",
    "source": "agent_observation",
    "confidence": 0.5,
    "created_at": "2026-04-02T08:30:00Z"
  }
]
```

---

## Supprimer une entrée

```bash
curl -X DELETE http://localhost:7771/api/v1/memory/mem-001
```

La suppression est immédiate. L'entrée ne sera plus injectée dans les sessions suivantes.

---

## Principe #6 — Mémoire à initiative de l'agent

La mémoire utilisateur respecte le Principe #6 d'Apollia OS : **jamais d'injection automatique de contexte mémoriel sans logique explicite**. Le runtime sélectionne les entrées par score de confiance, mais c'est le LLM qui décide de les utiliser. Il n'y a pas de "forçage" de comportement par la mémoire — seulement une mise à disposition d'informations contextuelles.
