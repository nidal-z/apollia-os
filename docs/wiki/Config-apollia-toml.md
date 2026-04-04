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
# Adresse IP sur laquelle binder le listener TCP.
# Défaut : 127.0.0.1 (loopback uniquement — inaccessible depuis le réseau)
# ⚠️  Ne passer à 0.0.0.0 que dans des contextes contrôlés (VM, CI) — voir ADR-051.
bind = "127.0.0.1"

# Port TCP du serveur REST.
# Défaut : 7771
port = 7771

# Exiger un token Bearer sur toutes les connexions TCP entrantes.
# Quand true (défaut), chaque requête TCP doit porter Authorization: Bearer <token>.
# Le socket Unix n'est jamais soumis à cette vérification.
# Token stocké dans ~/.apollia/api-token (chmod 0600, généré au premier démarrage).
# Rotation manuelle : apollia-os config rotate-token
# Défaut : true — NE PAS désactiver en production.
require_token = true

# Chemin du socket Unix local (utilisé par CLI et desktop sans auth).
# Défaut : /tmp/apollia.sock
unix_socket = "/tmp/apollia.sock"
```

> **Sécurité :** Le token `~/.apollia/api-token` est comparé à temps constant (pas de timing attack). Le runtime refuse de démarrer si le fichier a des permissions trop ouvertes (`0640`, `0644`, etc.). Voir [ADR-051](./Decisions-Log#adr-051) et [Securite-Local-First](./Securite-Local-First).

### [oria]

```toml
[oria]
# Nombre maximal de replans autorisés par exécution orchestrée.
# 0 = aucun replan (la tâche échoue au premier plan raté).
# Défaut : 2. Bornes : [0, 10].
max_replans = 2

# Score de complexité au-delà duquel l'Observer passe en mode Orchestrated.
# Défaut : 0.40. Bornes : [0.0, 1.0].
orchestrated_threshold = 0.40

# Limite de caractères de la sortie d'un step mémorisée dans la mémoire épisodique.
# Au-delà, le contenu est tronqué avec [truncated]. Voir ADR-054 et STEP_MEMORY_OUTPUT_MAX_CHARS.
# Défaut : 200. Bornes : [50, 10000].
step_memory_max_chars = 200

# Intervalle de vérification du StepBudget restant, en millisecondes.
# Défaut : 100. Bornes : [10, 5000].
budget_poll_ms = 100
```

### [pipelines]

```toml
[pipelines]
# Timeout par défaut d'un step, en secondes. Peut être surchargé par step via timeout_secs.
# Défaut : 300. Bornes : [5, 3600].
default_step_timeout_secs = 300
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

### [stt] — Moteur Speech-to-Text *(Sprint 24 — section TOML dépréciée en Sprint 28)*

> **Déprécié depuis Sprint 28 :** la configuration STT est désormais dans `~/.apollia/system.db` (table `stt_config`), gérée via `GET/PUT /api/v1/stt/config` ou l'app desktop. Si cette section est présente dans `apollia.toml`, un warning est émis au démarrage mais le boot continue normalement.

La section `[stt]` configure le moteur STT embarqué (ADR-041). Elle est **optionnelle** — le runtime démarre sans STT si la table `stt_config` est absente.

```toml
[stt]
# Activer le moteur STT
# Défaut : false
enabled = true

# Chemin du modèle GGML Whisper
# Défaut : ~/.apollia/models/whisper-large-v3-fr-q5_0.bin
model_path = "~/.apollia/models/whisper-large-v3-fr-q5_0.bin"

# Raccourci clavier global (desktop uniquement)
# Défaut : "ctrl+shift+space"
hotkey = "ctrl+shift+space"

# Mode d'injection du texte transcrit
# "paste"     : copie dans le clipboard puis simule Cmd/Ctrl+V (injection automatique)
# "clipboard" : copie dans le clipboard sans coller automatiquement
# Défaut : "paste"
clipboard_mode = "paste"

# Restaurer le contenu précédent du clipboard après injection
# Défaut : true
clipboard_restore = true

# Seuil de silence en dB pour le trim audio
# Défaut : -40.0
silence_threshold_db = -40.0

# Durée maximale d'enregistrement en secondes
# Défaut : 60
max_recording_sec = 60

# Langue par défaut pour la transcription (code ISO 639-1)
# Défaut : "fr"
language = "fr"

# Mode de déclenchement du raccourci
# "toggle"       : premier appui = ON, deuxième = OFF
# "push-to-talk" : maintenu = ON, relâché = OFF
# Défaut : "toggle"
trigger_mode = "toggle"
```

**Comportement si modèle absent :** warning loggé au démarrage, `stt_engine = None`, runtime continue. L'app desktop et la CLI affichent un message invitant à télécharger le modèle (`apollia-os stt model download`).

**Feature flags de compilation :**

| Feature | Commande | Usage |
|---|---|---|
| `stt-whisper-cpp` (défaut) | `cargo build` | Backend whisper.cpp CPU |
| `stt-metal` | `cargo build --features stt-metal` | Accélération Apple Silicon Metal |
| `stt-cuda` | `cargo build --features stt-cuda` | Accélération NVIDIA CUDA |

---

## Configuration opérationnelle — migrée vers SQLite *(Sprints 17 + 28)*

Les sections opérationnelles suivantes ne sont plus dans `apollia.toml` :

| Ancienne section TOML | Désormais dans | Gestion via | Sprint |
|---|---|---|---|
| `[[triggers]]` | `~/.apollia/triggers_def.db` | API REST CRUD + app desktop | 17 |
| `[[pipelines]]` | `~/.apollia/pipelines_def.db` | API REST CRUD + app desktop | 17 |
| `[notifications]` | `~/.apollia/notifications.db` | API REST CRUD + app desktop | 17 |
| `[stt]` | `~/.apollia/system.db` (table `stt_config`) | `GET/PUT /api/v1/stt/config` + app desktop | 28 |
| `[[llm.backends]]` | `~/.apollia/system.db` (table `llm_backends`) | `GET/POST/PUT/DELETE /api/v1/llm/backends` + app desktop | 28 |
| `mcp.toml` / `[mcp]` | `~/.apollia/mcp.db` (table `mcp_servers`) | API REST MCP + app desktop | 28 |

**Pourquoi :** un opérateur peut créer, modifier et supprimer ses backends LLM, sa config STT, ses serveurs MCP et ses triggers depuis l'interface graphique — sans toucher au TOML, sans redémarrer le runtime.

`apollia.toml` conserve uniquement la configuration **structurelle** : `[runtime]`, `[memory]`, `[tools]`, `[api]`, `[oria]`, `[pipelines]`, `[hitl]`, `[llm]` (observabilité uniquement), `[budget]`.

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
