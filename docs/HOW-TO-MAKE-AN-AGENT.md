# HOW TO MAKE AN AGENT — Apollia OS

> Guide de référence pour construire un agent ReAct sur Apollia OS.
> Format déclinable : voir aussi HOW-TO-MAKE-A-PIPELINE.md, HOW-TO-MAKE-A-WORKER.md (à venir).
>
> **Cas d'usage de référence de ce guide :**
> `veille-ia-agent` + `web-search-worker` + `synthesis-worker`
> (cf. `agents/assistants/veille-ia-agent.py` et `agents/workers/`)

---

## 0. C'est quoi un agent ReAct sur Apollia OS ?

Un agent **ReAct** (Reason-Act-Observe) est un programme Python qui :
1. Reçoit une tâche
2. **Raisonne** (LLM décide quoi faire)
3. **Agit** (appelle un outil ou délègue via A2A)
4. **Observe** (lit le résultat)
5. Répète jusqu'à produire une réponse finale

Ce n'est **pas** un pipeline déterministe. Le LLM décide à chaque étape. C'est ce qui
distingue un agent Apollia d'un simple script LLM.

**Trois types d'agents :**

| Type | Classe SDK | Usage |
|---|---|---|
| **Director** | `BaseReActAgent` | Orchestre d'autres agents via A2A |
| **Worker** | `WorkerAgent` | Spécialiste d'un domaine, appelé via A2A |
| **Conversationnel** | `ConversationalAgent` | Dialogue multi-tour sans outils |

---

## 1. Définir le problème

Avant d'écrire une ligne de code, répondre à ces questions :

### 1.1 Quelle est la tâche ?

- Qu'est-ce que l'agent doit produire ? (rapport, fichier, action, réponse)
- Quelle est la fréquence ? (à la demande, quotidien, déclenché par événement)
- Qui consomme le résultat ? (humain, autre agent, système externe)

**Exemple (veille-ia-agent) :**
> Produire un rapport Markdown quotidien. Fréquence : lundi-vendredi 7h.
> Consommateur : équipes Apollia (via fichier + notification Discord).

### 1.2 De quel niveau d'autonomie a-t-il besoin ?

- Entièrement autonome (zéro validation humaine) → `tools_requiring_approval = []`
- Autonome sauf actions risquées → `tools_requiring_approval = ["bash_executor"]`
- Validation humaine à chaque étape → HITL (`AIPResult.input_required`)

### 1.3 Checklist des features

| Feature | Besoin ? | Quand oui |
|---|---|---|
| **Mémoire** | ✓/✗ | Déduplication, historique cross-session, context bootstrap |
| **Outils natifs** | ✓/✗ | Accès fichiers, web, shell, notebooks |
| **A2A (délégation)** | ✓/✗ | Tâche complexe divisible en sous-spécialités |
| **Trigger** | ✓/✗ | Exécution planifiée (cron, interval, webhook) |
| **Notifications** | ✓/✗ | Alerter l'utilisateur à la fin ou en cas d'erreur |
| **HITL** | ✓/✗ | Validation humaine avant actions irréversibles |

---

## 2. Choisir l'architecture

### 2.1 Agent solo vs Director + Workers

| Critère | Agent solo | Director + Workers |
|---|---|---|
| Tâche simple, unitaire | ✓ | — |
| Sous-tâches très différentes | — | ✓ |
| Réutilisabilité des sous-tâches | — | ✓ |
| Démo impressive / showcase | — | ✓ |
| Moins de fichiers à maintenir | ✓ | — |

**Règle :** Si l'agent fait plus de 2 choses structurellement différentes (ex : recherche web
+ analyse LLM), séparer en Director + Workers.

### 2.2 Quand créer un Worker ?

Un Worker répond à "oui" à au moins 2 de ces critères :
- Il a des outils très spécifiques que les autres agents n'ont pas besoin
- Il peut être réutilisé par plusieurs directors
- Sa logique est complexe et mérite son propre SYSTEM_PROMPT
- Il peut tourner en parallèle avec d'autres workers

**Exemple :**
```
web-search-worker → tools_required = ["web_search", "web_read"]
synthesis-worker  → tools_required = []  (LLM only)
```

Ces deux workers sont réutilisables par tout agent de recherche.

---

## 3. Concevoir le Manifest

Le manifest déclare **au runtime** ce dont l'agent a besoin. Il est validé
**avant** que `run()` soit appelé — les outils manquants bloquent au démarrage.

```python
def manifest(self) -> dict[str, Any]:
    return {
        # Identité
        "name": "mon-agent",        # kebab-case, unique dans le runtime
        "version": "1.0.0",         # semver
        "description": "...",       # 1-3 phrases
        "agent_type": "assistant",  # "assistant" | "worker" | None

        # Outils : fail-fast vs dégradé
        "tools_required": ["file_write"],   # Absence = agent ne démarre pas
        "tools_optional": ["file_list"],    # Absence = agent démarre en mode DEGRADED
        "tools_requiring_approval": [],     # HITL gate avant exécution

        # Mémoire
        "memory_namespace": "mon-agent",    # Espace isolé en SQLite
        # "shared_memory_namespaces": ["autre-agent"],  # Lecture seule d'autres namespaces

        # A2A
        "supports_a2a": True,       # Peut déléguer ET être délégué
        "skills": [...],            # Déclaration des skills exposés (workers)

        # Budget
        "step_budget": {
            "max_steps": 20,
            "max_tool_calls": 15,
            "wall_clock_secs": 600,
        },
    }
```

### 3.1 required vs optional : la règle

**`tools_required`** : l'outil est indispensable au cœur de la mission.
Sans lui, l'agent ne peut pas remplir son contrat. → fail-fast.

**`tools_optional`** : l'outil améliore l'agent mais son absence est tolérable.
L'agent continue en mode dégradé (état `DEGRADED`).

```python
# Exemple veille-ia-agent :
"tools_required": ["file_write"],         # Sans file_write, pas de rapport
"tools_optional": ["file_list",           # Pratique mais pas indispensable
                   "a2a:search-and-extract",  # Workers optionnels → rapport minimal
                   "a2a:synthesize-report"],
```

### 3.2 Déclarer des skills (Workers uniquement)

```python
"skills": [
    {
        "id": "search-and-extract",      # ID unique dans le runtime
        "name": "Rechercher et extraire",
        "description": "...",            # Utilisé pour la découverte par les directors
        "input_modes": ["text"],
        "output_modes": ["text"],
        "input_schema": {                # Optionnel, pour la validation
            "queries": {"type": "array", "required": True},
        },
    }
],
```

---

## 4. Concevoir la mémoire

### 4.1 Les trois types de mémoire

| Type | API | Usage |
|---|---|---|
| **Sémantique** | `remember(key, value)` / `recall(key)` | Données persistantes structurées |
| **Épisodique** | `record(content, importance)` | Journal d'événements horodatés |
| **Recherche FTS** | `search(query)` | Retrouver des souvenirs par contenu |

### 4.2 Pattern de déduplication (cross-session)

```python
# Encoder une URL vue → stocker en mémoire sémantique
import hashlib
url_hash = hashlib.sha256(url.encode()).hexdigest()[:12]
await ctx.memory.remember(f"seen:{url_hash}", json.dumps({
    "title": title, "url": url, "date_seen": today
}))

# Au prochain run : récupérer tous les hashes vus
results = await ctx.memory.search("seen:", limit=500)
seen_hashes = [r["content"].split(":")[1][:12] for r in results]
```

### 4.3 Pattern de Bootstrap (contexte cross-session)

Le bootstrap évite de redécouvrir le contexte à chaque session.
Stocker une fois, rafraîchir périodiquement (TTL).

```python
# Vérifier si le bootstrap est nécessaire
async def _needs_bootstrap(ctx) -> bool:
    status = await ctx.memory.recall("bootstrap.status")
    if status != "complete":
        return True
    meta_raw = await ctx.memory.recall("bootstrap.meta")
    meta = json.loads(meta_raw or "{}")
    created = datetime.fromisoformat(meta.get("created_at", "2000-01-01"))
    return (datetime.now() - created).days > 7  # TTL 7 jours

# Persister le snapshot
async def _run_bootstrap(ctx) -> dict:
    snapshot = {"key": "value", ...}
    await ctx.memory.remember("bootstrap.snapshot", json.dumps(snapshot))
    await ctx.memory.remember("bootstrap.meta",
        json.dumps({"created_at": datetime.now().isoformat()}))
    await ctx.memory.remember("bootstrap.status", "complete")
    return snapshot

# Dans run() :
if await _needs_bootstrap(ctx):
    snapshot = await _run_bootstrap(ctx)
else:
    raw = await ctx.memory.recall("bootstrap.snapshot")
    snapshot = json.loads(raw) if raw else DEFAULT_SNAPSHOT
```

**Conventions de nommage des clés mémoire :**
- `bootstrap.*` — Contexte initial de l'agent
- `seen:{hash}` — Déduplication d'éléments vus
- `last_*` — Dernière valeur d'une métrique (ex: `last_run_date`)
- `total_*` — Compteur cumulatif (ex: `total_runs`)

### 4.4 Quand NE PAS utiliser la mémoire

- Pour les données qui changent à chaque run et n'ont pas besoin d'être rappelées
- Pour les données déjà accessibles dans le code (constantes, config)
- Principe #6 Apollia : **l'agent décide quand utiliser la mémoire** — jamais d'injection automatique

---

## 5. Écrire le System Prompt

Le system prompt encode l'expertise de l'agent. Sur un modèle 7B+, il doit être
**exhaustif** (le modèle ne connaît pas les règles implicites du domaine).

### 5.1 Structure recommandée

```
[NOM DE L'AGENT] — [rôle en une ligne]

## RÔLE
[Ce que l'agent fait exactement — 2-3 phrases]

## RÈGLES ABSOLUES
[Non-négociables — numérotées, avec RAISON pour chaque règle]
1. TOUJOURS faire X avant Y. RAISON : ...
2. Ne jamais Z. RAISON : ...

## PATTERNS OBLIGATOIRES
[Séquences d'actions que le LLM doit suivre]

## GESTION DES ERREURS
[Quoi faire en cas d'échec de chaque outil]

## FORMAT DE RÉPONSE
[Format exact attendu pour final_answer]

## LANGUE
[Français ? Anglais ? Dépend de l'input ?]
```

### 5.2 Bonnes pratiques

- Chaque règle a une **raison** (le LLM comprend mieux pourquoi respecter une règle expliquée)
- Les **patterns obligatoires** doivent être des pseudo-code, pas des instructions vagues
- Le **format de réponse** doit être un exemple concret, pas une description abstraite

---

## 6. La boucle ReAct

### 6.1 Comment ça marche

```
1. ctx.llm.complete(messages)          ← REASON : le LLM produit un JSON action
2. parse_json_action(response)         ← valider la structure
3. if action == "final_answer"         ← retourner AIPResult.completed(text)
4. if tool needs HITL                  ← retourner AIPResult.input_required(...)
5. ctx.tools.call(tool_name, args)     ← ACT : exécuter l'outil
6. messages.append(observation)        ← OBSERVE : ajouter le résultat
7. Répéter jusqu'à final_answer ou MAX_STEPS
```

### 6.2 Implémenter run()

```python
async def run(self, task: dict[str, Any], ctx: Any) -> dict[str, Any]:
    if ctx.llm is None:
        return AIPResult.failed("NO_LLM", "Aucun backend LLM configuré")

    # Extraire le message utilisateur
    user_message = task.get("input", {}).get("text", "")
    if not user_message:
        return AIPResult.failed("NO_INPUT", "Aucun input dans la tâche")

    # Préparer le contexte (bootstrap, mémoire)
    # ...

    # Lancer la boucle ReAct
    result = await self.react(task, ctx, user_message)

    # react() retourne str (final_answer) ou dict (AIPResult.failed/input_required)
    if isinstance(result, dict):
        return result
    return AIPResult.completed(result)
```

### 6.3 Déléguer via A2A (Director → Worker)

```python
# Invoquer un worker par son skill ID
result = await ctx.a2a_invoke(
    skill_id="search-and-extract",
    input={
        "queries": tech_queries,
        "axis": "tech",
        "seen_hashes": seen_hashes,
    },
    timeout_secs=120,
)
# result["result"] → dict retourné par le worker
# result["agent_name"] → "web-search-worker"
# result["duration_ms"] → int
```

**Erreurs A2A à gérer :**
- `SkillNotFound` : worker non démarré → continuer sans lui
- `Timeout` : worker trop lent → continuer avec résultat partiel
- `WorkerFailed` : erreur dans le worker → logger + continuer

### 6.4 AIPResult — les trois états

```python
# Succès
return AIPResult.completed("Texte ou dict de résultat")

# Erreur
return AIPResult.failed("CODE_ERREUR", "Message humain", details={...})

# Pause HITL (attente validation humaine)
return AIPResult.input_required(
    prompt="L'agent veut supprimer 15 fichiers. Confirmer ?",
    context={"tool": "bash_executor", "args": {"command": "rm -rf ..."}}
)
```

---

## 7. Les outils natifs

### 7.1 Outils disponibles

| Outil | Usage | Input clés |
|---|---|---|
| `web_search` | Recherche web (DuckDuckGo) | `query`, `max_results` |
| `web_read` | Extraction HTML → texte | `url` |
| `file_read` | Lire un fichier | `path`, `offset`, `limit` |
| `file_write` | Écrire un fichier | `path`, `content` |
| `file_edit` | Modifier en place | `path`, `old_text`, `new_text` |
| `file_list` | Lister un répertoire | `path`, `recursive` |
| `file_grep` | Chercher dans les fichiers | `pattern`, `path` |
| `bash_executor` | Shell sandboxé | `command`, `timeout_secs` |
| `python_executor` | Python isolé | `code`, `timeout_secs` |
| `ask_user` | Gate HITL | `prompt`, `context` |
| `memory_search` | FTS sur mémoire | `query`, `limit` |

### 7.2 Patterns importants

**Toujours lire avant d'écrire :**
```python
# Dans le system prompt, toujours écrire :
# "1. file_read AVANT file_write — sans exception."
```

**Gérer les erreurs réseau :**
```python
# web_read peut échouer → le LLM doit continuer avec les articles restants
# "Si web_read échoue pour une URL : continuer avec l'article suivant"
```

**web_search requiert [tools.web] dans apollia.toml :**
```toml
[tools.web]
enabled = true
ssrf_guard = true
```

---

## 8. Tester sans le runtime

Le SDK inclut des mocks pour tester en unitaire sans le runtime Rust.

```python
import pytest
from apollia.testing import MockContext
from apollia.agents import AIPResult

@pytest.mark.asyncio
async def test_agent_run():
    from agents.workers.web_search_worker import WebSearchWorker
    agent = WebSearchWorker()

    ctx = MockContext.create(
        tools={
            "web_search": {"results": [{"title": "Test", "url": "https://test.com"}]},
            "web_read": {"content": "Article content here"},
        },
        llm_response=json.dumps({
            "thought": "Found 1 article",
            "action": "final_answer",
            "text": json.dumps({"articles": [...], "total_found": 1})
        }),
    )

    task = {"input": {"queries": ["LLM news"], "axis": "tech", "seen_hashes": []}}
    result = await agent.run(task, ctx)

    assert result["status"] == "completed"
```

---

## 9. Configurer le déclenchement

### 9.1 Trigger cron dans apollia.toml

```toml
[[triggers]]
id = "daily-veille-ia"          # ID unique
agent = "veille-ia-agent"       # Nom de l'agent (= manifest["name"])
enabled = true
on_busy = { skip = {} }         # skip | queue | block

[triggers.source]
type = "cron"
schedule = "0 7 * * 1-5"       # Lun-Ven à 7h

[triggers.input_template]
text = "Génère la veille IA/LLM du jour"
```

### 9.2 Autres types de triggers

```toml
# Toutes les 30 minutes
[triggers.source]
type = "interval"
every = "30m"

# Quand un fichier est créé
[triggers.source]
type = "file_watch"
path = "/data/uploads"
events = ["create"]

# Via HTTP POST (webhook externe)
[triggers.source]
type = "webhook"
secret = "shared-hmac-secret"
```

### 9.3 Notifications (via l'UI produit)

Les canaux de notification sont configurés dans la base de données via l'interface
Apollia — **pas dans apollia.toml**.

```bash
# CLI équivalent :
apollia notification channel add desktop
apollia notification channel add discord --webhook-url "https://discord.com/api/webhooks/..."

# Vérifier
apollia notification channel list
```

Le runtime envoie automatiquement une notification sur `task.completed` avec le
résultat de `AIPResult.completed(...)`.

---

## 10. Checklist de lancement

Avant de déclarer un agent "production-ready" :

- [ ] `manifest()` complet (name, version, description, tools, skills si worker)
- [ ] `run()` gère le cas `ctx.llm is None`
- [ ] `run()` gère le cas `ctx.tools is None` (graceful degradation)
- [ ] `run()` gère le cas `input.text` vide
- [ ] Toutes les erreurs retournent `AIPResult.failed(...)` (jamais d'exception non catchée)
- [ ] `agent = MyAgent()` en bas du fichier (instance module-level obligatoire)
- [ ] Tests unitaires passent (`pytest -p no:cacheprovider`)
- [ ] Si mémoire : bootstrap testé sur run vierge ET run avec snapshot existant
- [ ] Si A2A : workers déclarés dans `tools_optional` avec préfixe `a2a:`
- [ ] Si trigger : entrée dans `apollia.toml` avec `on_busy` configuré
- [ ] Si notifications : canal configuré via l'UI Apollia

---

## Annexe A : Template agent solo minimal

```python
"""mon-agent — Description en une ligne."""
from __future__ import annotations
from typing import Any
from apollia.agents import AIPResult, BaseReActAgent

SYSTEM_PROMPT = """\
Tu es mon-agent. [Rôle, règles, format de réponse]
"""

def manifest() -> dict[str, Any]:
    return {
        "name": "mon-agent",
        "version": "1.0.0",
        "description": "...",
        "execution_mode": "direct",
        "tools_required": ["file_read"],
        "tools_optional": [],
        "memory_namespace": "mon-agent",
        "supports_a2a": False,
        "step_budget": {"max_steps": 15, "max_tool_calls": 30, "wall_clock_secs": 300},
    }

class MonAgent(BaseReActAgent):
    SYSTEM_PROMPT = SYSTEM_PROMPT
    MAX_STEPS = 15
    TEMPERATURE = 0.2

    def manifest(self): return manifest()

    async def run(self, task, ctx):
        if ctx.llm is None:
            return AIPResult.failed("NO_LLM", "Aucun backend LLM")
        user_message = task.get("input", {}).get("text", "")
        if not user_message:
            return AIPResult.failed("NO_INPUT", "Input vide")
        result = await self.react(task, ctx, user_message)
        if isinstance(result, dict):
            return result
        return AIPResult.completed(result)

agent = MonAgent()
```

---

## Annexe B : Template Director + Workers

```
agents/
├── assistants/
│   └── mon-director.py       ← Hérite de BaseReActAgent
└── workers/
    ├── worker-a.py            ← Hérite de WorkerAgent, skill: "skill-a"
    └── worker-b.py            ← Hérite de WorkerAgent, skill: "skill-b"
```

**Director :** `tools_optional = ["a2a:skill-a", "a2a:skill-b"]`  
**Worker :** `supports_a2a = True`, `skills = [{"id": "skill-a", ...}]`

Dans le system prompt du Director :
```
## ORCHESTRATION
1. Délègue X à a2a:skill-a
2. Délègue Y à a2a:skill-b
3. Fusionne les résultats
4. file_write → rapport
```

---

## Annexe C : Patterns courants

### Déduplication cross-session

```python
# Encoder
hash = hashlib.sha256(url.encode()).hexdigest()[:12]
await ctx.memory.remember(f"seen:{hash}", json.dumps({...}))

# Récupérer
results = await ctx.memory.search("seen:", limit=500)
```

### Retry sur erreur A2A

```python
result = await ctx.a2a_invoke("skill-id", input={...}, timeout_secs=120)
if not result or "error" in result:
    # continuer avec résultat partiel plutôt que bloquer
    articles = []
else:
    articles = result.get("result", {}).get("articles", [])
```

### Rapport de secours si worker indisponible

```python
if not articles:
    return AIPResult.completed({
        "report_markdown": "# Veille\n\n> Aucun article disponible (workers indisponibles).",
        "summary": "Aucun article disponible.",
        "article_count": 0,
    })
```
