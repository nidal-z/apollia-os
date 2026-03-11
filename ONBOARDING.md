# Onboarding Apollia OS

Guide complet pour lancer, naviguer et tester le projet pour la première fois.

---

## Sommaire

1. [Ce que tu as construit](#1-ce-que-tu-as-construit)
2. [Prérequis](#2-prérequis)
3. [Build](#3-build)
4. [Lancer le runtime](#4-lancer-le-runtime)
5. [Tour des features](#5-tour-des-features)
   - [5.1 Agents Python](#51-agents-python)
   - [5.2 Exécuter des tâches](#52-exécuter-des-tâches)
   - [5.3 Outils natifs](#53-outils-natifs)
   - [5.4 Mémoire](#54-mémoire)
   - [5.5 Audit trail](#55-audit-trail)
   - [5.6 LLM](#56-llm-optionnel)
   - [5.7 HITL — Approbations humaines](#57-hitl--approbations-humaines)
   - [5.8 Triggers — Automatisation](#58-triggers--automatisation)
   - [5.9 Pipelines multi-agent](#59-pipelines-multi-agent)
   - [5.10 Dashboard web](#510-dashboard-web)
   - [5.11 Notifications](#511-notifications)
6. [Démarrer avec apollia-reviewer](#6-démarrer-avec-apollia-reviewer)
7. [Configuration complète](#7-configuration-complète)
8. [Référence rapide CLI](#8-référence-rapide-cli)
9. [Écrire son propre agent](#9-écrire-son-propre-agent)
10. [Commandes just](#10-commandes-just)
11. [Fichiers créés au premier démarrage](#11-fichiers-créés-au-premier-démarrage)
12. [Dépannage](#12-dépannage)

---

## 1. Ce que tu as construit

Apollia OS est un **runtime local pour agents IA autonomes**. Un processus Rust qui tourne en arrière-plan, expose une API HTTP, et exécute des agents Python dans des environnements isolés. Zéro cloud requis.

```
apollia-os start
    └── Supervisor démarre 8 acteurs Tokio en séquence :
         1. EventBus            bus d'événements interne (broadcast)
         2. AgentRegistry       registre des agents déployés
         3. ToolRegistry        3 outils natifs (file_io, bash, python)
         4. LlmRouter           backends LLM (Anthropic / OpenAI / local) — optionnel
         5. TaskRouter          dispatch + contrôle de concurrence
         6. TriggerEngine       cron, file-watch, oneshot, webhook
         7. PipelineEngine      orchestration multi-agent — démarré si [[pipelines]] défini
         8. APIServer           HTTP sur /tmp/apollia.sock + :7771
         +  NotificationEngine  desktop, webhook sortants — démarré si [notifications] défini
```

> **Note — MemoryEngine :** ce n'est pas un acteur Tokio. Il s'instancie à la demande
> lorsqu'un agent accède à `ctx.memory`. Il n'apparaît donc pas dans le résumé de démarrage.

**Stack technique :** Rust + Tokio (async runtime) · PyO3 (bridge Python) · SQLite/FTS5 (persistance) · axum (API HTTP) · clap v4 (CLI)

**11 crates dans le workspace :**

| Crate | Rôle |
|---|---|
| `apollia-core` | Types partagés (AgentManifest, AIPTask, RuntimeEvent…) |
| `apollia-runtime` | Supervisor, AgentRegistry, TaskRouter, EventBus, APIServer |
| `apollia-oria` | ORIA Engine — Observer-Reasoner-Actor, StepBudget, ResilienceLayer |
| `apollia-tools` | Tool Registry + outils natifs |
| `apollia-memory` | Memory Engine SQLite + FTS5 |
| `apollia-aip` | Bridge PyO3 Rust ↔ Python async |
| `apollia-llm` | LLM Router multi-backend |
| `apollia-triggers` | Trigger Engine |
| `apollia-notifications` | Notification Engine |
| `apollia-pipelines` | Pipeline multi-agent (topologie BFS, HITL) |
| `apollia-cli` | Binaire final `apollia-os` |

---

## 2. Prérequis

### Rust

```bash
# Installer rustup si absent
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Vérifier
rustc --version   # doit être >= 1.75
```

### Python 3.11+

```bash
# macOS
brew install python@3.13

# Vérifier
python3 --version   # doit être >= 3.11
```

### macOS — variable PYO3_PYTHON (CRITIQUE)

Sans cette variable, `cargo build` échoue sur macOS.

```bash
# Pour la session courante
export PYO3_PYTHON=/opt/homebrew/bin/python3.13

# Pour toutes les sessions futures (à faire UNE FOIS)
echo 'export PYO3_PYTHON=/opt/homebrew/bin/python3.13' >> ~/.zshrc
source ~/.zshrc
```

### just (optionnel mais recommandé)

```bash
cargo install just
just --version
```

---

## 3. Build

```bash
cd /chemin/vers/apollia-v2

# Build release — 5-10 min Linux, 15-25 min macOS (première fois)
cargo build --workspace --release

# Rendre apollia-os accessible depuis n'importe où
export PATH="$PWD/target/release:$PATH"

# Vérifier
apollia-os --version   # → apollia-os 0.1.0
```

Pour rendre le PATH permanent :
```bash
echo 'export PATH="/chemin/vers/apollia-v2/target/release:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

---

## 4. Lancer le runtime

```bash
# Terminal 1 — processus principal, garde-le ouvert
apollia-os start
```

Sortie attendue (sans LLM ni pipelines configurés) :
```
  * EventBus            ready
  * AgentRegistry       ready
  * ToolRegistry        ready (3 native tools)
  * LlmRouter           disabled
  * TaskRouter          ready
  * TriggerEngine       ready (0 trigger(s))
  * PipelineEngine      disabled (no [[pipelines]] defined)
  * APIServer           listening on /tmp/apollia.sock + localhost:7771
  * NotificationEngine  disabled
  -------------------------------------------------
  * Runtime ready in 0.3s

  Press Ctrl+C or run `apollia-os stop` to shut down.
```

Avec LLM local et notifications configurés (comme dans les logs) :
```
  * LlmRouter           backend "local"
  * NotificationEngine  1 channel(s)
```

```bash
# Terminal 2 — vérifier l'état
apollia-os status
```

**Ports utilisés :**
- Unix socket : `/tmp/apollia.sock` (communication CLI ↔ runtime)
- TCP : `localhost:7771` (API HTTP + dashboard web)

---

## 5. Tour des features

### 5.1 Agents Python

Un agent = un fichier `.py` avec deux méthodes : `manifest()` + `async run()`. Pas de SDK, pas de classe de base.

```bash
# Déployer un agent
apollia-os agent start agents/apollia-reviewer.py

# Lister les agents actifs
apollia-os agent list

# Infos détaillées (état, outils, namespace mémoire)
apollia-os agent info apollia-reviewer

# Arrêter
apollia-os agent stop apollia-reviewer
```

Un agent a trois états possibles : `active` → `stopping` → `stopped`. S'il manque un outil requis mais non critique, il démarre en état `degraded`.

---

### 5.2 Exécuter des tâches

```bash
# Synchrone — attend le résultat (recommandé pour débuter)
apollia-os run apollia-reviewer "$(pwd)"

# Streaming — affiche la progression en temps réel
apollia-os run apollia-reviewer "$(pwd)" --stream

# Asynchrone — retourne immédiatement un task-id
apollia-os run apollia-reviewer "$(pwd)" --detach
# → task-id: t-abc123

# Suivre une tâche soumise en --detach
apollia-os task status t-abc123

# Annuler
apollia-os task cancel t-abc123

# Lister toutes les tâches
apollia-os task list
```

**Exit codes POSIX :**

| Code | Signification |
|---|---|
| `0` | Succès |
| `1` | Erreur d'usage CLI |
| `2` | Erreur runtime |
| `3` | Tâche échouée |
| `4` | Timeout |
| `5` | Annulée |

---

### 5.3 Outils natifs

Trois outils disponibles pour tous les agents via `ctx.tools.call(nom, params)` :

| Outil | Actions disponibles | Protections |
|---|---|---|
| `file_io` | `read`, `write`, `delete`, `list`, `glob` | Path traversal normalisé |
| `bash_executor` | Exécuter des commandes shell | Namespaces Linux (macOS : mode dev), timeout 30s |
| `python_executor` | Exécuter du code Python | Venv isolé par agent, timeout 60s |

```bash
# Lister les outils avec leur schéma d'entrée
apollia-os tools list

# Détail d'un outil
apollia-os tools describe file_io
apollia-os tools describe bash_executor
```

---

### 5.4 Mémoire

Chaque agent dispose d'un namespace SQLite isolé avec recherche plein texte FTS5. Trois types de mémoire :

| Type | Usage |
|---|---|
| Épisodique | Historique d'événements horodatés |
| Sémantique | Connaissances clé/valeur structurées |
| Procédurale | Plans et séquences d'actions apprises |

```bash
# Inspecter la mémoire d'un agent
apollia-os memory inspect apollia-reviewer
```

Depuis un agent Python :
```python
await ctx.memory.record("key", "value", importance=0.8)
result  = await ctx.memory.recall("key")
results = await ctx.memory.search("terme de recherche")
await ctx.memory.forget("key")
```

---

### 5.5 Audit trail

Chaque appel outil est tracé automatiquement dans SQLite : outil, paramètres, résultat, durée, agent, timestamp.

```bash
apollia-os audit list --limit 20
apollia-os audit stats
```

---

### 5.6 LLM (optionnel)

Sans configuration LLM, les agents fonctionnent en mode tool-only. `ctx.llm` vaut `None` dans ce cas — les agents bien écrits le gèrent gracieusement.

#### Configurer Anthropic

```bash
export ANTHROPIC_API_KEY=sk-ant-...
```

Ajouter dans `apollia.toml` :

```toml
[llm]
default = "anthropic"

[[llm.backends]]
type        = "api"
name        = "anthropic"
api_url     = "https://api.anthropic.com/v1"
model       = "claude-haiku-4-5-20251001"
api_key_env = "ANTHROPIC_API_KEY"
```

#### Configurer un modèle local (aucun cloud)

```toml
[llm]
default = "local"

[[llm.backends]]
type       = "embedded"
name       = "local"
model_path = "~/.apollia/models/llama3.2-3B-q4_K_M.gguf"
device     = "cpu"   # cpu | cuda | metal
```

#### Commandes LLM

```bash
apollia-os llm status           # État des backends configurés
apollia-os llm ping             # Tester la connexion
apollia-os llm ping anthropic   # Tester un backend spécifique
apollia-os llm chat "Explique le pattern acteur Tokio"
apollia-os model list           # Modèles GGUF dans ~/.apollia/models/
```

Depuis un agent Python :
```python
if ctx.llm:
    response = await ctx.llm.chat(
        system="Tu es un expert Rust.",
        user="Explique les acteurs Tokio en 3 phrases.",
    )
    text = response.content
```

---

### 5.7 HITL — Approbations humaines

Un agent peut suspendre et demander une validation humaine avant de continuer. La tâche passe en état `input_required`.

```bash
# Voir les tâches en attente
apollia-os task list --pending-approval

# Approuver — l'agent reprend avec approved=True
apollia-os task resume <task-id> --approve

# Rejeter — l'agent reprend avec approved=False
apollia-os task resume <task-id> --reject
```

Le dashboard `http://localhost:7771/` affiche les approbations en attente avec des boutons Approuver/Rejeter.

Pour déclarer qu'un outil nécessite une approbation dans le manifest :
```python
def manifest(self):
    return {
        "name": "mon-agent",
        ...
        "tools_requiring_approval": ["bash_executor"],
    }
```

---

### 5.8 Triggers — Automatisation

Déclencher un agent automatiquement sans commande manuelle.

#### Types de triggers

| Type | Déclencheur |
|---|---|
| `cron` | Planning cron standard (`0 9 * * MON-FRI`) |
| `interval` | Répétition toutes les N secondes |
| `oneshot` | Une seule fois à une date/heure précise |
| `file_watch` | Création/modification d'un fichier ou répertoire |
| `webhook` | Requête HTTP POST avec HMAC-SHA256 |

#### Configuration dans `apollia.toml`

```toml
[[triggers]]
id      = "review-quotidien"
agent   = "apollia-reviewer"
enabled = true
on_busy = "queue"     # queue | drop | error

[triggers.source]
type     = "cron"
schedule = "0 9 * * MON-FRI"
```

```bash
# Hot-reload sans redémarrer le runtime
apollia-os trigger reload

# Voir l'état des triggers
apollia-os trigger list
apollia-os trigger status review-quotidien
```

---

### 5.9 Pipelines multi-agent

Orchestrer plusieurs agents en séquence ou en parallèle, avec conditions, fallbacks et HITL.

#### Exemple de pipeline dans `apollia.toml`

```toml
[[pipelines]]
id          = "analyse-repo"
description = "Review + rapport"
on_failure  = "fail"    # fail | continue

[[pipelines.steps]]
id    = "review"
agent = "apollia-reviewer"
input = "{{trigger.payload}}"

[[pipelines.steps]]
id         = "rapport"
agent      = "rapport-agent"
input      = "{{steps.review.output}}"
depends_on = ["review"]
```

Les étapes sans `depends_on` commune s'exécutent en parallèle (topologie BFS).

#### Substitutions disponibles dans `input`

| Expression | Valeur injectée |
|---|---|
| `{{trigger.payload}}` | Payload du trigger déclencheur |
| `{{steps.X.output}}` | Output textuel de l'étape X |
| `{{steps.X.status}}` | Statut de l'étape X (`completed`/`failed`) |

```bash
apollia-os pipeline list
apollia-os pipeline run analyse-repo "$(pwd)"
apollia-os pipeline runs analyse-repo --limit 10
apollia-os pipeline status <run-id>
```

---

### 5.10 Dashboard web

Accessible dès que le runtime tourne :

```
http://localhost:7771/
```

Interface HTMX avec mise à jour temps réel via SSE. Sections :

| Section | Contenu |
|---|---|
| Agents | État, tâches actives, capacité |
| Tâches | ID, statut, résultat, durée |
| Triggers | État, derniers déclenchements |
| Pipelines | Runs actifs, progression par étape |
| Plans | Topologie des steps (Mode Orchestré ORIA) |
| Approbations | Tâches HITL en attente + boutons approve/reject |

---

### 5.11 Notifications

```bash
apollia-os notify test          # Tester tous les canaux configurés
apollia-os notify list          # Canaux actifs et leur état
apollia-os notify logs --last 20
```

Configuration dans `apollia.toml` :
```toml
[notifications]
events = ["task.input_required", "task.failed", "pipeline.completed"]

[[notifications.channels]]
id      = "desktop"
type    = "desktop"
enabled = true

[[notifications.channels]]
id      = "slack"
type    = "webhook"
enabled = true
url     = "https://hooks.slack.com/services/..."
```

---

## 6. Démarrer avec apollia-reviewer

L'agent `agents/apollia-reviewer.py` est le seul agent livré dans le repo. Il analyse le dernier commit Git et génère un rapport de code review en Markdown.

**Ce qu'il fait :**
- **Tier 0 (toujours actif)** — analyse statique : TODOs/FIXMEs/HACKs ajoutés, diff trop large (> 300 lignes), fichiers Rust sans `#[test]`
- **Tier 1 (si LLM configuré)** — review LLM du diff : dette technique, cohérence commit/code, suggestions concrètes

**Output :** rapport Markdown dans `<repo>/.apollia/reviews/review-latest.md` + retourné en sortie CLI

### Étape 1 — Démarrer le runtime

```bash
# Terminal 1
apollia-os start
```

### Étape 2 — Déployer l'agent

```bash
# Terminal 2
apollia-os agent start agents/apollia-reviewer.py
```

Vérifier :
```bash
apollia-os agent info apollia-reviewer
# state: active
# tools: bash_executor, file_io
# memory_namespace: apollia-reviewer
# execution_mode: direct
```

### Étape 3 — Lancer une review (Tier 0 — sans LLM)

```bash
# Review du repo Apollia OS lui-même
apollia-os run apollia-reviewer "$(pwd)"
```

Sortie attendue dans le terminal :
```markdown
# Review — feat(apollia-cli): add pipeline orchestration commands

**Branch:** `main`
**Repo:** `/chemin/vers/apollia-v2`

## Diff summary
...

## Static analysis
No static issues detected.

## LLM analysis
_LLM backend not configured — static analysis only._

---
*Generated by apollia-reviewer (Tier 0 — static only)*
```

Le rapport est aussi écrit dans :
```bash
cat .apollia/reviews/review-latest.md
```

### Étape 4 — Review avec streaming

```bash
apollia-os run apollia-reviewer "$(pwd)" --stream
# Affiche les événements en temps réel : tool calls, progression
```

### Étape 5 — Review avec LLM (Tier 1)

Avec une clé Anthropic :

```bash
export ANTHROPIC_API_KEY=sk-ant-...
```

Ajouter dans `apollia.toml` :
```toml
[llm]
default = "anthropic"

[[llm.backends]]
type        = "api"
name        = "anthropic"
api_url     = "https://api.anthropic.com/v1"
model       = "claude-haiku-4-5-20251001"
api_key_env = "ANTHROPIC_API_KEY"
```

Relancer le runtime et l'agent :
```bash
apollia-os stop
apollia-os start
apollia-os agent start agents/apollia-reviewer.py
apollia-os run apollia-reviewer "$(pwd)"
```

La section `## LLM analysis` contiendra maintenant une analyse détaillée du diff.

### Étape 6 — Vérifier l'historique en mémoire

```bash
apollia-os memory inspect apollia-reviewer
# Affiche les reviews enregistrées en mémoire épisodique
```

### Étape 7 — Automatiser via trigger cron

Pour une review automatique à chaque push (ou à 9h chaque matin) :

```toml
[[triggers]]
id      = "review-matin"
agent   = "apollia-reviewer"
enabled = true
on_busy = "drop"

[triggers.source]
type     = "cron"
schedule = "0 9 * * MON-FRI"
```

```bash
apollia-os trigger reload
apollia-os trigger status review-matin
```

---

## 7. Configuration complète

Créer `apollia.toml` à la racine du projet ou dans `~/.config/apollia/apollia.toml`.

**Ordre de priorité (croissant) :** valeurs compilées → `./apollia.toml` → `~/.config/apollia/apollia.toml` → variables d'environnement → flags CLI

### Configuration minimale pour démarrer

```toml
[runtime]
socket    = "/tmp/apollia.sock"
port      = 7771
log_level = "info"             # error | warn | info | debug | trace

[memory]
path = "~/.apollia/memory.db"

[tools]
sandbox              = false   # true sur Linux, false sur macOS
bash_timeout_seconds = 30

[budget]
max_steps               = 20
max_tool_calls          = 50
wall_clock_timeout_secs = 300
```

### Configuration complète annotée

```toml
# ── Runtime ────────────────────────────────────────────────────────────────
[runtime]
socket                = "/tmp/apollia.sock"
port                  = 7771
log_level             = "info"
drain_timeout_seconds = 30      # Délai graceful shutdown

# ── Memory Engine ──────────────────────────────────────────────────────────
[memory]
path             = "~/.apollia/memory.db"
max_size_mb      = 0            # 0 = illimité
episode_ttl_days = 0            # 0 = jamais expiré

# ── Tool Registry ──────────────────────────────────────────────────────────
[tools]
sandbox                  = false  # true sur Linux
venv_base_path           = "~/.apollia/venvs"
bash_timeout_seconds     = 30
python_timeout_seconds   = 60

# ── API ────────────────────────────────────────────────────────────────────
[api]
tcp_enabled  = true
bind_address = "127.0.0.1"

# ── Budget par tâche ───────────────────────────────────────────────────────
[budget]
max_steps               = 20
max_tool_calls          = 50
wall_clock_timeout_secs = 300   # 5 minutes

# ── LLM Router ─────────────────────────────────────────────────────────────
[llm]
default = "anthropic"

[[llm.backends]]
type        = "api"
name        = "anthropic"
api_url     = "https://api.anthropic.com/v1"
model       = "claude-haiku-4-5-20251001"
api_key_env = "ANTHROPIC_API_KEY"

[[llm.backends]]
type        = "api"
name        = "openai"
api_url     = "https://api.openai.com/v1"
model       = "gpt-4o-mini"
api_key_env = "OPENAI_API_KEY"

[[llm.backends]]
type       = "embedded"
name       = "local"
model_path = "~/.apollia/models/llama3.2-3B-q4_K_M.gguf"
device     = "cpu"

# ── Triggers ───────────────────────────────────────────────────────────────
[[triggers]]
id      = "review-quotidien"
agent   = "apollia-reviewer"
enabled = true
on_busy = "queue"   # queue | drop | error

[triggers.source]
type     = "cron"
schedule = "0 9 * * MON-FRI"

# ── Notifications ──────────────────────────────────────────────────────────
[notifications]
events = ["task.input_required", "task.failed", "pipeline.completed"]

[[notifications.channels]]
id      = "desktop"
type    = "desktop"
enabled = true

# ── Pipelines ──────────────────────────────────────────────────────────────
[[pipelines]]
id          = "mon-pipeline"
description = "Exemple"
on_failure  = "fail"

[[pipelines.steps]]
id    = "etape-1"
agent = "apollia-reviewer"
input = "{{trigger.payload}}"
```

### Variables d'environnement

```bash
APOLLIA_SOCKET          # → runtime.socket
APOLLIA_PORT            # → runtime.port
APOLLIA_LOG_LEVEL       # → runtime.log_level
APOLLIA_MEMORY_PATH     # → memory.path
APOLLIA_TOOLS_SANDBOX   # → tools.sandbox
ANTHROPIC_API_KEY       # → clé API Anthropic
OPENAI_API_KEY          # → clé API OpenAI/compatible
RUST_LOG                # → filtres tracing (ex: apollia_runtime=debug)
```

---

## 8. Référence rapide CLI

```bash
# ── Runtime ────────────────────────────────────────────────────────────────
apollia-os start [--port PORT]
apollia-os stop
apollia-os status [--json]

# ── Agents ─────────────────────────────────────────────────────────────────
apollia-os agent start <fichier.py>
apollia-os agent list [--json]
apollia-os agent info <nom>
apollia-os agent stop <nom>

# ── Tâches ─────────────────────────────────────────────────────────────────
apollia-os run <agent> "<input>" [--stream] [--detach] [--json]
apollia-os task list [--pending-approval] [--json]
apollia-os task status <id> [--json]
apollia-os task cancel <id>
apollia-os task inspect <id>       # Lit plans.db sans démarrer le runtime
apollia-os task resume <id> --approve | --reject

# ── Outils & Audit ─────────────────────────────────────────────────────────
apollia-os tools list [--json]
apollia-os tools describe <outil>
apollia-os audit list [--limit 20] [--json]
apollia-os audit stats [--json]

# ── Mémoire ────────────────────────────────────────────────────────────────
apollia-os memory inspect <namespace>

# ── LLM ────────────────────────────────────────────────────────────────────
apollia-os llm status [--json]
apollia-os llm ping [backend] [--json]
apollia-os llm chat "<prompt>" [--backend nom] [--json]
apollia-os model list [--json]     # Modèles locaux ~/.apollia/models/

# ── Triggers ───────────────────────────────────────────────────────────────
apollia-os trigger list [--json]
apollia-os trigger status <id>
apollia-os trigger fire <id>       # Déclencher manuellement
apollia-os trigger enable <id>
apollia-os trigger disable <id>
apollia-os trigger logs <id>
apollia-os trigger reload

# ── Pipelines ──────────────────────────────────────────────────────────────
apollia-os pipeline list [--json]
apollia-os pipeline run <id> "<input>" [--detach] [--json]
apollia-os pipeline runs <id> [--limit 20] [--json]
apollia-os pipeline status <run-id> [--json]

# ── Notifications ──────────────────────────────────────────────────────────
apollia-os notify test [--json]
apollia-os notify list [--json]
apollia-os notify logs [--last 20] [--json]
```

**`--json` est disponible sur toutes les commandes** pour une sortie parseable par scripts/CI.

---

## 9. Écrire son propre agent

Contrat minimal — deux méthodes, une instance globale :

```python
class MonAgent:

    def manifest(self):
        """Décrit l'agent au runtime. Appelé une fois au démarrage."""
        return {
            # Obligatoires
            "name":        "mon-agent",
            "version":     "1.0.0",
            "description": "Ce que fait l'agent.",

            # Outils nécessaires (le runtime vérifie leur disponibilité)
            "tools_required": ["bash_executor", "file_io"],

            # Optionnels
            "max_concurrent_tasks":    1,         # défaut 1
            "execution_mode":          "direct",  # "direct" | "orchestrated" | "auto"
            "memory_namespace":        "mon-ns",  # isolation mémoire
            "tools_requiring_approval": [],        # outils nécessitant HITL
        }

    async def run(self, task, ctx):
        """
        Appelé pour chaque tâche. DOIT retourner un dict avec au minimum
        task_id et status.

        task: {
            "task_id": "t-001",
            "input": {
                "parts": [{"type": "text", "text": "..."}]
            },
            "is_resumed": False,          # True si reprise après HITL
            "input_response": None,       # Réponse HITL si is_resumed
        }

        ctx: RuntimeContext (PyO3)
            ctx.tools   — accès aux outils natifs
            ctx.memory  — mémoire SQLite isolée
            ctx.llm     — LLM router (None si non configuré)
        """

        # Lire l'input
        parts = task["input"]["parts"]
        user_input = parts[0]["text"] if parts else ""

        # Appeler un outil
        result = await ctx.tools.call("bash_executor", {
            "command": f"echo '{user_input}'",
            "timeout_seconds": 10,
        })
        stdout = result.get("stdout", "")

        # Utiliser la mémoire
        if ctx.memory:
            await ctx.memory.record("derniere-tache", user_input)

        # Appeler le LLM si disponible
        llm_response = ""
        if ctx.llm:
            resp = await ctx.llm.chat(
                system="Tu es un assistant concis.",
                user=user_input,
            )
            llm_response = resp.content

        # Retourner le résultat
        return {
            "task_id": task["task_id"],
            "status":  "completed",       # ou "failed"
            "output":  [{"type": "text", "text": stdout or llm_response}],
            "error":   None,
        }


# Instance globale OBLIGATOIRE — le runtime cherche une variable `agent`
agent = MonAgent()
```

Déployer :
```bash
apollia-os agent start ./mon_agent.py
apollia-os run mon-agent "hello world"
```

---

## 10. Commandes just

```bash
just build          # cargo build --workspace
just test           # cargo test --workspace
just test-python    # tests Python (PYO3_PYTHON requis)
just lint           # cargo fmt --check + cargo clippy
just fmt            # cargo fmt --all
just ci             # lint + test + docs + check-includes
just docs           # PlantUML SVGs + ADR index + mdBook
just book           # mdBook uniquement → target/book/
just dev            # mdBook serve avec hot-reload sur http://localhost:3000
just diagrams       # Régénérer les SVGs PlantUML
just adr-index      # Régénérer book/src/decisions/index.md
just clean          # Supprimer artefacts (SVGs conservés)
just clean-all      # Supprimer tout y compris les SVGs
```

---

## 11. Fichiers créés au premier démarrage

```
~/.apollia/
  ├── memory.db           SQLite — mémoire épisodique/sémantique/procédurale
  ├── plans.db            SQLite — plans ORIA Mode Orchestré
  ├── triggers.db         SQLite — état persistant des triggers
  ├── hitl.db             SQLite — approbations HITL en attente
  ├── pipelines.db        SQLite — historique des runs pipelines
  ├── venvs/              Venvs Python (un par agent)
  │   └── apollia-reviewer/
  └── reviews/            Output de apollia-reviewer
      └── review-latest.md

/tmp/apollia.sock       Socket Unix (recréé à chaque `apollia-os start`)
```

---

## 12. Dépannage

### Le build échoue sur macOS avec une erreur PyO3

```bash
# Vérifier que la variable est exportée
echo $PYO3_PYTHON

# Si vide, l'exporter
export PYO3_PYTHON=/opt/homebrew/bin/python3.13

# Relancer le build
cargo build --workspace --release
```

### `apollia-os: command not found`

```bash
# Vérifier que le binaire est compilé
ls target/release/apollia-os

# Ajouter au PATH
export PATH="$(pwd)/target/release:$PATH"
```

### Le runtime ne démarre pas (port ou socket déjà utilisé)

```bash
# Vérifier si un processus apollia tourne déjà
ps aux | grep apollia-os

# Socket orphelin
rm -f /tmp/apollia.sock

# Relancer
apollia-os start
```

### La CLI répond "connection refused"

```bash
# Vérifier que le runtime tourne
ps aux | grep apollia-os

# Vérifier le socket
ls -la /tmp/apollia.sock

# Si le runtime tourne mais le socket est absent, arrêter proprement
pkill apollia-os
apollia-os start
```

### Le venv Python d'un agent est corrompu

```bash
# Supprimer le venv — il sera recréé au prochain `agent start`
rm -rf ~/.apollia/venvs/apollia-reviewer/

apollia-os agent start agents/apollia-reviewer.py
```

### Logs détaillés pour déboguer

```bash
RUST_LOG=apollia_runtime=debug,apollia_aip=debug apollia-os start
```

### Lancer les tests de smoke

```bash
# Tests unitaires et intégration (sans Python)
cargo test --workspace

# Avec Python
PYO3_PYTHON=/opt/homebrew/bin/python3.13 \
  cargo test --workspace --features python-tests -- --nocapture

# Une crate spécifique
cargo test -p apollia-runtime -- --nocapture

# Un test par nom
cargo test shutdown -- --nocapture
```
