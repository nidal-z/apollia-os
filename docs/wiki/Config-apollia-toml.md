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
```

---

## Voir aussi

- [INSTALL.md](./INSTALL) — installation et prérequis
- [INSTALL Production](./INSTALL-Production) — déploiement en production
- [Ops Exploitation et Debug](./Ops-Exploitation-et-Debug) — monitoring et debug
