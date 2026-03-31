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

## 7. Décisions architecturales

Voir [ADR-044](./Decisions-Log#adr-044--client-mcp--architecture-transport-lifecycle) pour la justification complète.

| Décision | Raison |
|---|---|
| Crate `apollia-mcp` dédiée (pas dans `apollia-tools`) | Responsabilité unique — subprocess lifecycle + protocole réseau orthogonal aux outils Rust purs |
| Transport stdio uniquement en V1 | Local-first : ~90 % des serveurs MCP communautaires sont stdio ; pas de réseau distant sans action explicite |
| Implémentation native JSON-RPC 2.0 | Principe #2 — zéro SDK MCP tiers dans le binaire |
| `McpClientManager` comme acteur Tokio | Principe #5 — zéro état partagé, toutes les mutations via channel `mpsc` |
| `McpConfigWriter` séparé du manager | Séparation I/O disque / état runtime — le writer est synchrone, le manager ne touche jamais le disque |
| `McpToolExecutor` implémente `ToolExecutor` | Les outils MCP passent par le même `ToolDispatcher` que les natifs — ajout sans modifier le chemin d'exécution |

---

## Voir aussi

- [MCP — Guide utilisateur](./MCP-Guide-Utilisateur) — configuration `mcp.toml`, exemples, troubleshooting
- [MCP — Intégration](./MCP-Integration) — alignement Apollia OS ↔ standard MCP
- [Briques Tool Registry](./Briques-Tool-Registry) — section 10 : outils MCP dans le registry
- [API HTTP Reference](./API-HTTP-Reference) — section MCP : `/api/v1/mcp/*`
