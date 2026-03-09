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
type       = "embedded"
name       = "local"
model_path = "~/.apollia/models/llama3.2-3B-q4_K_M.gguf"
device     = "cpu"    # "cpu" | "cuda" | "metal"
```

Requis : compilé avec `--features local`. Le chemin `~` est résolu au démarrage.

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

#### Exemple complet — config mixte (local + cloud)

```toml
[llm]
default = "local"    # préférer le modèle local par défaut

[llm.observability]
log_token_usage  = true
log_latency      = true
log_cost         = true
debug_log_prompt = false

# Backend 1 : modèle local (nécessite --features local)
[[llm.backends]]
type       = "embedded"
name       = "local"
model_path = "~/.apollia/models/llama3.2-3B-q4_K_M.gguf"
device     = "cpu"

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
[llm]
default = "local"

[[llm.backends]]
type       = "embedded"
name       = "local"
model_path = "~/.apollia/models/llama3.2-3B-q4_K_M.gguf"
device     = "cpu"

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

## Section [[triggers]] *(Sprint 9)*

Tableau TOML — chaque entrée déclenche automatiquement un agent selon une règle déclarative.

```toml
# Trigger Cron — chaque lundi à 8h
[[triggers]]
id      = "rapport-hebdomadaire"
agent   = "rapport-agent"
enabled = true
on_busy = "queue"    # queue | drop | error

[triggers.source]
type     = "cron"
schedule = "0 8 * * MON"

[triggers.input]
text = "Génère le rapport de la semaine {{week_iso}}"

# ---

# Trigger Interval — toutes les 30 minutes
[[triggers]]
id      = "check-inbox"
agent   = "mail-agent"
enabled = true
on_busy = "drop"

[triggers.source]
type  = "interval"
every = "30m"    # 30m | 1h | 6h | 1d

# ---

# Trigger FileWatch — sur création de fichier
[[triggers]]
id      = "import-csv"
agent   = "import-agent"
enabled = true
on_busy = "queue"

[triggers.source]
type   = "file_watch"
path   = "~/imports/"
events = ["create"]  # create | modify | delete | any

[triggers.input]
text = "Importe le fichier {{filename}}"

# ---

# Trigger Webhook — authentifié HMAC-SHA256
[[triggers]]
id      = "github-push"
agent   = "deploy-agent"
enabled = true
on_busy = "error"

[triggers.source]
type   = "webhook"
secret = "un-secret-robuste-minimum-32-caracteres"
```

**Variables `on_busy` :**

| Valeur | Comportement si agent WORKING |
|---|---|
| `queue` | Soumet la tâche — elle attend dans la file |
| `drop` | Ignore le fire, trace `TriggerSkipped` dans SQLite |
| `error` | Émet `TriggerError` sur EventBus |

**Variables de template disponibles :**

| Variable | Disponible dans | Description |
|---|---|---|
| `{{week_iso}}` | Cron, Interval | Semaine ISO (ex: `2026-W10`) |
| `{{date_iso}}` | Cron, Interval | Date ISO (ex: `2026-03-09`) |
| `{{filename}}` | FileWatch | Nom du fichier modifié |
| `{{filepath}}` | FileWatch | Chemin complet |
| `{{size_bytes}}` | FileWatch | Taille en octets |
| `{{file_event}}` | FileWatch | Type d'événement (`create`, `modify`, etc.) |
| `{{webhook_body}}` | Webhook | Contenu JSON du body |

**Validation au démarrage (`enabled=true`) :**
- Cron : expression valide (`cron::Schedule::from_str`)
- FileWatch : chemin `~` résolu, warning si répertoire absent
- Webhook : secret non vide (minimum 32 caractères recommandé)
- `enabled=false` : validation ignorée entièrement

---

## [notifications] — Moteur de notifications *(Sprint 11)*

La section `[notifications]` configure le `NotificationEngine`. Elle est **optionnelle** — si absente, le moteur n'est pas démarré et aucune notification n'est envoyée.

### Vue d'ensemble

Le système de notifications permet d'informer l'opérateur d'événements importants du runtime (tâche en attente d'input, échec, agent dégradé…) via des canaux externes. Le filtrage s'effectue à deux niveaux :

1. **Niveau moteur** — la liste `events` de `[notifications]` définit les événements que le moteur dispatch à ses canaux.
2. **Niveau canal** — chaque `[[notifications.channels]]` peut affiner cette liste avec son propre champ `events`.

### Structure TOML complète

```toml
[notifications]
# Événements globaux — filtrage au niveau du moteur
# Valeurs : "task.input_required", "task.failed", "task.completed",
#            "agent.degraded", "llm.backend_down", "trigger.error"
# Défaut : tous les événements si absent
events = ["task.input_required", "task.failed"]

[[notifications.channels]]
id      = "desktop"
type    = "desktop"
enabled = true
# events = ["task.input_required"]  # optionnel — filtre canal spécifique

[[notifications.channels]]
id      = "slack-webhook"
type    = "webhook"
enabled = true
url     = "https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXX"
events  = ["task.input_required", "task.failed", "agent.degraded"]
```

### Champs `[notifications]`

| Champ | Type | Défaut | Description |
|---|---|---|---|
| `events` | list[str] | (tous) | Liste des événements que le moteur dispatch. Si absent, tous les événements reconnus sont transmis. |

**Valeurs d'événements reconnues :**

| Événement | Sévérité | Description |
|---|---|---|
| `task.input_required` | Warning | Une tâche est en attente d'input humain (HITL) |
| `task.failed` | Error | Une tâche s'est terminée en erreur |
| `task.completed` | Info | Une tâche s'est terminée avec succès |
| `agent.degraded` | Warning | Un agent est passé à l'état DEGRADED |
| `llm.backend_down` | Error | Un backend LLM est inaccessible |
| `trigger.error` | Error | Un trigger a émis une erreur |

### Champs `[[notifications.channels]]`

| Champ | Type | Obligatoire | Description |
|---|---|---|---|
| `id` | str | Oui | Identifiant unique du canal (ex: `"desktop"`, `"slack"`) |
| `type` | str | Oui | Type de canal : `"desktop"` ou `"webhook"` |
| `enabled` | bool | Non (défaut `true`) | Si `false`, le canal est ignoré même s'il est déclaré |
| `url` | str | Oui pour `webhook` | URL HTTP POST de destination. Ignoré pour `desktop`. Erreur de démarrage si absent sur un canal `webhook` actif. |
| `events` | list[str] | Non | Sous-ensemble des événements globaux à recevoir sur ce canal. `["*"]` ou absent → hérite de la liste globale. |

**Logique de filtrage des événements par canal :**

| Valeur de `events` (canal) | Comportement |
|---|---|
| Absent (`None`) | Accepte tous les événements de la liste globale |
| `["*"]` | Accepte tous les événements de la liste globale |
| `["task.failed", "agent.degraded"]` | Accepte uniquement les événements listés (sous-ensemble) |

### Types de canaux

| Type | Implémentation | Prérequis |
|---|---|---|
| `desktop` | Notification native OS via `notify-rust` | macOS (NSUserNotification) ou Linux (libnotify ≥ 0.7.9 + session graphique) |
| `webhook` | Requête HTTP POST vers l'URL configurée | Réseau accessible depuis la machine |

**Payload webhook (HTTP POST, `Content-Type: application/json`) :**

```json
{
  "event":   "task.input_required",
  "severity": "warning",
  "message":  "La tâche abc123 attend une réponse humaine",
  "timestamp": "2026-03-09T08:00:00Z"
}
```

### Dégradation gracieuse

Le runtime **ne s'arrête jamais** à cause d'un échec de notification :

- **`desktop` sans session graphique** — `notify-rust` retourne `Ok(())` silencieusement, l'événement est consommé sans erreur.
- **Webhook en timeout ou URL inaccessible** — `WARN` loggé (`apollia_notifications — webhook "slack" failed: ...`), le runtime continue.
- **Canal `webhook` sans `url`** — erreur de démarrage détectée au parsing TOML (fail-fast, Principe #4). Le runtime refuse de démarrer avec un message explicite.
- **Section `[notifications]` absente** — `INFO` loggé (`Supervisor: aucune section [notifications] — NotificationEngine désactivé`), aucun canal n'est instancié.

### Exemple — Desktop uniquement

```toml
[notifications]
events = ["task.input_required", "task.failed"]

[[notifications.channels]]
id      = "bureau"
type    = "desktop"
enabled = true
```

### Exemple — Multi-canaux avec filtrage fin

```toml
[notifications]
events = ["task.input_required", "task.failed", "task.completed", "agent.degraded"]

# Canal desktop — uniquement les événements urgents
[[notifications.channels]]
id      = "bureau"
type    = "desktop"
enabled = true
events  = ["task.input_required", "task.failed", "agent.degraded"]

# Canal Slack — tous les événements globaux
[[notifications.channels]]
id      = "slack"
type    = "webhook"
enabled = true
url     = "https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXX"

# Canal monitoring interne — uniquement les erreurs
[[notifications.channels]]
id      = "alertmanager"
type    = "webhook"
enabled = true
url     = "http://localhost:9093/api/v2/alerts"
events  = ["task.failed", "agent.degraded", "llm.backend_down", "trigger.error"]
```

---

## Voir aussi

- [INSTALL.md](./INSTALL) — installation et prérequis
- [INSTALL Production](./INSTALL-Production) — déploiement en production
- [Ops Exploitation et Debug](./Ops-Exploitation-et-Debug) — monitoring et debug
