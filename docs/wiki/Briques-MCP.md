# Client MCP — apollia-mcp

> *Spécification de la crate `apollia-mcp` : client MCP natif, transport stdio, cycle de vie des sessions, et intégration dans le Tool Registry.*

---

## 1. Rôle dans l'architecture

La crate `apollia-mcp` est le **client MCP d'Apollia OS**. Elle connecte le Tool Registry aux serveurs MCP externes : tout processus tiers (Node.js, Python, binaire natif) qui implémente le Model Context Protocol peut être consommé depuis un agent sans aucun code d'intégration supplémentaire.

**Responsabilités :**
- Charger les configurations serveurs depuis `~/.apollia/mcp.db` (`McpServerRepository`) et gérer le cycle de vie des processus
- Implémenter le protocole JSON-RPC 2.0 + MCP (initialize, tools/list, tools/call)
- Enregistrer les outils découverts dans le `ToolRegistryHandle` sous la convention `mcp:{server}/{tool}`
- Appliquer la gate HITL avant chaque exécution si `requires_approval = true`
- Exposer un acteur Tokio (`McpClientManager`) pour les mutations à chaud (ajout, suppression, redémarrage)
- Persister toutes les configurations via `McpServerRepository` (SQLite, Sprint 28)

---

## 2. Structure de la crate

```
crates/apollia-mcp/src/
├── lib.rs                ← exports publics : McpClientManagerHandle, McpToolExecutor, McpConfig, McpServerConfig, McpServerRepository
├── config.rs             ← McpConfig, McpServerConfig, interpolation ${VAR}
├── server_repository.rs  ← McpServerRepository : SQLite CRUD (save/list/find_by_name/delete/set_enabled/import_from_toml)
├── jsonrpc.rs            ← JsonRpcRequest, JsonRpcResponse, JsonRpcError — JSON-RPC 2.0
├── protocol.rs           ← McpInitializeParams, McpToolsListResult, McpToolDefinition, McpCallResult
├── session.rs            ← McpSession : spawn subprocess, stdin writer task, stdout reader task, corrélation requête/réponse
├── manager.rs            ← McpClientManager (acteur Tokio), McpClientManagerHandle, McpServerStatus
└── executor.rs           ← McpToolExecutor : implémente ToolExecutor, gate HITL, acheminement via manager
```

---

## 3. Types publics

### `McpServerConfig`

Type partagé entre le dépôt SQLite et le gestionnaire de sessions :

```rust
/// Configuration d'un serveur MCP — persistée dans mcp.db (table mcp_servers).
pub struct McpServerConfig {
    pub name: String,                    // identifiant unique — "notion", "sqlite" ([a-z0-9_-]+)
    pub command: String,                 // exécutable : "npx", "uvx", binaire
    pub args: Vec<String>,               // arguments de la commande
    pub env: HashMap<String, String>,    // variables d'environnement (valeurs interpolées ${VAR})
    pub transport: String,               // "stdio" | "streamable-http" | "sse"
    pub url: Option<String>,             // URL du serveur distant (transport HTTP/SSE uniquement)
    pub requires_approval: bool,         // gate HITL sur tous les outils du serveur
    pub init_timeout_secs: u64,          // défaut 30 — handshake initialize
    pub call_timeout_secs: u64,          // défaut 60 — tools/call
    pub tags: Vec<String>,               // tags propagés aux ToolDescriptor
}
```

Les valeurs du champ `env` supportent deux syntaxes d'interpolation résolues par `resolve_env()` au chargement :

| Syntaxe | Source | Exemple |
|---|---|---|
| `${NOM_VAR}` | Variable d'environnement shell | `${NOTION_API_KEY}` |
| `${APOLLIA_SECRET:NOM_VAR}` | OS Keychain (via crate `keyring`, Sprint 27) | `${APOLLIA_SECRET:NOTION_API_KEY}` |

Pour `APOLLIA_SECRET:`, la clé lue dans le keychain suit le format `{server_name}:{nom_var}`. Une variable absente (shell ou keychain) retourne une erreur de configuration explicite. Les secrets sont écrits dans le keychain par la page Intégrations du desktop — voir [Guide Intégrations](./Integrations-Guide).

### `McpServerRepository` *(Sprint 28)*

SQLite-backed repository pour la persistance des `McpServerConfig`. Remplace `McpConfigWriter` (supprimé).

```rust
pub struct McpServerRepository { /* conn: Connection (WAL mode) */ }

impl McpServerRepository {
    /// Ouvre mcp.db et applique le schéma (idempotent).
    pub fn open(path: &Path) -> Result<Self, McpRepoError>;

    /// Insère ou remplace une configuration.
    pub fn save(&self, config: &McpServerConfig) -> Result<(), McpRepoError>;

    /// Retourne tous les serveurs (actifs et désactivés).
    pub fn list(&self) -> Result<Vec<McpServerConfig>, McpRepoError>;

    /// Retourne un serveur par nom, ou None.
    pub fn find_by_name(&self, name: &str) -> Result<Option<McpServerConfig>, McpRepoError>;

    /// Supprime un serveur.
    pub fn delete(&self, name: &str) -> Result<(), McpRepoError>;

    /// Active ou désactive un serveur sans le supprimer.
    pub fn set_enabled(&self, name: &str, enabled: bool) -> Result<(), McpRepoError>;

    /// Importe depuis une liste existante — no-op si la table est non-vide.
    pub fn import_from_toml(&self, configs: Vec<McpServerConfig>) -> Result<usize, McpRepoError>;
}
```

**Schéma :** `mcp.db` (table `mcp_servers`) — colonnes `name`, `command`, `args_json`, `env_json`, `transport`, `url`, `requires_approval`, `init_timeout_secs`, `call_timeout_secs`, `tags_json`, `enabled`, `created_at`, `updated_at`.

**Migration depuis mcp.toml :** `import_from_toml()` est une migration one-shot : si la table est déjà peuplée, elle retourne `Ok(0)` sans rien modifier.

---

### `McpSession`

Gère une connexion unique à un processus serveur MCP :

```rust
impl McpSession {
    /// Démarre le subprocess et effectue le handshake initialize/initialized.
    pub async fn spawn(config: &McpServerConfig) -> Result<Self, McpSessionError>;

    /// Envoie tools/list et retourne les définitions d'outils.
    pub async fn list_tools(&mut self) -> Result<Vec<McpToolDefinition>, McpSessionError>;

    /// Envoie tools/call et retourne le résultat MCP.
    pub async fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: Value,
    ) -> Result<McpCallResult, McpSessionError>;

    /// Ferme proprement le subprocess (SIGTERM + wait avec timeout).
    pub async fn shutdown(self) -> Result<(), McpSessionError>;
}
```

**Architecture interne :** deux tâches Tokio par session — `stdin_writer_task` consomme un channel `mpsc` de requêtes JSON-RPC, `stdout_reader_task` lit les réponses ligne par ligne et les route vers les `oneshot` correspondants par corrélation d'`id`.

### `McpClientManager` (acteur Tokio)

Hub central, pattern acteur strict — zéro état partagé. Toutes les mutations passent par un channel `mpsc` :

```rust
/// Handle clonable vers l'acteur McpClientManager.
pub struct McpClientManagerHandle {
    tx: mpsc::Sender<McpManagerCommand>,
}

impl McpClientManagerHandle {
    /// Démarre l'acteur et charge tous les serveurs depuis mcp.toml.
    pub async fn start(
        config: McpConfig,
        tool_registry: ToolRegistryHandle,
    ) -> Result<Self, McpManagerError>;

    /// Retourne le statut de tous les serveurs connectés.
    pub async fn list_servers(&self) -> Vec<McpServerStatus>;

    /// Retourne le statut détaillé d'un serveur (outils inclus).
    pub async fn get_server(&self, name: &str) -> Option<McpServerDetail>;

    /// Ajoute un serveur à chaud et l'enregistre dans le registry.
    pub async fn add_server(&self, config: McpServerConfig) -> Result<McpServerStatus, McpManagerError>;

    /// Supprime un serveur, arrête sa session, et retire ses outils du registry.
    pub async fn remove_server(&self, name: &str) -> Result<(), McpManagerError>;

    /// Redémarre la session d'un serveur existant.
    pub async fn restart_server(&self, name: &str) -> Result<McpServerStatus, McpManagerError>;

    /// Arrête toutes les sessions proprement (appelé au graceful shutdown).
    pub async fn shutdown(self) -> Result<(), McpManagerError>;
}
```

### `McpServerStatus`

```rust
pub struct McpServerStatus {
    pub name: String,
    pub server_info: Option<String>,   // version retournée par initialize
    pub tools_count: usize,
    pub requires_approval: bool,
    pub connected: bool,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub last_call_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub package: Option<String>,
    pub transport: String,
}
```

### `McpToolExecutor`

Implémente le trait `ToolExecutor` de `apollia-tools` — les outils MCP sont indiscernables des outils natifs pour le `ToolDispatcher` :

```rust
impl ToolExecutor for McpToolExecutor {
    fn tool_name(&self) -> &'static str;

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError>;
}
```

`execute` :
1. Vérifie `requires_approval` (serveur) ou `tools_requiring_approval` (agent) — si actif : crée une `PendingApproval` et suspend.
2. Sérialise `input` comme `arguments` du `tools/call` JSON-RPC.
3. Achemine via `McpClientManagerHandle`.
4. Retourne le `content` de la réponse MCP.

### `McpConfigWriter`

Gère les mutations de `mcp.toml` (I/O disque, séparé du manager) :

```rust
impl McpConfigWriter {
    pub fn new(path: PathBuf) -> Self;
    pub fn add_server(&self, config: &McpServerConfig) -> Result<(), McpConfigWriteError>;
    pub fn remove_server(&self, name: &str) -> Result<(), McpConfigWriteError>;
    pub fn update_server(&self, name: &str, config: &McpServerConfig) -> Result<(), McpConfigWriteError>;
}
```

Chaque méthode : lit → modifie en mémoire → valide (unicité des noms, champs requis) → réécrit. L'ordre des serveurs est préservé par `update_server`. Les commentaires TOML ne sont pas préservés (TOML roundtrip via serde_toml).

---

## 4. Protocole MCP — flux

### Handshake (STORY-330)

```
Runtime → Serveur :  {"jsonrpc":"2.0","id":"1","method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"apollia-mcp","version":"0.1.0"}}}
Runtime ← Serveur :  {"jsonrpc":"2.0","id":"1","result":{"protocolVersion":"2024-11-05","capabilities":{},"serverInfo":{"name":"notion-mcp-server","version":"1.0.0"}}}
Runtime → Serveur :  {"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
```

### Découverte des outils (STORY-331)

```
Runtime → Serveur :  {"jsonrpc":"2.0","id":"2","method":"tools/list","params":{}}
Runtime ← Serveur :  {"jsonrpc":"2.0","id":"2","result":{"tools":[{"name":"search","description":"...","inputSchema":{...}}]}}
```

### Exécution d'un outil (STORY-332)

```
Runtime → Serveur :  {"jsonrpc":"2.0","id":"3","method":"tools/call","params":{"name":"search","arguments":{"query":"rapport Q1"}}}
Runtime ← Serveur :  {"jsonrpc":"2.0","id":"3","result":{"content":[{"type":"text","text":"..."}]}}
```

---

## 5. Intégration dans le runtime

### Phase 3b — Supervisor startup (STORY-335)

`McpClientManagerHandle` est démarré après `ToolRegistryHandle` (Phase 3) :

```rust
// Supervisor::start — Phase 3b
let mcp_config = McpConfig::load_from(&data_dir.join("mcp.toml")).unwrap_or_default();
let mcp_manager = McpClientManagerHandle::start(mcp_config, tool_registry.clone()).await?;
// Les outils MCP sont maintenant dans le ToolRegistry
```

### Graceful shutdown (STORY-338)

Au `SIGTERM` ou `POST /api/v1/shutdown`, le Supervisor appelle `mcp_manager.shutdown()` avant d'arrêter le runtime. Chaque session envoie `SIGTERM` à son subprocess et attend 5 secondes avant `SIGKILL`. Garantit zéro processus zombie.

---

## 6. Erreurs

```rust
pub enum McpSessionError {
    SpawnFailed { reason: String },
    HandshakeTimeout { server: String, secs: u64 },
    ProtocolViolation { message: String },
    CallTimeout { tool: String, secs: u64 },
    Disconnected { server: String },
    JsonRpcError { code: i32, message: String },
}

pub enum McpManagerError {
    ServerNotFound(String),
    ServerAlreadyExists(String),
    SessionError(McpSessionError),
    ConfigError(McpConfigError),
}

pub enum McpConfigWriteError {
    IoError(String),
    ParseError(String),
    DuplicateName(String),
    ServerNotFound(String),
}
```

---

## 7. Transports HTTP/SSE *(ADR-046)*

Sprint 27 (ADR-046) introduit une architecture de **transport abstrait** pour connecter des serveurs MCP distants. La crate expose le trait `McpTransport` avec trois implémentations selectionnées dynamiquement selon le champ `transport` de `McpServerConfig`.

### Trait McpTransport

```rust
// crates/apollia-mcp/src/transport/mod.rs
#[async_trait]
pub trait McpTransport: Send + Sync + 'static {
    /// Envoie un message JSON-RPC (le transport ajoute le \n).
    async fn send(&self, message: &str) -> Result<(), TransportError>;
    /// Attend le prochain message JSON-RPC depuis le serveur.
    async fn recv(&self) -> Result<String, TransportError>;
    /// Ferme proprement la connexion.
    async fn shutdown(&self) -> Result<(), TransportError>;
    /// PID du processus serveur (None pour les transports réseau).
    fn pid(&self) -> Option<u32> { None }
}
```

### StdioTransport

Transport historique : spawn d'un subprocess, stdin/stdout pipes.

- Spawn : `tokio::process::Command` avec `stdin(Stdio::piped())` + `stdout(Stdio::piped())`
- Une tâche `stdin_writer_task` consomme un channel `mpsc` et écrit vers stdin
- Une tâche `stdout_reader_task` lit ligne par ligne et route vers les `oneshot` de corrélation
- `shutdown()` : envoie `SIGTERM`, attend 5 secondes, puis `SIGKILL`
- `pid()` : retourne le PID du subprocess

### StreamableHttpTransport *(ADR-046)*

Fichier : `crates/apollia-mcp/src/transport/http.rs`

Implémente le protocole MCP *Streamable HTTP* : **un POST HTTP par requête/réponse**.

```
Client → POST {url}  Content-Type: application/json
                     Accept: application/json, text/event-stream
                     Mcp-Session-Id: {session-id}   ← après le 1er échange
                     Authorization: Bearer {token}   ← si configuré
              body:  {"jsonrpc":"2.0","id":1,"method":"tools/call",...}

Server → 200 OK      Mcp-Session-Id: abc123          ← extrait au 1er échange
              body:  {"jsonrpc":"2.0","id":1,"result":{...}}
```

**Caractéristiques :**
- **Lazy** : aucune connexion ouverte avant le premier `send()` — construction synchrone, réseau différé
- **Session affinity** : le header `Mcp-Session-Id` reçu dans la première réponse est mémorisé et renvoyé sur toutes les requêtes suivantes
- **Timeout** : `call_timeout_secs` de la config (défaut 60s), appliqué par requête
- **Auth** : les `env` résolus sont injectés comme headers HTTP (ex. `Authorization: Bearer …`)
- **shutdown()** : no-op — HTTP n'a pas de connexion persistante à fermer
- **Erreurs** : `TransportError::Io("HTTP 401")` pour code non-2xx, `TransportError::Io("timeout")` si dépassement

### SseTransport *(ADR-046)*

Fichier : `crates/apollia-mcp/src/transport/sse.rs`

Connexion **SSE persistante** en lecture + HTTP POST en écriture.

```
Construction :
  Client → GET {sse_url}      ← connexion persistante ouverte en tâche background
  Server → event: endpoint\n
           data: {post_url}\n ← URL de POST extraite par la tâche SSE

Premier send() :
  Client → POST {post_url}    Content-Type: application/json
                              Authorization: Bearer {token}  ← si configuré

Réponses :
  Server → data: {"jsonrpc":"2.0","id":1,"result":{...}}\n ← sur le flux SSE
```

**Caractéristiques :**
- **Background task** : une tâche Tokio ouverte au moment de la construction (`SseTransport::new()`) consomme le flux SSE et transfère les réponses via `mpsc::channel(64)`
- **Endpoint discovery** : `send()` attend (jusqu'à `timeout`) que la tâche SSE ait reçu l'événement `endpoint` avant d'envoyer le POST
- **Reconnection** : **non implémentée** — si la connexion SSE se coupe, `recv()` retourne `TransportError::Closed`. L'opérateur doit déclencher un `restart_server` explicite
- **Shutdown** : `shutdown_tx.send(true)` signale la tâche background de s'arrêter
- **Auth** : headers injectés sur le GET initial ET sur chaque POST

---

## 8. Sélection du transport

La fonction factory `create_transport(config, resolved_env)` (`transport/mod.rs`) dispatche sur `config.transport` :

| Valeur `transport` | Implémentation | Champ `url` requis | Processus local |
|---|---|---|---|
| `"stdio"` | `StdioTransport` | Non | Oui (subprocess) |
| `"streamable-http"` | `StreamableHttpTransport` | Oui | Non |
| `"sse"` | `SseTransport` | Oui | Non |
| autre valeur | `TransportError::Unsupported` | — | — |

**Règles de configuration :**

```json
// Serveur local stdio (npm/pip)
{
  "name": "filesystem",
  "command": "npx",
  "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home/user/docs"],
  "transport": "stdio"
}

// Serveur distant HTTP (ex. Notion MCP officiel)
{
  "name": "notion",
  "command": "",
  "args": [],
  "transport": "streamable-http",
  "url": "https://mcp.notion.com/mcp",
  "env": { "Authorization": "Bearer ${APOLLIA_SECRET:NOTION_API_KEY}" }
}

// Serveur distant SSE
{
  "name": "brave-search",
  "command": "",
  "args": [],
  "transport": "sse",
  "url": "https://api.search.brave.com/sse",
  "env": { "Authorization": "Bearer ${BRAVE_API_KEY}" }
}
```

Pour les transports réseau (`streamable-http`, `sse`), les entrées de `resolved_env` sont transmises comme **headers HTTP** sur chaque requête. La résolution `${VAR}` et `${APOLLIA_SECRET:VAR}` est effectuée avant l'appel à `create_transport()`.

---

## 9. Error recovery

### Comportement au démarrage

Un serveur qui échoue à démarrer (spawn, handshake timeout, erreur de protocole) est **loggué au niveau `error` et ignoré** — les autres serveurs continuent. L'acteur `McpClientManager` ne s'arrête pas sur un seul serveur défaillant.

```
tracing::error!(server = %name, error = %e, "MCP server failed to start, skipping");
```

### Erreurs de session

| Erreur | Cause | Surface |
|---|---|---|
| `InitializeTimeout { timeout_secs }` | Handshake non terminé dans `init_timeout_secs` (défaut 30s) | Démarrage |
| `ToolCallTimeout { timeout_secs }` | `tools/call` non terminé dans `call_timeout_secs` (défaut 60s) | Exécution |
| `ServerExited` | Subprocess terminé prématurément ou canal transport fermé | Exécution |
| `JsonRpcError { code, message }` | Le serveur a retourné un objet d'erreur JSON-RPC | Exécution |
| `ToolCallFailed { cause }` | Erreur côté serveur lors de l'exécution de l'outil | Exécution |
| `InitializeFailed { cause }` | Réponse `initialize` malformée ou protocole non supporté | Démarrage |

### Reconnexion manuelle

Il n'y a **pas de reconnexion automatique** : si une session meurt après démarrage, elle reste déconnectée jusqu'à intervention explicite. L'opérateur dispose de deux mécanismes :

```bash
# Via CLI
$ apollia-os mcp restart notion

# Via API REST
$ curl -X POST http://localhost:7771/api/v1/mcp/servers/notion/restart
```

`restart_server()` dans le `McpClientManagerHandle` :
1. Appelle `shutdown()` sur la session existante (SIGTERM + attente pour stdio, no-op pour HTTP/SSE)
2. Spawne une nouvelle `McpSession` avec la même configuration
3. Re-enregistre les outils découverts dans le `ToolRegistry`
4. Retourne le nouveau `McpServerStatus`

### Timeout per-request vs per-session

Les timeouts sont configurés **par serveur** dans `McpServerConfig`, pas globalement :

| Champ | Défaut | Scope |
|---|---|---|
| `init_timeout_secs` | 30s | Handshake `initialize` uniquement |
| `call_timeout_secs` | 60s | Chaque `tools/call` individuellement |

Pour les transports HTTP et SSE, `call_timeout_secs` est transmis au `reqwest::Client` comme timeout de requête.

---

## 10. Convention de nommage : mcp:{server}/{tool}

Tous les outils MCP sont enregistrés dans le `ToolRegistry` sous la convention `mcp:{server_name}/{tool_name}`.

### Garantie d'unicité

Le séparateur `/` garantit qu'il n'y a jamais de collision entre :
- deux serveurs différents exposant un outil de même nom
- un outil MCP et un outil natif (les natifs n'ont jamais le préfixe `mcp:`)

```
mcp:notion/search_pages          ← serveur "notion", outil "search_pages"
mcp:brave-search/web_search      ← serveur "brave-search", outil "web_search"
mcp:notion/create_page           ← même serveur "notion", outil différent
```

### Construction et décomposition

**Construction** (à l'enregistrement dans le ToolRegistry) :
```rust
// manager.rs
let full_name = format!("mcp:{}/{}", server_name, tool_def.name);
```

**Décomposition** (au routage d'un appel) :
```rust
// executor.rs
pub fn parse_tool_name(name: &str) -> Option<(&str, &str)> {
    let stripped = name.strip_prefix("mcp:")?;
    let slash = stripped.find('/')?;
    Some((&stripped[..slash], &stripped[slash + 1..]))
}
// "mcp:notion/search_pages" → Some(("notion", "search_pages"))
// "bash_executor"           → None  (outil natif, routage différent)
```

`parse_tool_name()` retourne `None` si :
- pas de préfixe `mcp:`
- pas de séparateur `/`
- segment serveur ou outil vide

### Enregistrement en conflit

Si deux outils produisent le même `full_name` (impossible par construction, mais possible si deux serveurs ont le même `name`), le second `tool_registry.register()` logue un avertissement et l'enregistrement échoue silencieusement. La contrainte d'unicité du nom de serveur est portée par `McpServerRepository` (UNIQUE sur `name` en SQLite).

---

## 11. Décisions architecturales

Voir [ADR-044](./Decisions-Log#adr-044--client-mcp--architecture-transport-lifecycle) et [ADR-046](../adr/ADR-046-transport-http-sse-mcp.md) pour les justifications complètes.

| Décision | Raison |
|---|---|
| Crate `apollia-mcp` dédiée (pas dans `apollia-tools`) | Responsabilité unique — subprocess lifecycle + protocole réseau orthogonal aux outils Rust purs |
| Transport stdio uniquement en V1, HTTP/SSE en Sprint 27 | Local-first ; HTTP/SSE ajoutés quand ~70% du MCP Registry a migré vers les remotes (ADR-046) |
| Implémentation native JSON-RPC 2.0 | Principe #2 — zéro SDK MCP tiers dans le binaire |
| `McpClientManager` comme acteur Tokio | Principe #5 — zéro état partagé, toutes les mutations via channel `mpsc` |
| Trait `McpTransport` au lieu de type enum | Architecture extensible, facilite le testing (mock transport), aligne avec l'enum dans `apollia-tools` |
| `McpToolExecutor` implémente `ToolExecutor` | Les outils MCP passent par le même `ToolDispatcher` que les natifs — ajout sans modifier le chemin d'exécution |
| Pas de reconnexion automatique | Principe #4 — fail fast. La reconnexion silencieuse masquerait les erreurs ; l'opérateur choisit explicitement de redémarrer |

---

## 12. Mode Serveur MCP — Sprint 36

Depuis le Sprint 36 (STORY-468, ADR-062), Apollia OS peut fonctionner en **mode serveur MCP** : en plus d'être client MCP, il expose ses outils natifs à des clients externes (Claude Desktop, VS Code Copilot Chat, Cursor).

### McpStdioServer

```rust
/// Serveur MCP exposant les outils natifs Apollia via stdio.
pub struct McpStdioServer {
    tool_registry: apollia_tools::ToolRegistry,
    runtime_handle: Option<apollia_runtime::RuntimeHandle>,
}

impl McpStdioServer {
    pub fn new(tool_registry: ToolRegistry, runtime_handle: Option<RuntimeHandle>) -> Self { ... }
    /// Boucle principale : lit stdin ligne par ligne, dispatche, écrit sur stdout.
    pub async fn run(self) -> Result<(), McpServerError> { ... }
}
```

**Transport :** stdio uniquement (pas de port réseau — Principe #1 Local-first).

**9 outils natifs exposés :** `bash_executor`, `file_read`, `file_write`, `file_edit`, `glob`, `grep`, `ls`, `mcp_client`, `agent_install`.

**10e outil conditionnel :** `submit_task` — disponible uniquement si `--with-runtime` est passé à la CLI (nécessite le runtime complet).

```bash
# Lancer le serveur MCP sans runtime
$ apollia mcp-server

# Avec l'outil submit_task activé
$ apollia mcp-server --with-runtime
```

### Requêtes JSON-RPC supportées

| Méthode | Réponse |
|---|---|
| `initialize` | `protocolVersion: "2024-11-05"`, `serverInfo.name: "apollia-os"` |
| `tools/list` | Liste des 9 (ou 10) outils avec leurs JSON Schema |
| `tools/call` | Résultat de l'exécution via `ToolRegistry` |

**Erreurs JSON-RPC :**
- `-32700` : Parse error (JSON invalide sur stdin)
- `-32000` : Outil inconnu ou erreur d'invocation

### Fichiers ajoutés

```
crates/apollia-mcp/src/
├── server.rs           ← McpStdioServer (nouveau)
├── server_types.rs     ← McpRequest, CallToolParams, InitializeParams (nouveau)
└── server_tools.rs     ← Adaptateurs des 9 outils natifs (nouveau)

crates/apollia-cli/src/commands/
└── mcp_server.rs       ← Sous-commande `apollia mcp-server` (nouveau)
```

> **Voir aussi :** [ADR-062](../adr/ADR-062-mcp-server-mode.md) — justification du transport stdio et des 9 outils

---

## Voir aussi

- [MCP — Guide utilisateur](./MCP-Guide-Utilisateur) — configuration `mcp.toml`, exemples, troubleshooting
- [MCP — Intégration](./MCP-Integration) — alignement Apollia OS ↔ standard MCP
- [Briques Tool Registry](./Briques-Tool-Registry) — section 10 : outils MCP dans le registry
- [API HTTP Reference](./API-HTTP-Reference) — section MCP : `/api/v1/mcp/*`
- [ADR-046](../adr/ADR-046-transport-http-sse-mcp.md) — décision transport HTTP/SSE pour serveurs MCP distants
