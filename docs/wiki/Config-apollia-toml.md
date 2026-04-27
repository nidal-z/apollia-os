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
# Fichier vide valide — toutes les valeurs par défaut sont compilées dans le binaire.
# Aucune section n'est obligatoire pour démarrer Apollia OS.

# Personnalisation minimale recommandée :
[api]
unix_socket = "/tmp/apollia.sock"
port        = 7771
```

---

## Référence complète

### [runtime]

```toml
[runtime]
# Capacité du bus d'événements interne (nombre de slots)
# Défaut : 1024
eventbus_capacity = 1024

# Capacité des files de messages par agent (AgentMailbox)
# Défaut : 100
mailbox_capacity = 100

# Timeout global de démarrage du runtime, en secondes
# 0 = illimité — non recommandé en production
# Défaut : 300
startup_timeout_secs = 300
```

> **Socket, port, log_level :** Ces paramètres ne sont **pas** dans `[runtime]`.
> - Socket Unix et port TCP → section **`[api]`** (`unix_socket`, `port`)
> - Niveau de log → variable d'environnement **`RUST_LOG`** (ex: `RUST_LOG=apollia_runtime=debug`) ou flag CLI **`--debug`**
> - Drain shutdown → `ShutdownConfig` hardcodé (non exposé dans `apollia.toml`)

### [memory]

> ⚠️ **Section non supportée.** Il n'existe pas de section `[memory]` dans `ApolliaCConfig`. Les clés `path`, `max_size_mb`, `episode_ttl_days`, `fts5_enabled` sont ignorées si présentes dans `apollia.toml`.
>
> - **Chemin mémoire :** calculé automatiquement → `~/.apollia/memory/<namespace>.db` (non configurable via TOML)
> - **FTS5 :** toujours activé — pas de toggle de désactivation
> - **TTL par épisode :** configurable par agent via son manifest (`memory_config.episodic_retention_days`), pas globalement

### [tools]

```toml
[tools]
# Outils natifs désactivés statiquement au démarrage.
# Complète la table tools de governance.db : un outil absent des deux sources
# est actif. Un outil présent dans l'une ou l'autre est inactif.
# Défaut : []
disabled = []

# Limite de caractères retournés par un appel d'outil
# Défaut : 30000
max_output_chars = 30000
```

> **sandbox, venv_base_path, bash_timeout_seconds, python_timeout_seconds :** Ces clés ne sont **pas** dans `[tools]`.
> - **Sandbox :** activé/désactivé par profil `SandboxProfile` dans le manifest de chaque outil (non configurable globalement via TOML)
> - **Chemin venvs :** `~/.apollia/venvs/` hardcodé (non configurable via TOML)
> - **Timeouts bash/python :** passés dans le payload d'appel (`BashInput.timeout_secs`, `PythonInput.timeout_secs`), pas de défaut global TOML

#### [tools.web_search]

```toml
[tools.web_search]
# Backend préféré pour l'outil web_search.
# "auto"       : DuckDuckGo en priorité, Brave si une clé API est disponible (défaut)
# "duckduckgo" : DuckDuckGo uniquement (zero-config)
# "brave"      : Brave uniquement — requiert une clé API
backend = "auto"

# Si true, le démarrage échoue si le backend sélectionné n'est pas opérationnel.
# Utile pour les déploiements où web_search est critique.
# Défaut : false
require_configured = false

[tools.web_search.brave]
# Nom de la variable d'environnement portant la clé API Brave Search.
# Défaut : "BRAVE_SEARCH_API_KEY"
api_key_env_var = "BRAVE_SEARCH_API_KEY"

# Timeout de requête HTTP en secondes. Bornes : [1, 120]. Défaut : 15.
timeout_secs = 15

# Nombre maximum de résultats retournés par Brave. Bornes : [1, 20]. Défaut : 10.
max_results = 10

[tools.web_search.duckduckgo]
# Timeout de requête HTTP en secondes. Bornes : [1, 120]. Défaut : 15.
timeout_secs = 15

# Taille maximale de la réponse HTML DuckDuckGo avant abandon, en kio.
# Bornes : [16, 16 384]. Défaut : 1024 (1 Mio).
max_response_kb = 1024
```

#### [tools.web_read]

```toml
[tools.web_read]
# Timeout de requête HTTP en secondes. Bornes : [1, 120]. Défaut : 20.
timeout_secs = 20

# Taille maximale de la réponse HTTP avant abandon, en kio.
# Bornes : [64, 32 768]. Défaut : 2048 (2 Mio).
max_response_kb = 2048

# Active le garde anti-SSRF : rejette les URL à destination d'hôtes privés
# (127.x, 10.x, 192.168.x, ::1, etc.). Désactiver uniquement en lab isolé.
# Défaut : true — NE PAS désactiver en production.
ssrf_guard = true
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

### [mcp]

```toml
[mcp]
# Durée de validité des approbations HITL MCP, en heures.
# Quand un opérateur exécute `apollia mcp set-approval`, l'entrée dans mcp_approvals
# est créée avec expires_at = now + approval_ttl_hours.
# 0 = approbation permanente (jamais expirée).
# Défaut : 24. Bornes : [0, 8760] (0 h à 1 an).
approval_ttl_hours = 24
```

### [permissions]

```toml
[permissions]
# Commandes auto-approuvées sans HITL (SafeList, couche 1).
# Format : "tool_name(arg_text)" ou "tool_name".
# Vide par défaut — aucune commande n'est auto-approuvée sans configuration explicite.
safe_commands = [
    "bash_executor(git status)",
    "bash_executor(git log)",
]

# Active la détection d'injections shell (couche 3, priorité absolue).
# Désactiver uniquement pour les environnements de test contrôlés.
# Défaut : true — NE PAS désactiver en production.
injection_detection = true

# Durée de vie des règles préfixe SQLite (PrefixRuleEngine, couche 2), en heures.
# Défaut : 168 (7 jours).
prefix_rule_ttl_hours = 168

# Chemin de la base SQLite consolidée (governance.db).
# Contient les tables permission_rules, permission_audit, tools, tool_credentials.
# Défaut : ~/.apollia/governance.db
db_path = "~/.apollia/governance.db"
```

### [filesystem]

```toml
[filesystem.journal]
# Active le journal réversible (ADR-069).
# Défaut : true
enabled = true

# Nombre maximal de sessions conservées dans le journal.
# Les sessions au-delà sont purgées (LRU).
# Défaut : 50
max_sessions = 50

# Répertoire racine du journal réversible.
# Défaut : ~/.apollia/journal
root = "~/.apollia/journal"
```

### [pipelines]

```toml
[pipelines]
# Timeout par défaut d'un step, en secondes. Peut être surchargé par step via timeout_secs.
# Défaut : 60. Bornes : [5, 3600].
default_step_timeout_secs = 60
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

> ⚠️ **Section non supportée.** Il n'existe pas de section `[budget]` dans `ApolliaCConfig`. Les clés `max_steps`, `max_tool_calls`, `wall_clock_timeout_secs` sont ignorées si présentes dans `apollia.toml`.
>
> Les valeurs par défaut du StepBudget sont compilées dans `StepBudgetConfig::default()` :
> - `max_steps` = **30**
> - `max_tool_calls` = **60**
> - `wall_clock_secs` = **600** (10 minutes)
>
> Chaque agent peut **surcharger ces valeurs** dans son manifest (`step_budget` → voir [ch07-01-step-budget](../../book/src/ch07-01-step-budget.md)). Il n'y a pas de configuration globale TOML pour le StepBudget.

---

### [stt] — Moteur Speech-to-Text *(section TOML dépréciée en)*

> **Déprécié :** la configuration STT est désormais dans `~/.apollia/system.db` (table `stt_config`), gérée via `GET/PUT /api/v1/stt/config` ou l'app desktop. Si cette section est présente dans `apollia.toml`, un warning est émis au démarrage mais le boot continue normalement.

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

`apollia.toml` conserve uniquement la configuration **structurelle** : `[runtime]`, `[api]`, `[tools]`, `[oria]`, `[pipelines]`, `[hitl]`, `[llm]`, `[a2a]`, `[registry]`. Les sections `[memory]` et `[budget]` ne sont **pas** désérialisées.

Voir :
- [Briques Triggers](./Briques-Triggers) — CRUD triggers
- [Briques Pipelines](./Briques-Pipelines) — CRUD pipelines
- [Briques Notifications](./Briques-Notifications) — CRUD notifications
- [API HTTP — Index](./API-HTTP-Reference) — endpoints CRUD (3 pages par domaine)

---

## Variables d'environnement

Toutes les options configurables via variables d'environnement avec le préfixe `APOLLIA_` :

| Variable | Effet |
|---|---|
| `APOLLIA_SOCKET` | ⚠️ Non supporté — le socket Unix se configure via `[api] unix_socket` |
| `APOLLIA_PORT` | ⚠️ Non supporté — le port TCP se configure via `[api] port` |
| `APOLLIA_LOG_LEVEL` | ⚠️ Non supporté — utiliser `RUST_LOG` à la place |
| `RUST_LOG` | Filtres tracing (ex: `RUST_LOG=apollia_runtime=debug,apollia_tools=info`) |
| `APOLLIA_LLM_DEFAULT` | Override du backend LLM par défaut (équiv. `[llm] default`) |
| `BRAVE_SEARCH_API_KEY` | Clé API Brave Search pour l'outil `web_search` |

---

## Profil de développement

```toml
# apollia-dev.toml

[api]
unix_socket = "/tmp/apollia-dev.sock"
port        = 7772

# Budget plus permissif : surcharger par agent via le manifest (step_budget section)
# Pas de section [budget] — utiliser le manifest de l'agent

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

Niveau de log en dev : `RUST_LOG=apollia_runtime=debug,apollia_tools=info apollia-os start --config apollia-dev.toml`

Utiliser avec :
```bash
apollia-os start --config apollia-dev.toml
```

---

## Profil de production Linux

```toml
# apollia-prod.toml

[api]
unix_socket  = "/run/apollia/apollia.sock"
port         = 7771
bind         = "127.0.0.1"  # jamais exposer sur 0.0.0.0 en prod
require_token = true

[tools]
disabled     = []            # tous les outils actifs — ajuster selon la politique de sécurité
max_output_chars = 30000

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

## Variables d'environnement LLM et outils web

| Variable | Usage |
|---|---|
| `ANTHROPIC_API_KEY` | Clé API Anthropic (backend `type = "api"` avec api_url anthropic) |
| `OPENAI_API_KEY` | Clé API OpenAI ou compatible |
| `APOLLIA_LLM_DEFAULT` | Override du backend par défaut |
| `BRAVE_SEARCH_API_KEY` | Clé API Brave Search (ou autre nom via `tools.web_search.brave.api_key_env_var`) |

---


---

## Voir aussi

- [INSTALL.md](./INSTALL) — installation et prérequis
- [INSTALL Production](./INSTALL-Production) — déploiement en production
- [Ops Exploitation et Debug](./Ops-Exploitation-et-Debug) — monitoring et debug
- [Briques Pipelines](./Briques-Pipelines) — documentation complète `[[pipelines]]`
