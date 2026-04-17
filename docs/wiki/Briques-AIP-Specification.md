# Briques — AIP Specification — Apollia OS

> Spécification complète de l'Agent Interface Protocol : contrat duck typing, types de données, RuntimeContext et exemples fonctionnels.
> Public cible : développeur d'agent Python, contributeur Rust

---

## Vue d'ensemble

L'Agent Interface Protocol (AIP) est le contrat minimal entre un agent Python et le runtime Apollia OS. Sa philosophie tient en une phrase : **un agent est n'importe quel objet Python avec `manifest()` et `async run()`**.

Pas de classe de base obligatoire. Pas de framework à apprendre. Un agent LangGraph, CrewAI, AutoGen ou entièrement custom peut tourner dans Apollia OS avec moins de 10 lignes d'adaptation.

L'AIP définit quatre composants : le `AgentManifest` (identité et capacités), le `ProcessState` (lifecycle du processus), le `AIPTask`/`AIPResult` (contrat de communication), et le `RuntimeContext` (services injectés).

---

## Composant 1 — AgentManifest

La carte d'identité de l'agent. Retournée par `manifest()` sous forme de dict Python ou d'objet sérialisable en JSON. Le runtime la convertit en `AgentManifest` Rust via serde_json à l'état `INITIALIZING`.

### Tous les champs

```python
def manifest(self):
    return {
        # Obligatoires
        "name": "mon-agent",           # str — identifiant unique dans le runtime
        "version": "1.0.0",            # str — semver
        "description": "...",          # str — description humaine

        # Outils (validation fail-fast à INITIALIZING)
        "tools_required": ["file_io"], # list[str] — absent = agent ne démarre pas
        "tools_optional": ["mcp:fs"],  # list[str] — absent = état DEGRADED, pas fatal

        # Mémoire
        "memory_namespace": "mon-ns",  # str | None — None = pas de mémoire persistante
        "shared_memory_namespaces": [], # list[str] — namespaces partagés en lecture

        # Concurrence et budget
        "max_concurrent_tasks": 1,     # int — défaut: 1
        "step_budget": {               # dict | None — None = défauts runtime
            "max_steps": 40,           # int — défaut runtime: 30
            "max_tool_calls": 80,      # int — défaut runtime: 60
            "wall_clock_timeout_secs": 900  # int — défaut runtime: 600
        },

        # Réseau
        "network_allowlist": None,     # list[str] | None — None = pas de réseau

        # Sécurité
        "dangerous_tools_allowed": False,  # bool — défaut: False
        "tools_requiring_approval": [],    # list[str] — outils nécessitant approbation humaine (Mode Orchestré)

        # LLM backend *(Sprint 28, ADR-047)*
        "llm_backend": None,           # str | None — nom d'un backend dans system.db ; None = défaut runtime

        # Protocoles
        "supports_streaming": False,   # bool — SSE si True
        "supports_a2a": False,         # bool — AgentCard A2A si True

        # Dépendances Python *(Worker Agents)*
        "packages": [],                # list[str] — pip packages installés au INITIALIZING
                                       # Syntaxe pip standard : "openpyxl>=3.1.0", "pandas==2.1.4"
                                       # Installés une seule fois via PythonExecutor::setup_venv()

        # Métadonnées
        "tags": ["finance", "crm"],    # list[str]
        "skills": [],                  # list[AgentSkill dict]

        # Rôle sémantique (contrat v2 — utilisé par l'UI pour catégoriser)
        "agent_type": "assistant",     # str | None — "worker" | "assistant" | "system" | None

        # Documentation utilisateur (contrat v2 — optionnel, recommandé pour les assistants)
        "examples": [                  # list[str] — prompts illustrant les usages typiques
            "Crée une spec pour un système d'auth JWT",
            "Quelles sont les specs en attente ?",
        ],
        "limitations": [               # list[str] — ce que l'agent ne fait pas
            "Ne génère jamais de code",
            "Requiert une description fonctionnelle pour démarrer",
        ],
        "setup_notes": "...",          # str | None — prérequis de configuration (None = aucun)
    }
```

### Champs obligatoires vs optionnels

| Champ | Obligatoire | Défaut | Effet si absent |
|---|---|---|---|
| `name` | oui | — | Erreur démarrage |
| `version` | oui | — | Erreur démarrage |
| `description` | oui | — | Erreur démarrage |
| `tools_required` | oui | `[]` | Erreur démarrage |
| `tools_optional` | non | `[]` | Ignoré |
| `memory_namespace` | non | `None` | `ctx.memory` est `None` |
| `max_concurrent_tasks` | non | `1` | 1 tâche à la fois |
| `step_budget` | non | `None` | Défauts runtime (10 steps, 20 calls, 300s) |
| `dangerous_tools_allowed` | non | `False` | Outils dangereux bloqués |
| `tools_requiring_approval` | non | `[]` | Aucun outil ne nécessite d'approbation |
| `supports_a2a` | non | `False` | Pas de AgentCard A2A |
| `llm_backend` | non | `None` | Backend LLM par défaut du runtime |
| `packages` | non | `[]` | Aucune dépendance pip — venv Python standard |
| `agent_type` | non | `None` | Catégorie inconnue — l'UI affiche une entrée neutre |
| `examples` | non | `[]` | Aucun exemple — section quick-start masquée dans l'UI |
| `limitations` | non | `[]` | Aucune limitation déclarée — section masquée dans l'UI |
| `setup_notes` | non | `None` | Aucun prérequis — section masquée dans l'UI |

### tools_requiring_approval (Sprint 11)

Liste les outils dont l'exécution doit être approuvée par un humain avant d'être lancée par ORIA en **Mode Orchestré uniquement**. Lorsqu'un step planifié utilise un outil figurant dans cette liste, ORIA suspend la tâche avec `status = input_required` avant d'appeler l'outil, et attend une décision humaine explicite.

```python
def manifest(self):
    return {
        "name": "devis-agent",
        "version": "2.0.0",
        "description": "Génère et envoie des devis",
        "tools_required": ["file_io", "smtp"],
        "execution_mode": "orchestrated",
        "system_prompt": "Tu es un agent de devis...",

        # L'outil smtp nécessite une confirmation humaine avant chaque envoi.
        "tools_requiring_approval": ["smtp"],
    }
```

Règles d'application :
- N'a d'effet qu'en `execution_mode: "orchestrated"`. En mode `direct` ou `auto`, ce champ est ignoré.
- Une liste vide (défaut) signifie qu'aucun outil ne nécessite d'approbation.
- L'outil doit également figurer dans `tools_required` ou `tools_optional` pour être résolu par le runtime.

### examples, limitations, setup_notes — Documentation utilisateur (contrat v2)

Trois champs optionnels permettant aux développeurs d'agents de documenter leur agent directement dans le manifest, sans fichier externe susceptible de dériver de l'implémentation. L'UI Apollia les affiche dans le panneau détail de l'agent.

**Philosophie de conception :** aucun framework existant (MCP, LangChain, OpenAI Assistants, CrewAI) ne propose ces trois champs de manière structurée — ils utilisent soit un champ `description` libre, soit des fichiers de documentation séparés. Apollia adopte une approche déclarative et colocalisée avec le code.

| Champ | Type | Rôle UI |
|---|---|---|
| `examples` | `list[str]` | Quick-start chips cliquables — masqués si liste vide |
| `limitations` | `list[str]` | Panneau détail — masqués si liste vide |
| `setup_notes` | `str \| None` | Bannière de configuration — masquée si `None` |

**Guidelines rédactionnelles :**

```python
"examples": [
    # 2 à 5 entrées. Formulées comme de vraies requêtes utilisateur.
    # Concrètes et spécifiques — éviter les formulations génériques.
    "Crée une spec pour un système d'auth JWT avec refresh tokens",
    "Quelles sont les specs en attente dans ce projet ?",
],

"limitations": [
    # 2 à 4 entrées. Formulées à l'infinitif ou à la première personne.
    # Uniquement les limites non-évidentes — pas "ne peut pas voler".
    "Ne génère jamais de code — uniquement des specs structurées",
    "Requiert une description fonctionnelle pour démarrer",
],

"setup_notes": (
    # Un seul paragraphe court (3-4 lignes max).
    # Répond à : qu'est-ce que l'utilisateur doit configurer avant de démarrer ?
    # None si l'agent démarre sans aucune configuration.
    "Fonctionne mieux avec un fichier CLAUDE.md dans le workspace. "
    "Sans ce fichier, l'assistant pose des questions de clarification."
),
```

**Recommandation :** ces champs sont optionnels mais fortement recommandés pour les agents `agent_type: "assistant"`. Les workers peuvent les laisser vides — leur interface est suffisamment simple.

### agent_type — Rôle sémantique (contrat v2)

Distingue le rôle d'un agent dans le système. Ce champ est utilisé par l'UI pour catégoriser les agents, indépendamment de `supports_a2a` (qui est `true` pour les deux populations).

| Valeur | Rôle | Caractéristiques |
|---|---|---|
| `"worker"` | Agent opérationnel | Appelé par des agents via A2A, stateless, `execution_mode: "direct"` |
| `"assistant"` | Interlocuteur humain | Multi-tour, orchestre des workers, `execution_mode: "auto"` |
| `"system"` | Infrastructure interne | Onboarding, supervision — non exposé dans l'UI principale |
| `None` | Non déclaré | Agents antérieurs au contrat v2 — l'UI affiche une entrée neutre |

**Pourquoi ne pas utiliser `supports_a2a` ?** Les workers reçoivent des appels A2A (ils sont la cible). Les assistants émettent des appels A2A (ils sont l'émetteur). Les deux ont donc `supports_a2a: true`, ce qui ne permet pas de les distinguer.

**Pourquoi ne pas utiliser `execution_mode` ?** `execution_mode` est une instruction interne à ORIA (comment le réacteur exécute l'agent). `agent_type` est une information de découverte pour l'UI et le routage. Ce sont deux préoccupations orthogonales.

```python
# Worker — appelé par des agents
def manifest(self):
    return {
        "execution_mode": "direct",
        "agent_type": "worker",
        "supports_a2a": True,
        ...
    }

# Assistant — interlocuteur humain
def manifest(self):
    return {
        "execution_mode": "auto",
        "agent_type": "assistant",
        "supports_a2a": True,  # émet des appels A2A vers des workers
        ...
    }
```

### packages — Dépendances pip (Worker Agents)

Liste les paquets pip à installer dans le venv Python isolé de l'agent. Le runtime les installe une seule fois à l'état `INITIALIZING` via `PythonExecutor::setup_venv()`. Si un paquet manque ou échoue à l'installation, l'agent passe en `STOPPED`.

```python
def manifest(self):
    return {
        "name": "excel-agent",
        "version": "1.0.0",
        "description": "Analyse des fichiers Excel",
        "tools_required": ["file_io"],

        # Dépendances pip — syntaxe pip standard
        "packages": [
            "openpyxl>=3.1.0",
            "pandas==2.1.4",
            "requests",
        ],
    }
```

Règles d'application :
- Syntaxe pip standard (`"nom"`, `"nom>=version"`, `"nom==version"`) — toute contrainte pip acceptée.
- Installés dans un venv Python **isolé par agent** — pas de conflit entre agents.
- Liste vide (défaut) : aucune installation, démarrage immédiat.
- Principalement utilisé par les **Worker Agents** déclarés dans le registre communautaire.

### Structure AgentSkill

Utilisée si `supports_a2a: True` pour construire automatiquement la AgentCard :

```python
{
    "id": "generate-quote",
    "name": "Génération de devis",
    "description": "Génère un devis PDF à partir d'un brief client",
    "input_modes": ["text", "data"],
    "output_modes": ["file", "text"]
}
```

---

## Composant 2 — AIPTask

Ce que le runtime envoie à l'agent via `run(task, ctx)`. En Python, `task` est un dict JSON.

```python
async def run(self, task, ctx):
    # Champs de base
    task_id    = task["task_id"]           # str — UUID généré par le runtime
    context_id = task["context_id"]        # str — groupe de tâches liées
    parts      = task["input"]["parts"]    # list[dict] — AIPPart
    history    = task.get("history", [])   # list[dict] — messages précédents
    timeout    = task.get("timeout_seconds")  # int | None

    # Champs HITL — Human-in-the-Loop (Sprint 11)
    is_resumed     = task["is_resumed"]        # bool — True si reprise après approbation
    input_response = task["input_response"]    # InputResponse | None — None au premier appel
```

### Champs HITL — is_resumed et input_response (Sprint 11)

Ces deux champs permettent à un agent de distinguer un premier appel d'une reprise après décision humaine.

| Champ | Type Python | Valeur initiale | Valeur à la reprise |
|---|---|---|---|
| `task["is_resumed"]` | `bool` | `False` | `True` |
| `task["input_response"]` | `InputResponse \| None` | `None` | Instance `InputResponse` peuplée |

#### Classe InputResponse

Injectée automatiquement dans `run.__globals__` par le bridge Rust (STORY-092). Aucun import requis.

```python
class InputResponse:
    approved:     bool           # True si l'humain a approuvé, False si rejeté
    reason:       str | None     # Raison transmise par l'humain — None si approuvé
    context:      dict           # Contexte JSON sérialisé par l'agent au moment du suspend
    responded_at: str            # Horodatage ISO 8601 de la décision humaine
```

L'attribut `context` est restitué tel quel depuis SQLite — il contient exactement ce que l'agent avait passé à `AIPResult.input_required(prompt, context)` lors de la suspension.

### Structure AIPPart

Les parties sont polymorphes via le champ `type` :

```python
# TextPart
{"type": "text", "text": "Générer un devis pour 10 licences Figma"}

# DataPart
{"type": "data", "data": {"client": "Acme", "budget": 5000}}

# FilePart
{"type": "file", "name": "brief.pdf", "mime_type": "application/pdf",
 "data": "<base64>", "uri": None}
```

### Accéder au contenu

```python
async def run(self, task, ctx):
    parts = task["input"]["parts"]

    # Texte brut
    text_parts = [p["text"] for p in parts if p["type"] == "text"]
    user_input = text_parts[0] if text_parts else ""

    # Données structurées
    data_parts = [p["data"] for p in parts if p["type"] == "data"]
    structured = data_parts[0] if data_parts else {}
```

---

## Composant 3 — AIPResult

Ce que l'agent retourne. Peut être un dict Python ou l'une des classes factory injectées par le bridge.

### Classe AIPResult — factory methods (Sprint 11)

La classe `AIPResult` est injectée automatiquement dans `run.__globals__` par le bridge Rust (STORY-092). **Aucun import requis.** Elle expose trois factory methods :

```python
# Tâche terminée avec succès
return AIPResult.completed("Devis PDF généré avec succès")

# Tâche échouée avec code et message structurés
return AIPResult.failed("TOOL_ERROR", "L'outil smtp a retourné une erreur 550")

# Suspendre et demander une approbation humaine
return AIPResult.input_required(
    prompt="Confirmer l'envoi du devis à dupont@sa.fr ?",
    context={"amount": 5100, "email": "dupont@sa.fr"}
)
```

#### AIPResult.input_required(prompt, context)

Suspend la tâche et notifie l'utilisateur sur les canaux configurés (Sprint 11, STORY-099).

| Paramètre | Type | Description |
|---|---|---|
| `prompt` | `str` | Question affichée à l'humain pour prendre sa décision |
| `context` | `dict` | Données JSON que l'agent souhaite récupérer à la reprise |

Le runtime :
1. Persiste `prompt` et `context` dans SQLite (STORY-094)
2. Passe la tâche en `status = input_required`
3. Notifie l'utilisateur (canaux configurés — STORY-099)
4. À la reprise, restitue `context` dans `task["input_response"].context`

### Format dict (compatible rétrograde)

Il reste possible de retourner un dict Python brut :

```python
# Résultat minimal
return {
    "task_id": task["task_id"],      # str — obligatoire
    "status": "completed",           # str — voir TaskStatus
    "output": [                      # list[AIPPart] — résultat
        {"type": "text", "text": "Résultat..."}
    ],
}

# Avec erreur
return {
    "task_id": task["task_id"],
    "status": "failed",
    "error": {
        "code": "INVALID_INPUT",
        "message": "Le champ 'client' est requis"
    }
}

# Human-in-the-loop (format bas niveau — préférer AIPResult.input_required())
return {
    "status": "input_required",
    "output": [],
    "input_required_data": {
        "prompt": "Quel budget maximum pour ce devis ?",
        "context": {"client": "Acme"}
    }
}
```

### Valeurs TaskStatus

| Valeur | Signification |
|---|---|
| `"completed"` | Tâche terminée avec succès |
| `"failed"` | Erreur non récupérable |
| `"input_required"` | Tâche suspendue, attente d'une décision humaine |
| `"canceled"` | Annulée par le runtime ou l'opérateur |

---

## Composant 4 — RuntimeContext

Le deuxième argument de `run()`. Injecté par le runtime. Donne accès à tous les services.

```python
async def run(self, task, ctx):
    # ctx.tools — ToolProxy (toujours disponible)
    result = await ctx.tools.call("file_io", {"action": "list", "path": "."})

    # ctx.memory — MemoryInterface | None (None si pas de memory_namespace)
    if ctx.memory:
        await ctx.memory.record("Tâche reçue", importance=0.5,
                                task_id=task["task_id"])

    # ctx.log — logs structurés via le runtime
    ctx.log.info("processing_task", task_id=task["task_id"])

    # ctx.step_budget — lecture seule (StepBudgetView)
    remaining = ctx.step_budget.steps_remaining
    if remaining < 3:
        ctx.log.warn("budget_low", steps_remaining=remaining)
```

### ctx.tools — ToolProxy

```python
# Appeler un outil
result = await ctx.tools.call("bash_executor", {"command": "ls -la /tmp"})
# result est un dict Python issu du JSON retourné par l'outil

# Lister les outils disponibles pour cet agent
available = ctx.tools.list_tools()  # list[str]

# Compter les appels (pour diagnostiquer)
count = ctx.tools.tool_call_count()  # int
```

### ctx.memory — MemoryInterface

Disponible uniquement si `memory_namespace` est défini dans le manifest. `None` sinon.

```python
if ctx.memory:
    # Stocker un épisode (mémoire épisodique)
    await ctx.memory.record(
        "Client Acme a demandé 10 licences Figma",
        importance=0.7,
        task_id=task["task_id"],
        metadata={"client": "Acme", "product": "Figma"}
    )

    # Stocker un fait (mémoire sémantique)
    await ctx.memory.remember(
        "Le budget max d'Acme est 5000€",
        confidence=0.9,
        source=task["task_id"]
    )

    # Récupérer un fait précis
    facts = await ctx.memory.recall("budget max Acme")

    # Recherche full-text (FTS5 + BM25)
    results = await ctx.memory.search("licences Figma", limit=5)
    # results : list[dict] avec clé "content" et "score"

    # Supprimer un enregistrement
    await ctx.memory.forget(memory_id)
```

### ctx.tools.describe() — Introspection d'outils *(Sprint 20)*

```python
# Obtenir le schéma complet d'un outil
schema = await ctx.tools.describe("bash_executor")
# schema : dict | None
# Contient : name, version, description, kind, input_schema, output_schema, permissions
if schema:
    input_fields = schema["input_schema"]["properties"]
```

### ctx.send() / ctx.receive() — Messagerie inter-agents *(Sprint 20)*

```python
# Envoyer un message à un autre agent
await ctx.send("agent-b", {"type": "data", "content": "résultat partiel"})

# Recevoir un message (avec timeout)
msg = await ctx.receive(timeout=5.0)
# msg : dict | None — contient "from", "payload", "sent_at"
if msg:
    data = msg["payload"]
```

**Contraintes :**
- Max 100 messages en file par agent (au-delà : `MailboxError::QueueFull`)
- Les messages sont des `serde_json::Value` (JSON arbitraire)
- L'agent destinataire doit être démarré (pas de persistance hors-mémoire)
- L'`AgentMailbox` est un acteur Tokio séparé du `TaskRouter`

### ctx.delegate() — Délégation A2A *(Sprint 32)*

Délègue une tâche à un Worker Agent identifié par son `skill_id`. Méthode A2A de bas niveau — expose directement la fonction `A2aDelegateFn` injectée par le runtime.

```python
# Déléguer une tâche à un Worker Agent
result = await ctx.delegate(
    skill_id="generate-quote",              # str — ID de la compétence du worker
    payload={"client": "Acme", "amount": 5000},  # dict — données d'entrée JSON
    timeout_secs=120                        # int | None — défaut: 120s
)
# result : dict avec les clés task_id, agent_name, output
task_id    = result["task_id"]    # str — UUID de la tâche déléguée
agent_name = result["agent_name"] # str — nom de l'agent qui a exécuté
output     = result["output"]     # list[dict] — AIPPart[] — résultat de la tâche
```

| Paramètre | Type | Obligatoire | Défaut | Description |
|---|---|---|---|---|
| `skill_id` | `str` | oui | — | Identifiant de la compétence du Worker Agent cible |
| `payload` | `dict` | oui | — | Données d'entrée JSON sérialisables |
| `timeout_secs` | `int \| None` | non | `120` | Timeout en secondes |

**Prérequis :**
- `supports_a2a: True` dans le manifest (sinon `RuntimeError`)
- La fonction de délégation A2A doit être disponible dans le contexte d'exécution (injectée uniquement pour les Director Agents en Mode Orchestré)

**Erreurs :**
- `RuntimeError: "A2A delegation requires supports_a2a: true in manifest"` — manifest incorrect
- `RuntimeError: "A2A delegation not available in this runtime context"` — contexte non-orchestré

### ctx.user_context — Contexte utilisateur global *(Sprint 28)*

Propriété (pas une méthode) qui expose les entrées de mémoire utilisateur injectées en **mode chat uniquement**. `None` en mode task.

```python
async def run(self, task, ctx):
    uc = ctx.user_context
    if uc is not None:
        # uc : dict[str, list[tuple[str, str]]]
        # Catégories : "preferences", "habits", "context"
        prefs  = uc.get("preferences", [])  # list[tuple[str, str]]
        habits = uc.get("habits", [])
        ctxts  = uc.get("context", [])

        for key, value in prefs:
            ctx.log.info("user_pref", key=key, value=value)
```

**Contenu :**
- `preferences` : préférences explicites de l'utilisateur (ex: `("langue", "français")`)
- `habits` : habitudes détectées (ex: `("format_réponse", "bullet points")`)
- `context` : contexte situationnel (ex: `("projet_courant", "apollia-os")`)

**Règles d'application :**
- Disponible uniquement en **mode chat** (`ChatSession`) — `None` sinon
- Chargé depuis le namespace mémoire `__user__` via `recall_all()` au démarrage de la session
- L'agent décide quoi en faire — jamais d'injection automatique dans le prompt (Principe #6)
- `None` si la mémoire utilisateur est vide ou si le mode ne supporte pas ce contexte

### ctx.step_budget — StepBudgetView

```python
# Lecture seule — l'agent ne peut pas modifier le budget
remaining_steps = ctx.step_budget.steps_remaining      # int
remaining_calls = ctx.step_budget.tool_calls_remaining  # int
elapsed_secs    = ctx.step_budget.elapsed_seconds       # float
```

---

## Agent minimal complet

```python
# minimal_agent.py
class MinimalAgent:
    def manifest(self):
        return {
            "name": "minimal-agent",
            "version": "1.0.0",
            "description": "Agent sans outils ni mémoire",
            "tools_required": [],
        }

    async def run(self, task, ctx):
        parts = task["input"]["parts"]
        text = parts[0]["text"] if parts else ""
        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": f"Reçu : {text}"}],
        }

agent = MinimalAgent()
```

Déployer :

```bash
$ apollia-os agent start ./minimal_agent.py
✓ minimal-agent [ACTIVE]

$ apollia-os run minimal-agent "test"
Reçu : test
```

---

## Agent avec mémoire et outils

```python
# full_agent.py
class FullAgent:
    def manifest(self):
        return {
            "name": "full-agent",
            "version": "1.0.0",
            "description": "Agent avec mémoire et outils fichiers",
            "tools_required": ["file_io"],
            "memory_namespace": "full-agent-memory",
            "max_concurrent_tasks": 2,
        }

    async def run(self, task, ctx):
        user_input = task["input"]["parts"][0]["text"]

        # Lire l'historique mémoriel pertinent
        past = []
        if ctx.memory:
            results = await ctx.memory.search(user_input, limit=3)
            past = [r["content"] for r in results]

        # Appeler un outil
        files = await ctx.tools.call("file_io", {
            "action": "list",
            "path": ".",
            "pattern": "*.py"
        })

        # Mémoriser cette interaction
        if ctx.memory:
            await ctx.memory.record(
                f"Requête : {user_input}",
                importance=0.6,
                task_id=task["task_id"]
            )

        response = f"Fichiers Python : {files.get('files', [])}"
        if past:
            response += f"\n(Contexte mémoriel : {past[0]})"

        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": response}],
        }

agent = FullAgent()
```

---

## Agent avec Human-in-the-Loop (Sprint 11)

Exemple complet d'un agent qui suspend la tâche pour demander confirmation avant d'envoyer un devis.

```python
# devis_agent.py
class DevisAgent:
    def manifest(self):
        return {
            "name": "devis-agent",
            "version": "2.0.0",
            "description": "Génère et envoie des devis avec validation humaine",
            "tools_required": ["file_io", "smtp"],
            "execution_mode": "orchestrated",
            "system_prompt": "Tu es un agent de génération de devis...",
            # L'envoi par email nécessite une approbation humaine.
            "tools_requiring_approval": ["smtp"],
        }

    async def run(self, task, ctx):
        if not task["is_resumed"]:
            # Premier appel — générer le devis, puis demander confirmation
            amount = task["input"]["parts"][0].get("data", {}).get("amount", 0)
            email  = task["input"]["parts"][0].get("data", {}).get("email", "")

            # Suspendre et demander validation à l'humain
            return AIPResult.input_required(
                prompt=f"Confirmer l'envoi du devis à {email} ?",
                context={"amount": amount, "email": email}
            )

        # Reprise — la décision humaine est disponible
        ir = task["input_response"]
        if ir.approved:
            email  = ir.context["email"]
            amount = ir.context["amount"]
            # Envoyer le devis via l'outil smtp
            await ctx.tools.call("smtp", {
                "to": email,
                "subject": f"Votre devis — {amount} €",
                "body": "Veuillez trouver ci-joint votre devis."
            })
            return AIPResult.completed(f"Devis envoyé avec succès à {email}")
        else:
            reason = ir.reason or "refusé"
            return AIPResult.failed("REJECTED", f"Envoi annulé : {reason}")

agent = DevisAgent()
```

Flux d'exécution :

```
apollia-os run devis-agent '{"amount": 5100, "email": "dupont@sa.fr"}'
  → status: input_required
  → Notification envoyée à l'opérateur

apollia-os task resume <task-id> --approve
  → is_resumed=True, input_response.approved=True
  → Devis envoyé avec succès à dupont@sa.fr
  → status: completed
```

---

## Types Rust — HITL (Sprint 11)

Les types Rust correspondants sont définis dans `apollia-core/src/result.rs` et `apollia-core/src/task.rs`.

```rust
/// Données portées par AIPResult quand status == InputRequired.
/// Persistées dans SQLite par le runtime (STORY-094).
pub struct InputRequiredData {
    /// Prompt affiché à l'utilisateur pour prendre sa décision.
    pub prompt: String,
    /// Contexte JSON sérialisé par l'agent au moment de la suspension.
    /// Restitué intégralement dans InputResponseData::context à la reprise.
    pub context: serde_json::Value,
}

/// Réponse humaine reçue après une suspension input_required.
/// Injectée dans AIPTask::input_response lors de la reprise (STORY-095).
pub struct InputResponseData {
    /// true si l'utilisateur a approuvé, false si rejeté.
    pub approved: bool,
    /// Raison transmise par l'humain — None si approuvé.
    pub reason: Option<String>,
    /// Contexte JSON sérialisé par l'agent, restitué intégralement.
    pub context: serde_json::Value,
    /// Horodatage ISO 8601 de la décision humaine.
    pub responded_at: String,
}
```

Le factory method côté Rust :

```rust
// Construit un AIPResult::InputRequired avec prompt et context.
// Détecté par le runtime via status == InputRequired.
AIPResult::input_required("Confirmer l'envoi ?", serde_json::json!({"email": "dupont@sa.fr"}))
```

---

## Validation duck typing

Le runtime valide via inspection Python à `INITIALIZING` :

1. L'objet `agent` doit exister au niveau module (`agent = MyAgent()`)
2. `hasattr(agent, 'manifest')` doit être `True`
3. `manifest()` doit retourner un dict JSON-sérialisable avec `name`, `version`, `description`, `tools_required`
4. `hasattr(agent, 'run')` doit être `True`
5. `run` doit être une coroutine async (`asyncio.iscoroutinefunction`)

Si une validation échoue, l'agent s'arrête en `STOPPED` avec un message d'erreur précis.

---

## SDK Python et type stubs

Le SDK Python (`pip install -e ./sdk`) fournit des type stubs PEP 561 pour toutes les classes PyO3 injectées par le runtime :

- `RuntimeContext` — `sdk/apollia/stubs/context.pyi`
- `ToolProxy` — `sdk/apollia/stubs/tools.pyi`
- `LlmProxy` — `sdk/apollia/stubs/llm.pyi`
- `MemoryInterface` — `sdk/apollia/stubs/memory.pyi`

Ces stubs activent l'autocomplete IDE et la validation `mypy` pour les agents Python. Le SDK propose également des classes de base optionnelles (`BaseReActAgent`, `ConversationalAgent`, `OrchestratedAgent`) et une infrastructure de test avec `MockContext`. Voir [Agents SDK Guide](./Agents-SDK-Guide) et [ADR-037](../adr/ADR-037-python-sdk-packaging).

---

## Voir aussi

- [Agents SDK Guide](./Agents-SDK-Guide) — SDK Python complet (classes de base, mocks, scaffolding)
- [Agents Quickstart](./Agents-Quickstart) — démarrer en 5 minutes
- [Agents RuntimeContext Guide](./Agents-RuntimeContext-Guide) — référence complète des services
- [Architecture Vue d'ensemble](./Architecture-Vue-Ensemble) — AIP dans le contexte global
- [ADR-003](../adr/ADR-003-duck-typing-aip) — pourquoi duck typing plutôt que classe de base
- [ADR-014](../adr/ADR-014-bridge-spawn-blocking-asyncio-run) — bridge async Rust → Python
- [ADR-037](../adr/ADR-037-python-sdk-packaging) — packaging Python SDK
