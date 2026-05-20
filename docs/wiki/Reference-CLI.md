# `apollia-os` — référence CLI

> Référence technique exhaustive, complémentaire au [book pédagogique][book].
> Snapshot **2026-05-21** (sprint CLI-parity).
>
> Tous les exemples supposent que le binaire `apollia-os` est sur le `PATH` et
> que le daemon est démarré (sauf mention "local-first").

## Sommaire

- [Flags globaux](#flags-globaux)
- [Codes de sortie](#codes-de-sortie)
- [Niveau 1 : start · stop · status · run · chat](#niveau-1)
- [Niveau 2 : agent · task · tools · permissions · memory · audit · stt · notify · llm · model · trigger · mcp · workspace · auth · update · review · onboard · plan-cache · resilience · rollback](#niveau-2)
- [Niveau 3 : doctor · logs · version · digest · trace · hitl · config · connector · project · user-memory · chat-config](#niveau-3)
- [Liens vers les ADR](#decisions)

---

## Flags globaux

| Flag | Description |
|---|---|
| `--socket <PATH>` | Chemin du socket Unix (défaut `/tmp/apollia.sock`). |
| `--json` | Sortie JSON sur stdout (désactive couleurs et progress). |
| `-q, --quiet` | Réduit l'output au strict succès/erreur. |
| `-v, --verbose` | Détails supplémentaires (durées, steps, etc.). |
| `--debug` | Logs internes + traces ORIA sur stderr (`RUST_LOG=debug`). |
| `--no-color` | Désactive les codes ANSI. |
| `-V, --version` | Affiche la version du binaire. |
| `-h, --help` | Affiche l'aide. |

## Codes de sortie

| Code | Sémantique |
|---|---|
| 0 | Succès |
| 1 | Erreur générale (usage, input invalide) |
| 2 | Runtime indisponible (connexion refusée) |
| 3 | Tâche échouée |
| 4 | Timeout |
| 5 | Interrompu (Ctrl-C) |

---

## Niveau 1

### `start`
Démarre le runtime en foreground.
- `--port <PORT>` (défaut 7771)

### `stop`
Arrête un runtime en cours d'exécution.

### `status`
Affiche l'état runtime + liste des agents.

### `run <agent_id> <input>`
Soumet une tâche et attend le résultat.
- `--stream` : streaming SSE.
- `--detach` : fire-and-forget, retourne le task_id.
- `--alternatives` : choix interactif entre deux plans.
- `--allowed-tools T1,T2` : whitelist outils session.
- `--disallowed-tools T1,T2` : blacklist (prioritaire).

### `chat [--resume <SESSION_ID>] [--list]`
REPL interactif (rustyline + historique `~/.apollia/repl_history`). Slash commands : `/fork`, `/fork N`, `/list-commands`, custom dans `.apollia/commands/*.md`.

---

## Niveau 2

### `agent <list|start|stop|info|install|uninstall|enable|disable|update|new|logs|validate|package>`
Lifecycle complet d'agents. `agent install <path|git_url> [--skip-tests]`, `agent new <name> --type react|conversational|orchestrated`.

### `task <list|status|cancel|inspect|resume|approvals>`
- `task list [--pending-approval]`
- `task resume <id> --approve | --reject [--reason TEXT]`
- `task inspect <id>` : plan d'exécution local (lit `~/.apollia/plans.db`).

### `tools <list|enable|disable|config|reload|credentials|describe|approvals>`
- `tools config get|set <KEY_PATH> [VALUE]`
- `tools credentials list|set|delete|test`
- `tools approvals pending|resolved [--days N]`

### `permissions <list|revoke|audit>`
Règles `governance.db`. `permissions list [--scope global|project] [--tool TOOL]`, `permissions revoke <id> [--yes]`, `permissions audit [--tool T] [--limit N]`.

### `memory <inspect|list|clear|purge|learn-procedure|export|import>`
Opérations local-first (pas de runtime requis) sur `~/.apollia/memory/`.

### `audit <list|stats|export>`
- `audit list [--limit N]`
- `audit stats`
- `audit export [--output FILE] [--limit N]` : dump JSON complet.

### `stt <status|transcribe|transcriptions|model|config>`
STT engine + transcription d'audio + gestion des modèles whisper.

### `notify <test|list|logs|create|update|delete|events>`
Canaux de notification + types d'événements.

### `llm <status|ping|chat|costs|backends>`
- `llm backends list|create|update|delete|set-default`
- `llm costs` : usage agrégé.

### `model <list|search|show|hardware|delete>`
- `model search <query> [--limit N]` : registry HF via le runtime.
- `model show <org>/<repo>` : metadata + files.
- `model hardware` : profil RAM/CPU/GPU détecté.
- `model delete <name> --confirm` : suppression locale.

### `trigger <list|status|fire|enable|disable|logs|reload|create|update|delete>`
CRUD complet de triggers.

### `mcp <list|add|remove|get|test|restart|update|raw-config|set-approval|list-pending|revoke-approval>`
Gestion des serveurs MCP + queue d'approbations HITL.

### `workspace <status|init>`
Inspection du workspace + génération `APOLLIA.md`.

### `auth <login|status|logout>`
Flow OAuth2 PKCE pour providers LLM (Anthropic, OpenAI, Vertex).

### `update [--check]`
Auto-updater depuis GitHub Releases.

### `review <path>`
Code review via l'agent `apollia-review`.

### `onboard [--topic identity|preferences|tools|domain|agents]`
Onboarding conversationnel.

### `plan-cache <stats|clear|evict>`
Cache ORIA des plans d'exécution.

### `resilience <list|show|reset>`
Circuit breakers.

### `rollback <session_id> [--dry-run] [--list]`
Inverse les mutations fs persistées dans le journal.

### `mcp-server [--with-runtime]`
Lance Apollia en mode serveur MCP stdio (pour Claude Desktop / Cursor / VS Code).

---

## Niveau 3

### `doctor`
Diagnostique local : `~/.apollia/`, `apollia.toml`, `governance.db`, `agents.db`, modèles, PyO3, socket. Exit 0 si OK, 1 si erreur. `--json` pour rapport machine-readable.

### `logs [--file PATH] [--last N] [-f|--follow]`
Tail du fichier de logs runtime (`~/.apollia/logs/runtime.log` par défaut).

### `version [--json]`
Version + build profile + features actives.

### `digest [--since 24h|7d|30d]`
Snapshot agrégé : tâches récentes, coûts LLM, statistiques d'audit.

### `trace <task_id> [--format human|json]`
Trace event-sourced d'une tâche (ADR-088).

### `hitl`
Alias pour `task list --pending-approval`.

### `config <get|set|validate|edit|show>`
Gestion globale de `apollia.toml` :
- `config get [KEY_PATH]` : valeur ou contenu complet.
- `config set <KEY_PATH> <VALUE>` : écriture in-place (préserve commentaires).
- `config validate` : parse + reporting d'erreur.
- `config edit` : ouvre `$EDITOR`.
- `config show` : sortie JSON du parse.

### `connector <list|accounts|test|revoke>`
Connecteurs SaaS natifs (Google Workspace, Microsoft 365).
- `connector list` : enumère les connectors disponibles + leurs services.
- `connector accounts [--provider P]` : comptes liés au keyring multi-account.
- `connector test <provider> <account>` : health check (userinfo round-trip + scopes).
- `connector revoke <provider> <account> --confirm` : supprime le token local.

### `project <list|create|show|update|delete|agents|templates>`
Local-first sur `~/.apollia/projects.db`.
- `project create <name> [--description] [--instructions] [--workspace DIR]`
- `project agents <list|add|remove> <project> <agent>`
- `project templates <list|seed-builtins>`

### `user-memory <show|set|forget|reset|schema|export|import>`
Profil utilisateur global (`~/.apollia/user_memory.db`).
- `user-memory set <KEY> <VALUE>` : écrit en tant que User.
- `user-memory export [--output FILE]` / `import --input FILE [--overwrite]`.

### `chat-config <get|set|reset>`
Configuration du Chat Libre (`governance.db`).
- `chat-config set system-prompt "Tu es ..."`
- `chat-config set allowed-tools file_read,bash,http`
- `chat-config set llm-backend anthropic | none`

---

## Decisions

Les choix architecturaux derrière le CLI sont tracés dans :
- ADR-008 — pattern noun-verb
- ADR-064 — OAuth2 PKCE + keyring
- ADR-088 — trace event-sourced
- ADR-098 — SDK Python rebuild (impacte `agent install` / `agent new`)
- [docs/wiki/Briques-CLI.md][briques] — spec d'origine
- [docs/internal/release/CLI-MATRIX.md][matrix] — parité Desktop / CLI

[book]: ../../book/src
[briques]: ./Briques-CLI.md
[matrix]: ../internal/release/CLI-MATRIX.md
