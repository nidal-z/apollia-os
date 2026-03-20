# Configuration — apollia.toml — Apollia OS

> Référence complète du fichier de configuration apollia.toml avec toutes les options, valeurs par défaut et exemples.
> Public cible : opérateur, développeur

---

## Vue d'ensemble

Apollia OS cherche sa configuration dans cet ordre (priorité croissante) :
1. Valeurs par défaut compilées
2. `./apollia.toml` dans le répertoire courant
3. `~/.config/apollia/apollia.toml` (configuration utilisateur)
4. Variables d'environnement (préfixe `APOLLIA_`)
5. Flags CLI (`--config`, `--socket`, etc.)

---

## Fichier minimal

```toml
[runtime]
socket = "/tmp/apollia.sock"
port   = 7771

[memory]
path = "./data/memory.db"

[tools]
sandbox = true
```

---

## Référence complète

### [runtime]

```toml
[runtime]
# Chemin du socket Unix pour la communication locale
# Défaut : /tmp/apollia.sock
socket = "/tmp/apollia.sock"

# Port TCP de l'API HTTP
# Défaut : 7771
port = 7771

# Niveau de log (error | warn | info | debug | trace)
# Défaut : info
log_level = "info"

# Délai de drain graceful shutdown en secondes
# Défaut : 30
drain_timeout_seconds = 30
```

### [memory]

```toml
[memory]
# Chemin de la base SQLite pour le Memory Engine
# Défaut : ./data/memory.db
path = "./data/memory.db"

# Taille maximale de la base en Mo (0 = illimité)
# Défaut : 0
max_size_mb = 0

# TTL des épisodes en jours (0 = jamais expiré)
# Défaut : 0
episode_ttl_days = 0

# Activer FTS5 (full-text search) — requis pour ctx.memory.search()
# Défaut : true
fts5_enabled = true
```

### [tools]

```toml
[tools]
# Activer le sandbox Linux namespaces pour bash_executor et python_executor
# Défaut : true sur Linux, false sur macOS (namespaces non disponibles)
sandbox = true

# Répertoire de base pour les venvs Python par agent
# Défaut : ./data/venvs
venv_base_path = "./data/venvs"

# Timeout par défaut pour bash_executor en secondes
# Défaut : 30
bash_timeout_seconds = 30

# Timeout par défaut pour python_executor en secondes
# Défaut : 60
python_timeout_seconds = 60
```

### [api]

```toml
[api]
# Activer l'API TCP (en plus du socket Unix)
# Défaut : true
tcp_enabled = true

# Lier l'API TCP sur cette adresse (0.0.0.0 = toutes les interfaces)
# Défaut : 127.0.0.1 (loopback uniquement)
bind_address = "127.0.0.1"
```

### [llm] et [[llm.backends]] — Moteur LLM

La section `[llm]` configure le `LlmRouter`. Elle est **optionnelle** — le runtime démarre sans LLM, et `ctx.llm` sera `None` dans les agents.

```toml
[llm]
# Nom du backend utilisé par défaut (get(None) → ce backend)
default = "local"

# Observabilité — paramètres communs à tous les backends
[llm.observability]
log_token_usage  = true   # log tokens consommés (défaut: true)
log_latency      = true   # log latence de chaque appel (défaut: true)
log_cost         = false  # log coût USD estimé pour les backends cloud (défaut: false)
debug_log_prompt = false  # log le prompt complet au niveau TRACE (défaut: false, JAMAIS en prod)
```

#### Backend embarqué — inférence locale in-process

```toml
[[llm.backends]]
type         = "embedded"
name         = "local"
model_path   = "~/.apollia/models/llama3.2-3B-q4_K_M.gguf"
quantization = "q4_k_m"
device       = "cpu"    # "cpu" (défaut) | "metal" | "cuda"
```

Le chemin `~` est résolu au démarrage. Si le fichier est absent, `LlmError::ModelNotFound` est émis immédiatement (fail-fast, Principe #4).

**Feature requise selon le device :**

| Valeur `device` | Feature à compiler | Commande |
|---|---|---|
| `"cpu"` | `local` ou `local-cpu` | `cargo build --features local` |
| `"metal"` | `local-metal` | `cargo build --features local-metal` ¹ |
| `"cuda"` | `local-cuda` | `cargo build --features local-cuda` (non testé) |

¹ Sans Xcode complet : préfixer avec `MISTRALRS_METAL_PRECOMPILE=0` (shaders Metal compilés JIT au premier appel au lieu d'être baked au build — voir [INSTALL.md](./INSTALL) pour le détail).

#### Backend cloud OpenAI-compatible

```toml
[[llm.backends]]
type        = "api"
name        = "gpt-4o-mini"
api_url     = "https://api.openai.com/v1"
model       = "gpt-4o-mini"
api_key_env = "OPENAI_API_KEY"   # nom de la variable d'environnement
```

#### Backend cloud Anthropic

```toml
[[llm.backends]]
type        = "api"
name        = "anthropic"
api_url     = "https://api.anthropic.com/v1"
model       = "claude-haiku-4-5-20251001"
api_key_env = "ANTHROPIC_API_KEY"
```

**Heuristique de sélection client :** `api_url.contains("anthropic.com")` → `AnthropicClient`. Sinon → `OpenAICompatibleClient`.

**Comportement si clé API absente :** warning loggé au démarrage (`WARN apollia_llm — backend "anthropic" skipped: ANTHROPIC_API_KEY not set`), backend ignoré, runtime continue. L'agent recevra `ctx.llm = None` si *tous* les backends configurés échouent ou sont absents.

**Expansion du chemin `~` :** effectuée au parsing TOML (avant `LlmRouter::from_config`). Le chemin est converti en chemin absolu — si le fichier est absent, `LlmError::ModelNotFound` est émis au démarrage (fail-fast, Principe #4).

#### Exemple — local CPU (défaut)

```toml
[llm]
default = "local"

[[llm.backends]]
type         = "embedded"
name         = "local"
model_path   = "~/.apollia/models/llama3.2-3B-q4_K_M.gguf"
quantization = "q4_k_m"
device       = "cpu"    # --features local
```

#### Exemple — local Metal (Apple Silicon GPU)

```toml
[llm]
default = "local-metal"

[[llm.backends]]
type         = "embedded"
name         = "local-metal"
model_path   = "~/.apollia/models/llama3.2-3B-q4_K_M.gguf"
quantization = "q4_k_m"
device       = "metal"  # --features local-metal (ou local-metal,local-accelerate)
```

#### Exemple complet — config mixte (local Metal + cloud)

```toml
[llm]
default = "local-metal"    # GPU Apple Silicon en priorité

[llm.observability]
log_token_usage  = true
log_latency      = true
log_cost         = true
debug_log_prompt = false

# Backend 1 : modèle local sur GPU Apple Silicon (--features local-metal)
[[llm.backends]]
type         = "embedded"
name         = "local-metal"
model_path   = "~/.apollia/models/llama3.2-3B-q4_K_M.gguf"
quantization = "q4_k_m"
device       = "metal"

# Backend 2 : Anthropic cloud (fallback pour les tâches gourmandes en raisonnement)
[[llm.backends]]
type        = "api"
name        = "anthropic"
api_url     = "https://api.anthropic.com/v1"
model       = "claude-haiku-4-5-20251001"
api_key_env = "ANTHROPIC_API_KEY"
```

Dans un agent, pour utiliser le backend cloud explicitement :
```python
response = await ctx.llm.chat(
    system="Analyse complexe...", user=user_input, backend="anthropic"
)
```

---

### [budget] — défauts StepBudget

```toml
[budget]
# Nombre maximum d'étapes par tâche (défaut runtime)
# L'agent peut augmenter cette valeur via son manifest, mais pas la dépasser
max_steps = 10

# Nombre maximum d'appels d'outils par tâche (défaut runtime)
max_tool_calls = 20

# Timeout mur en secondes par tâche (défaut runtime)
wall_clock_timeout_secs = 300
```

---

## Configuration opérationnelle — migrée vers SQLite *(Sprint 17)*

Depuis le Sprint 17 (ADR-033), les sections opérationnelles suivantes ne sont plus dans `apollia.toml` :

| Ancienne section TOML | Désormais dans | Gestion via |
|---|---|---|
| `[[triggers]]` | `~/.apollia/triggers_def.db` | API REST CRUD + app desktop |
| `[[pipelines]]` | `~/.apollia/pipelines_def.db` | API REST CRUD + app desktop |
| `[notifications]` | `~/.apollia/notifications.db` | API REST CRUD + app desktop |

**Pourquoi :** un opérateur non-technique peut créer, modifier et supprimer ses triggers, pipelines et canaux de notification depuis l'interface graphique — sans toucher au TOML, sans redémarrer le runtime.

`apollia.toml` conserve uniquement la configuration **structurelle** : `[runtime]`, `[memory]`, `[tools]`, `[api]`, `[llm]`, `[budget]`, `[observability]`.

Voir :
- [Briques Triggers](./Briques-Triggers) — CRUD triggers
- [Briques Pipelines](./Briques-Pipelines) — CRUD pipelines
- [Briques Notifications](./Briques-Notifications) — CRUD notifications
- [API HTTP Reference](./API-HTTP-Reference) — endpoints CRUD

---

## Variables d'environnement

Toutes les options configurables via variables d'environnement avec le préfixe `APOLLIA_` :

| Variable | Équivalent TOML |
|---|---|
| `APOLLIA_SOCKET` | `runtime.socket` |
| `APOLLIA_PORT` | `runtime.port` |
| `APOLLIA_LOG_LEVEL` | `runtime.log_level` |
| `APOLLIA_MEMORY_PATH` | `memory.path` |
| `APOLLIA_TOOLS_SANDBOX` | `tools.sandbox` |
| `RUST_LOG` | Filtres tracing (ex: `apollia_runtime=debug`) |

---

## Profil de développement

```toml
# apollia-dev.toml

[runtime]
log_level = "debug"
socket = "/tmp/apollia-dev.sock"
port   = 7772

[tools]
sandbox = false  # désactivé sur macOS / en dev

[budget]
max_steps = 50   # plus permissif pour le debug
max_tool_calls = 100
wall_clock_timeout_secs = 600

# LLM local pour le dev (aucun coût cloud)
# Sur macOS Apple Silicon : utiliser device = "metal" avec --features local-metal
[llm]
default = "local"

[[llm.backends]]
type         = "embedded"
name         = "local"
model_path   = "~/.apollia/models/llama3.2-3B-q4_K_M.gguf"
quantization = "q4_k_m"
device       = "cpu"    # remplacer par "metal" avec --features local-metal sur Apple Silicon

[llm.observability]
debug_log_prompt = true   # activer uniquement en dev
```

Utiliser avec :
```bash
apollia-os start --config apollia-dev.toml
```

---

## Profil de production Linux

```toml
# apollia-prod.toml

[runtime]
log_level = "warn"
socket = "/run/apollia/apollia.sock"
port   = 7771
drain_timeout_seconds = 60   # plus long pour les tâches longues

[memory]
path = "/var/lib/apollia/memory.db"
max_size_mb = 2048
episode_ttl_days = 90

[tools]
sandbox = true   # activer Linux namespaces
venv_base_path = "/var/lib/apollia/venvs"

[api]
bind_address = "127.0.0.1"  # jamais exposer sur 0.0.0.0 en prod

[budget]
max_steps = 10
max_tool_calls = 20
wall_clock_timeout_secs = 300

# LLM cloud en production
[llm]
default = "anthropic"

[llm.observability]
log_token_usage = true
log_latency     = true
log_cost        = true    # activer pour suivi des coûts en prod

[[llm.backends]]
type        = "api"
name        = "anthropic"
api_url     = "https://api.anthropic.com/v1"
model       = "claude-haiku-4-5-20251001"
api_key_env = "ANTHROPIC_API_KEY"
```

---

## Variables d'environnement LLM

| Variable | Usage |
|---|---|
| `ANTHROPIC_API_KEY` | Clé API Anthropic (backend `type = "api"` avec api_url anthropic) |
| `OPENAI_API_KEY` | Clé API OpenAI ou compatible |
| `APOLLIA_LLM_DEFAULT` | Override du backend par défaut |

---


---

## Voir aussi

- [INSTALL.md](./INSTALL) — installation et prérequis
- [INSTALL Production](./INSTALL-Production) — déploiement en production
- [Ops Exploitation et Debug](./Ops-Exploitation-et-Debug) — monitoring et debug
- [Briques Pipelines](./Briques-Pipelines) — documentation complète `[[pipelines]]`
