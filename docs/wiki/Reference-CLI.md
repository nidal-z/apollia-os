# `apollia-os` — référence CLI

> Référence technique exhaustive, complémentaire au [book pédagogique][book].
> Snapshot **2026-05-27** (sprint CLI v0.1.0 — rattrapage + polish).
>
> Tous les exemples supposent que le binaire `apollia-os` est sur le `PATH` et
> que le daemon est démarré (sauf mention "local-first").

## Sommaire

- [Flags globaux](#flags-globaux)
- [Codes de sortie](#codes-de-sortie)
- [Niveau 1 : start · stop · status · run · chat](#niveau-1)
- [Niveau 2 : agent · task · tools · permissions · memory · audit · stt · notify · llm · model · trigger · mcp · workspace · auth · update · review · onboard · plan-cache · resilience · rollback · mcp-server](#niveau-2)
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

### `chat [--resume <SESSION_ID>] [--list]` · sous-commandes `delete|rename|export`
REPL interactif (rustyline + historique `~/.apollia/repl_history`). Slash commands : `/fork`, `/fork N`, `/list-commands`, custom dans `.apollia/commands/*.md`.
Hygiène des sessions persistées (sans REPL) :
- `chat delete <session_id> --confirm [--db PATH]` : supprime la session + ses messages dans `~/.apollia/chat.db`.
- `chat rename <session_id> <title> [--db PATH]` : met à jour le titre user-défini.
- `chat export <session_id> --output FILE [--format markdown|json] [--db PATH]` : export pour audit (markdown par défaut, ou JSON structuré). Stdout si `--output` absent.

---

## Niveau 2

### `agent <list|start|stop|info|status|messages|install|uninstall|enable|disable|update|new|package|logs|validate|repair>`
Lifecycle complet d'agents. `agent install <path|git_url> [--skip-tests]`, `agent new <name> --type react|conversational|orchestrated`.
- `agent status <id>` : snapshot compact (état + actives/complétées + last activity), distillé pour les poll loops.
- `agent messages <id> [--limit N]` : messages A2A in-memory pour `<id>` (route `GET /api/v1/agents/:name/messages`).
- `agent logs <id> [--last N]` : fallback sur le trail d'audit filtré par `agent_id` jusqu'à ce qu'un canal de log dédié arrive en v0.1.1+. `--follow` n'est pas implémenté (message explicite, exit 1).

### `task <list|status|cancel|inspect|resume|approvals>`
- `task list [--pending-approval]`
- `task resume <id> --approve | --reject [--reason TEXT]`
- `task inspect <id>` : plan d'exécution local (lit `~/.apollia/plans.db`).

### `tools <list|enable|disable|config|reload|credentials|describe|approvals>`
- `tools config get|set <KEY_PATH> [VALUE]`
- `tools credentials list|set|delete|test`
- `tools approvals pending|resolved [--days N]`

### `permissions <list|revoke|audit|add>`
Règles `governance.db` :
- `permissions list [--scope global|project] [--tool TOOL]`
- `permissions revoke <id> [--yes]` · `permissions revoke --all --scope <global|project> [--yes]`
- `permissions audit [--tool T] [--limit N]`
- `permissions add --tool NAME [--prefix PATH] [--action allow|deny] --scope <project|global> [--project-path PATH]` : insertion batch / scripts, sans passer par le REPL HITL. Le scope `session` n'est pas persistable.

### `memory <inspect|list|clear|purge|learn-procedure|export|import|forget|search>`
Opérations local-first (pas de runtime requis) sur `~/.apollia/memory/`.
- `memory forget <namespace> <entry_id>` : suppression d'une entrée par UUID (épisodique / sémantique / procédurale + FTS5).
- `memory search <namespace> <query> [--limit N] [--source episodic|semantic]` : recherche BM25 sur l'index FTS5.

### `audit <list|stats|export>`
- `audit list [--limit N]`
- `audit stats`
- `audit export [--output FILE] [--limit N]` : dump JSON complet.

### `stt <status|transcribe|transcriptions|model|config>`
STT engine + transcription d'audio + gestion des modèles whisper.

### `notify <test|list|logs|create|update|delete|events>`
Canaux de notification + types d'événements.

### `llm <status|ping|chat|costs|backends|reload|setup>`
- `llm backends list|show|create|update|delete|set-default`
- `llm costs` : usage agrégé. `llm costs --threshold <USD>` (set) / `--get-threshold` (read) → `[llm] cost_alert_threshold_usd` dans `apollia.toml`.
- `llm setup --local --model <PATH.gguf> [--name <NAME>] [--device metal|cuda|cpu] [--system-db PATH] [--models-dir DIR]` : wizard first-run qui copie le GGUF dans `~/.apollia/models/`, l'inscrit comme backend `local` (provider `llama-cpp`) avec `is_default=true`. Lance `llm reload` après.

### `model <list|search|show|hardware|delete>`
- `model search <query> [--limit N]` : registry HF via le runtime.
- `model show <org>/<repo>` : metadata + files.
- `model hardware` : profil RAM/CPU/GPU détecté.
- `model delete <name> --confirm` : suppression locale.

### `trigger <list|status|fire|enable|disable|logs|reload|create|update|delete>`
CRUD complet de triggers — 5 kinds supportés : `cron`, `interval`, `oneshot`, `filewatch`, `webhook`.
- `trigger create <id> --agent NAME --kind KIND --detail VALUE [--on-busy queue|drop] [--input TEMPLATE]` :
  - `--detail` mappé selon le kind : expression cron / durée (`30m`, `1h`) / RFC 3339 / path / shared HMAC-SHA256 secret (≥ 32 chars validé côté CLI).
  - `--on-busy queue` (défaut) enfile les déclenchements pendant qu'une tâche tourne ; `drop` jette.
- `trigger update <id> [--detail VALUE] [--on-busy P] [--input T]` : merge avec la définition courante (kind préservé).

### `mcp <list|add|remove|get|test|restart|update|raw-config|set-approval|list-pending|revoke-approval|oauth|secret>`
Gestion des serveurs MCP + queue d'approbations HITL + secrets serveurs + flow OAuth :
- `mcp secret set <server> <env_var> <value>` / `mcp secret delete <server> <env_var>` : stockage de secrets serveurs (env vars) dans le keychain partagé `apollia-mcp` (honore `APOLLIA_TOKEN_STORAGE=file`).
- `mcp oauth login <server> [--scopes …] [--client-id ID] [--db PATH]` : flow PKCE end-to-end, browser + callback loopback, token persisté sous `apollia-mcp-oauth/<server>`.
- `mcp oauth status [<server>] [--db PATH]` : état des tokens MCP HTTP OAuth.
- `mcp oauth logout <server> --confirm` : retire le token local.
- `mcp oauth client-id set <env_var> <value>` / `clear <env_var>` : surchage le client_id par-env-var en keychain (`apollia-mcp-client-ids`).
- `mcp oauth discover <server> [--db PATH]` : RFC 9728 + RFC 8414 discovery contre le serveur. Read-only, pas d'échange de token.

**Déférés v0.1.1** : `mcp catalogue` (browse registry) et `mcp enrichments list` — backend dans `apollia-desktop`, cross-crate refactor requis.

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

### `config <get|set|validate|edit|show|reset>`
Gestion globale de `apollia.toml` :
- `config get [KEY_PATH]` : valeur ou contenu complet.
- `config set <KEY_PATH> <VALUE>` : écriture in-place (préserve commentaires).
- `config validate` : parse + reporting d'erreur.
- `config edit` : ouvre `$EDITOR`.
- `config show` : sortie JSON du parse.
- `config reset --confirm [--dry-run] [--home PATH]` : factory reset. Wipe les enfants de `~/.apollia/` (databases, journals, models, configs). Les entrées keychain OS ne sont **pas** touchées — utiliser `auth logout`, `connector revoke`, `mcp oauth logout` pour les nettoyer.

### `connector <list|accounts|test|revoke|client-id|client-secret|api-key|drive>`
Connecteurs SaaS natifs (Google Workspace, Microsoft 365).
- `connector list` : enumère les connectors disponibles + leurs services.
- `connector accounts [--provider P]` : comptes liés au keyring multi-account.
- `connector test <provider> <account>` : health check (userinfo round-trip + scopes).
- `connector revoke <provider> <account> --confirm` : supprime le token local.

**Mode Expert OAuth (power user)** — paramétrer ses propres credentials Google/Microsoft, sans toucher au binaire :
- `connector client-id list` / `set <provider> <client_id>` : override `oauth-clients.toml` (résolution `env var > file > compiled default`).
- `connector client-secret set <provider> <secret>` : idem pour le client secret (Google only).
- `connector api-key set <provider> <key>` : idem pour l'API key (Google Picker).
- `connector drive folder list` / `set <account> <path>` / `reset <account>` : override per-account du dossier Drive root.
- `connector drive folder picked list <account>` / `picked remove <account> <folder_id>` : revue/suppression des dossiers Drive captures via le Picker Desktop. **Le Picker UI lui-même reste Desktop-only.**

### `project <list|create|show|update|delete|agents|templates|link|chats>`
Local-first sur `~/.apollia/projects.db`.
- `project create <name> [--description] [--instructions] [--workspace DIR]`
- `project agents <list|add|remove> <project> <agent>`
- `project templates <list|seed-builtins>`
- `project link <project_id> --session <chat_session_id> [--unlink] [--chat-db PATH]` : attache (ou détache via `--unlink`) une session chat existante à un projet — écrit `chat_sessions.project_id` directement dans `~/.apollia/chat.db`.
- `project chats <project_id> [--chat-db PATH]` : liste les sessions chat liées à un projet.

### `user-memory <show|set|forget|reset|schema|export|import>`
Profil utilisateur global (`~/.apollia/user_memory.db`).
- `user-memory set <KEY> <VALUE>` : écrit en tant que User.
- `user-memory export [--output FILE]` / `import --input FILE [--overwrite]`.

### `chat-config <get|set|reset|permissions|authorizations>`
Configuration du Chat Libre (`governance.db`).
- `chat-config set system-prompt "Tu es ..."`
- `chat-config set allowed-tools file_read,bash,http`
- `chat-config set llm-backend anthropic | none`
- `chat-config permissions <list|delete>` : règles persistées scopées `agent_id = apollia:chat` (équivalent du panneau Desktop Settings → Chat permissions).
- `chat-config authorizations <list|revoke>` : **déféré v0.1.1**. Les autorisations vivent en mémoire du daemon et ne sont pas listables sans route HTTP runtime ; le sous-commande affiche un message explicite et exit 1 jusqu'à ce qu'elle soit câblée.

---

## Decisions

Les choix architecturaux derrière le CLI sont tracés dans :
- ADR-008 — pattern noun-verb
- ADR-064 — OAuth2 PKCE + keyring
- ADR-088 — trace event-sourced
- ADR-098 — SDK Python rebuild (impacte `agent install` / `agent new`)
- [docs/wiki/Briques-CLI.md][briques] — spec d'origine
- [docs/internal/release/CLI-STATE.md][state] — état pré-release v0.1.0 + gaps Desktop attendus

[book]: ../../book/src
[briques]: ./Briques-CLI.md
[state]: ../internal/release/CLI-STATE.md
