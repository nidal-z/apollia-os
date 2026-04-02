# Namespaces et isolation

Chaque agent a sa propre mémoire — un fichier SQLite dédié que personne d'autre ne peut écrire. C'est l'isolation par namespace : la garantie qu'un agent ne peut pas accidentellement lire ou corrompre les souvenirs d'un autre.

---

## Namespace privé

`memory_namespace` dans le manifest définit le namespace privé de l'agent :

```python
"memory_namespace": "file-assistant-memory"
```

Cela crée (ou ouvre) le fichier `~/.apollia/memory/file-assistant-memory.db`. Toutes les écritures via `ctx.memory` atterrissent dans ce fichier. Aucun autre agent n'y accède.

Vous choisissez le nom librement. Convention recommandée : `<nom-agent>-memory`. Évitez les noms génériques comme `memory` ou `data` — en production, plusieurs agents peuvent tourner sur la même machine.

---

## Namespaces partagés en lecture

Un agent peut lire (jamais écrire) dans les namespaces d'autres agents via `shared_memory_namespaces` :

```python
def manifest(self):
    return {
        "name": "report-director",
        "memory_namespace": "report-director-memory",   # privé : lecture/écriture
        "shared_memory_namespaces": [                   # partagés : lecture seule
            "file-assistant-memory",
            "crm-agent-memory",
        ],
    }
```

Depuis `run()`, la recherche dans les namespaces partagés est transparente :

```python
# Cherche dans le namespace privé ET les namespaces partagés
results = await ctx.memory.search("rapport client Acme")
```

Les résultats indiquent leur namespace d'origine dans les métadonnées. Les écritures via `ctx.memory.record()` ou `ctx.memory.remember()` écrivent **toujours** dans le namespace privé — impossible d'écrire dans un namespace partagé.

Ce mécanisme est la base des architectures multi-agents : un agent coordinateur peut consulter la mémoire de ses workers sans les modifier.

---

## Mémoire utilisateur globale

En plus des namespaces agents, Apollia OS maintient une **mémoire utilisateur globale** dans le namespace réservé `__user__`. Ce namespace stocke les informations sur la personne qui utilise le système — préférences, habitudes, contexte professionnel.

### Ce qu'elle contient

| Catégorie | Exemples |
|---|---|
| `preferences` | `language: français`, `format: markdown`, `tone: concis` |
| `habits` | `working_hours: 9h-18h`, `review_frequency: daily` |
| `context` | `current_project: apollia-os`, `role: CTO` |

### Comment elle se peuple

La mémoire utilisateur se remplit par trois voies :

**L'onboarding** — à la première utilisation, un agent conversationnel (`onboarding-agent`) pose quelques questions pour collecter les informations de base. Ces entrées ont une confiance de 0.9 (déclarées explicitement).

**L'inférence post-chat** — après chaque session de chat avec plus de 4 messages, le runtime analyse la conversation et extrait les nouvelles informations pertinentes (confiance 0.5). Ce traitement est asynchrone et ne bloque pas la fermeture de session.

**Les corrections explicites** — depuis l'interface desktop (Settings > Mes Mémoires), l'utilisateur peut valider, corriger ou supprimer des entrées. Les corrections manuelles ont une confiance de 0.95 — elles ne sont jamais écrasées par l'inférence automatique.

### Accès depuis les agents

En mode chat, `ctx.user_context` expose la mémoire utilisateur :

```python
async def run(self, task, ctx):
    # Disponible seulement en mode chat
    user_ctx = ctx.user_context

    if user_ctx:
        prefs = dict(user_ctx.get("preferences", []))
        lang   = prefs.get("language", "français")
        tone   = prefs.get("tone", "neutre")
        # Adapter le comportement selon les préférences
```

En mode tâche (invoqué via `apollia-os run`), `ctx.user_context` vaut `None`.

### Principe fondamental

> La mémoire utilisateur est une **information**, pas une règle.

Le LLM reçoit le contexte utilisateur comme information supplémentaire dans le system prompt. Il décide quoi en faire — ce n'est jamais un filtre déterministe. Un agent peut choisir d'ignorer le contexte utilisateur si la tâche l'exige. C'est une extension du principe #6 : la mémoire reste à l'initiative de l'agent.

---

## TTL — expiration automatique

Les entrées mémoire peuvent expirer automatiquement. Configuration globale dans `apollia.toml` :

```toml
[memory]
episodic_ttl_days   = 90     # épisodes supprimés après 90 jours
semantic_ttl_days   = null   # faits permanents par défaut
procedural_ttl_days = null   # workflows permanents par défaut
purge_on_startup    = true   # purge async au démarrage (non-bloquant)
```

TTL par entrée individuelle (prioritaire sur la config globale) :

```python
from datetime import timedelta

# Épisode éphémère — expire après 24h
await ctx.memory.record(
    content="Alerte : quota API à 80%",
    importance=0.9,
    expires_in=timedelta(hours=24),
)

# Fait temporaire — expire après 7 jours
await ctx.memory.remember(
    key="campaign.black_friday.active",
    value=True,
    expires_in=timedelta(days=7),
)
```

La purge est **asynchrone et non-bloquante** — elle ne ralentit jamais le démarrage de l'agent.

---

## La CLI de gestion mémoire

Pour inspecter, exporter, importer, et purger la mémoire sans passer par un agent :

```bash
# Inspecter un namespace
$ apollia-os memory inspect file-assistant-memory
  Namespace   : file-assistant-memory
  Fichier     : ~/.apollia/memory/file-assistant-memory.db (2.3 MB)
  Embedding   : fts_only
  Épisodes    : 847  (12 expirés)
  Sémantique  : 234 clés
  Procédures  : 8

# Recherche directe
$ apollia-os memory search file-assistant-memory "rapport Q3"

# Export — souveraineté totale sur vos données
$ apollia-os memory export file-assistant-memory --format json > backup.json

# Import
$ apollia-os memory import file-assistant-memory backup.json
  ✔ 847 entrées importées

# Purge manuelle des entrées expirées
$ apollia-os memory purge file-assistant-memory
  ✔ 12 entrées expirées supprimées

# Lister tous les namespaces
$ apollia-os memory list
  NAMESPACE                  FICHIER                             TAILLE
  file-assistant-memory      ~/.apollia/memory/file-assi…db     2.3 MB
  crm-agent-memory           ~/.apollia/memory/crm-agent…db     14.7 MB
  __user__                   ~/.apollia/memory/user_memory.db   0.1 MB
```

---

## Isolation et concurrence

Chaque namespace est un fichier SQLite distinct. Plusieurs agents qui partagent un namespace en lecture peuvent lire simultanément grâce au mode WAL (Write-Ahead Logging) de SQLite — plusieurs lecteurs, un seul writer, sans blocage.

```
~/.apollia/memory/
├── file-assistant-memory.db    ← namespace privé de file-assistant
├── crm-agent-memory.db         ← namespace privé de crm-agent
├── shared-knowledge.db         ← namespace partagé (lecture par plusieurs agents)
└── user_memory.db              ← namespace __user__ (mémoire utilisateur globale)
```

Chaque fichier est une base SQLite autonome — il peut être copié, exporté, et restauré indépendamment. La souveraineté totale sur les données de l'agent passe aussi par là : vos souvenirs sont dans des fichiers sur votre machine, pas dans un service cloud.
