# Tool Registry - Catalogue, Sandbox, Audit Trail

> *Spécification complète du système de gestion des outils : catalogue, résolution, sandbox, et traçabilité.*

---

## 1. Rôle dans l'architecture

Le Tool Registry est la **couche d'outillage** d'Apollia OS. Il répond à 4 questions :

1. **Catalogue** : Quels outils sont disponibles pour les agents ?
2. **Résolution** : Comment valider qu'un agent dispose de tous ses outils requis au démarrage ?
3. **Injection** : Comment fournir à l'agent une interface propre vers ses outils via `RuntimeContext` ?
4. **Exécution** : Comment invoquer un outil de manière isolée et tracer cette invocation ?

---

## 2. ToolDescriptor - L'unité du catalogue

Chaque outil est décrit par un `ToolDescriptor`, aligné sur le schéma MCP :

```rust
pub struct ToolDescriptor {
    pub name: String,                // "bash_executor", "file_read", "http_fetch"
    pub version: String,             // "1.0.0" semver
    pub description: String,
    pub kind: ToolKind,
    pub input_schema: serde_json::Value,   // JSON Schema object
    pub output_schema: Option<serde_json::Value>,
    pub sandbox_profile: SandboxProfile,
    pub tags: Vec<String>,
    pub dangerous: bool,             // True = warning + log audit explicite
}

pub enum ToolKind {
    Native,                 // Implémenté en Rust dans apollia-tools
    McpServer {             // Serveur MCP externe
        server_url: String,
        transport: McpTransport,  // Stdio | Http | WebSocket
        tool_name: String,
    },
    Custom {                // Outil Python enregistré par l'utilisateur
        module_path: String,
        class_name: String,
    },
}
```

---

## 3. Outils natifs core

### 3.1 `bash_executor`

Exécute des commandes shell dans un environment isolé.

```python
# Usage depuis l'agent
result = await ctx.tools.bash_executor.run(
    command="ls -la /workspace",
    timeout=30,
    working_dir="/workspace"
)
# result.stdout, result.stderr, result.exit_code, result.duration_ms
```

**Sandbox appliquée :** PID namespace isolé, mount namespace avec répertoire temporaire dédié, réseau désactivé par défaut, cgroups (CPU 1 core max, RAM 256MB max, timeout hard 30s).

**Implémentation :** `tokio::process::Command` + `unshare(1)` pour les namespaces Linux (disponible sur tout Linux moderne avec user namespaces non-privilégiés).

### 3.2 `python_executor`

Exécute du code Python dans un virtualenv isolé par agent.

```python
result = await ctx.tools.python_executor.run(
    code="""
import json
data = {"result": 42 * 1.2}
print(json.dumps(data))
""",
    timeout=60,
    packages=["pandas"]  # Packages installés à INITIALIZING, pas ici
)
```

**Isolation :** Virtualenv dédié dans `~/.apollia/sandboxes/<agent_id>/venv/`. Les packages déclarés dans le manifest sont installés à `INITIALIZING` - une tentative d'installer un package à l'exécution échoue avec une erreur claire.

**Décision de design :** Les packages sont installés au démarrage (fail fast) et non à l'exécution pour éviter les surprises de performance et les installations silencieuses non auditées.

### 3.3 `file_read`

Lit un fichier dans le répertoire sandbox de l'agent. Supporte un offset et une limite de lignes pour les fichiers volumineux.

```python
# Lecture complète
content = await ctx.tools.file_read.run(path="config.json")
# content.text, content.line_count, content.size_bytes

# Lecture partielle avec numéros de ligne (utile pour les grands fichiers)
content = await ctx.tools.file_read.run(
    path="rapport.log",
    offset=100,   # Commence à la ligne 100
    limit=50      # Retourne 50 lignes maximum
)
# content.text contient les lignes préfixées de leur numéro : "101\tpremière ligne\n..."
```

**Sandbox :** Tout chemin est validé par `SandboxRoot` avant lecture. Une tentative de traversal (`../`) retourne une erreur `SandboxPathError::TraversalAttempted`.

### 3.4 `file_write`

Écrit un fichier dans le répertoire sandbox. Crée les répertoires parents manquants.

```python
await ctx.tools.file_write.run(
    path="output/result.json",
    content=json.dumps(data, indent=2)
)
# Crée output/ s'il n'existe pas - opération atomique (write + rename)
```

**Sandbox :** Chemin validé par `SandboxRoot`. Écriture hors sandbox rejetée avant toute opération disque.

### 3.5 `file_edit`

Remplacement chirurgical d'une chaîne dans un fichier. Échoue si `old_str` est introuvable ou apparaît plusieurs fois.

```python
await ctx.tools.file_edit.run(
    path="src/config.toml",
    old_str='host = "localhost"',
    new_str='host = "0.0.0.0"'
)
# Erreur si old_str absent : ToolExecutionError::ExecutionFailed { code: "NOT_FOUND", ... }
# Erreur si old_str non-unique : ToolExecutionError::ExecutionFailed { code: "NOT_UNIQUE", ... }
```

**Cas d'erreur :** La non-unicité est une protection explicite - fournir un contexte plus large dans `old_str` pour disambiguïser.

### 3.6 `file_list`

Liste les entrées d'un répertoire, triées. Supporte un paramètre `depth` pour les arborescences.

```python
entries = await ctx.tools.file_list.run(path=".", depth=2)
# entries : liste de { name, path, kind: "file"|"dir", size_bytes, modified_at }
```

**Sandbox :** Limité à l'arborescence sandbox de l'agent.

### 3.7 `file_glob`

Recherche de fichiers par pattern glob dans le sandbox.

```python
matches = await ctx.tools.file_glob.run(pattern="**/*.json")
# matches : liste de chemins relatifs à la racine sandbox
# Exemple : ["output/result.json", "config/agent.json"]
```

**Sandbox :** Le glob est ancré à la racine sandbox - un pattern absolu est rejeté par `SandboxRoot`.

### 3.8 `file_grep`

Recherche par expression régulière dans les fichiers du sandbox, avec lignes de contexte optionnelles.

```python
results = await ctx.tools.file_grep.run(
    pattern="ERROR|WARN",
    glob="**/*.log",
    context=2   # Lignes avant/après chaque match (optionnel)
)
# results : liste de { path, line_number, line, before: [...], after: [...] }
```

**Implémentation :** Basé sur ripgrep (bibliothèque Rust `grep-regex`). Résultats limités à 1000 matches par appel pour éviter les surcharges mémoire.

### 3.9 `http_fetch` *(feature flag `http`)*

Requêtes HTTP GET/POST avec application de la network allowlist de l'agent. Corps limité à 1 Mo.

```python
response = await ctx.tools.http_fetch.run(
    url="https://api.exemple.com/data",
    method="GET",   # "GET" (défaut) ou "POST"
    headers={"Authorization": "Bearer token"},
    body=None,      # JSON string pour POST
    timeout=15      # Secondes, défaut 10, max 60
)
# response.status_code, response.body, response.headers
```

**Contrôle réseau :** Chaque requête est vérifiée contre la `network_allowlist` déclarée dans le manifest agent avant émission. Une requête vers un domaine non listé est rejetée avec `ToolExecutionError::ExecutionFailed { code: "DOMAIN_NOT_ALLOWED" }`.

```python
AgentManifest(
    network_allowlist=["api.exemple.com", "*.googleapis.com"]
    # None (défaut) = aucun accès réseau autorisé
    # ["*"] = accès complet (warning au démarrage, entrée audit explicite)
)
```

**Garde anti-SSRF :** Après vérification de l'allowlist, `http_fetch` applique un second filtre via `apollia_tools::ssrf::assert_public`. Les URL pointant vers des hôtes privés (loopback `127.x`, RFC 1918, link-local `169.254.x.x`, metadata cloud, domaines `.local`/`.internal`/`localhost`) sont rejetées avec `ToolExecutionError::ExecutionFailed { code: "SSRF_BLOCKED" }`. Ce filtre s'applique même si l'hôte figure dans l'allowlist - une misconfiguration opérateur ne peut pas ouvrir l'accès à l'infrastructure interne.

**Feature flag :** Compilé uniquement si `features = ["http"]` dans `apollia-tools`. Absent du binaire par défaut.

### 3.10 `memory_search` *(feature flag `memory-search`)*

Recherche FTS5/BM25 dans la mémoire de l'agent, isolée par namespace. Respecte le Principe #6 (mémoire à initiative de l'agent - jamais d'injection automatique).

```python
results = await ctx.tools.memory_search.run(
    query="contrat client Acme",
    namespace="agent-devis-gen",  # Isolation stricte par namespace
    limit=10                       # Défaut 5, max 50
)
# results : liste de { id, content, score, metadata, created_at }
```

**Isolation namespace :** La requête FTS5 est toujours filtrée par `namespace = ?` avant évaluation du score BM25. Un agent ne peut jamais accéder aux souvenirs d'un autre agent.

**Principe #6 :** L'agent appelle explicitement `memory_search` - le runtime n'injecte jamais de contexte mémoriel automatiquement. L'initiative est toujours côté agent.

**Feature flag :** Compilé uniquement si `features = ["memory-search"]` dans `apollia-tools`. Requiert `apollia-memory` dans le workspace.

### 3.11 `mcp_consumer`

Pont vers n'importe quel serveur MCP de l'écosystème.

```python
# Le nom d'accès = "mcp_" + nom du serveur configuré
result = await ctx.tools.mcp_filesystem.read_file(path="/docs/rapport.pdf")
result = await ctx.tools.mcp_database.query(sql="SELECT * FROM clients")
```

**Fonctionnement :** Connexion persistante au serveur MCP (stateful client) ouverte à `INITIALIZING`, fermée à `STOPPING`. Pour les serveurs stdio, le processus est géré par Apollia OS.

### Outil déprécié : `file_io`

> **DEPRECATION** - `file_io` est déprécié et remplacé par les outils atomiques `file_read`, `file_write`, `file_edit`, `file_list`, `file_glob`, `file_grep`.
>
> Le code source est conservé dans `apollia-tools` mais l'outil **n'est plus enregistré** dans le registry. Toute tentative de résolution via `tools_required = ["file_io"]` ou `tools_optional = ["file_io"]` retourne un avertissement dans les logs :
>
> ```
> WARN tool_registry: "file_io" is deprecated and no longer registered; \
>      migrate to file_read/file_write/file_edit/file_list/file_glob/file_grep
> ```
>
> Voir ADR-043 pour la justification de la décomposition atomique.

---

## 4. Enregistrement d'un outil custom

### 4.1 Définition de l'outil

```python
# tools/erp_connector.py
from apollia_os import AIPTool, ToolDescriptor, SandboxProfile

class ERPConnector(AIPTool):
    descriptor = ToolDescriptor(
        name="erp_acme",
        version="1.0.0",
        description="Lecture/écriture ERP Acme Corp via API REST",
        input_schema={
            "type": "object",
            "properties": {
                "action": {"type": "string", "enum": ["get", "post", "put"]},
                "resource": {"type": "string"},
                "payload": {"type": "object"}
            },
            "required": ["action", "resource"]
        },
        sandbox_profile=SandboxProfile.NETWORK_RESTRICTED,
        tags=["erp", "business", "external-api"],
        dangerous=False
    )

    async def execute(self, action: str, resource: str, payload: dict = None) -> dict:
        # Logique d'intégration
        response = await self._api_client.call(action, resource, payload)
        return {"data": response, "status": "ok"}
```

### 4.2 Enregistrement via CLI

```bash
$ apollia-os tools register ./tools/erp_connector.py
  → Validation du schéma : OK
  → Test d'instanciation : OK
  ✔ erp_acme v1.0.0 enregistré dans le registry local

$ apollia-os tools list
  NOM                       TYPE     VERSION  SANTÉ   SANDBOX
  bash_executor             natif    0.1.0    ✔       FileSystem
  python_executor           natif    0.1.0    ✔       FileSystem
  file_read                 natif    0.2.0    ✔       FileSystem
  file_write                natif    0.2.0    ✔       FileSystem
  file_edit                 natif    0.2.0    ✔       FileSystem
  file_list                 natif    0.2.0    ✔       FileSystem
  file_glob                 natif    0.2.0    ✔       FileSystem
  file_grep                 natif    0.2.0    ✔       FileSystem
  http_fetch                natif    0.2.0    ✔       NetworkRestricted
  memory_search             natif    0.2.0    ✔       ReadOnly
  permission_rule_add       natif    0.1.0    ✔       ReadOnly
  permission_rule_remove    natif    0.1.0    ✔       ReadOnly
  permission_rule_list      natif    0.1.0    ✔       ReadOnly
  mcp_consumer              mcp      0.1.0    ✔       FileSystem
  erp_acme                  custom   1.0.0    ✔       NetworkRestricted
```

> Note : `file_io` n'apparaît plus dans cette liste (déprécié, non enregistré).

---

## 5. Résolution au démarrage

La résolution se déroule uniquement pendant `INITIALIZING`. C'est la phase où le runtime valide que l'agent dispose de tout ce dont il a besoin.

### 5.1 Distinction `tools_required` vs `tools_optional`

```python
AgentManifest(
    tools_required=["file_read", "file_write", "python_executor"],  # BLOQUANT : absent → STOPPED
    tools_optional=["mcp_erp_acme"],                                 # NON-BLOQUANT : absent → DEGRADED
)
```

| Outil | Disponibilité | Résultat |
|---|---|---|
| `tools_required` présent | → | Démarrage normal → `ACTIVE` |
| `tools_required` absent | → | Erreur claire → `STOPPED` |
| `tools_optional` présent | → | Démarrage normal → `ACTIVE` |
| `tools_optional` absent | → | Warning + démarrage → `DEGRADED` |
| outil déprécié (`file_io`) | → | Warning dans les logs + résolution échoue comme `NotFound` |
| dépendance `a2a:<skill>` (required ou optional) | → | Résolue d'office sans lookup registry → `ACTIVE` |

**Règle préfixe `a2a:`** - Les entrées commençant par `a2a:` (ex. `a2a:search-and-extract`) sont des skills d'agents inter-agents et ne sont pas enregistrées dans le `ToolRegistry`. Le resolver les considère comme résolues d'office ; la résolution réelle a lieu à l'invocation via le `ToolProxy` + `A2AInvoker`. Un manifest mixte est valide :

```python
AgentManifest(
    tools_required=["file_read"],                          # natif - vérifié dans le registry
    tools_optional=["a2a:synthesize-report", "mcp_erp"],   # A2A : OK d'office ; mcp_erp : warning si absent
)
```

### 5.2 Erreurs de résolution

```rust
pub enum ToolResolutionError {
    NotFound(String),               // Outil déclaré mais inexistant (inclut les dépréciés)
    McpServerUnreachable(String),   // Serveur MCP stdio introuvable ou HTTP down
    SandboxConflict(String),        // Profils sandbox incompatibles
    PackageInstallFailed(String),   // pip install échoué
    PermissionDenied(String),       // Outil `dangerous=true` sans flag explicite
}
```

---

## 6. Profils de sandbox

```rust
pub enum SandboxProfile {
    ReadOnly,            // Lecture seule, pas réseau, CPU/RAM limités
    FileSystem,          // Lecture/écriture sandbox agent, pas réseau
    NetworkRestricted,   // FileSystem + réseau limité à network_allowlist
    Full,                // Tout autorisé - nécessite dangerous=true dans ToolDescriptor
}
```

**Implémentation technique MVP (sans Docker) :**

| Profil | Mécanisme Linux | Limites |
|---|---|---|
| `ReadOnly` | `subprocess` + mount namespace (tmpfs ro) + PID namespace | CPU 0.5 core, RAM 128MB, timeout 30s |
| `FileSystem` | `subprocess` + mount namespace (sandbox rw) + PID namespace | CPU 1 core, RAM 256MB, timeout 60s |
| `NetworkRestricted` | FileSystem + network namespace + iptables whitelist | Idem + réseau filtré |
| `Full` | FileSystem + network namespace sans filtre | Idem + entrée audit explicite |

**Roadmap sandbox :**
- v0.1 : `subprocess` + namespaces Linux (`unshare`)
- v0.2 : `nsjail` (Google, open-source) - namespaces + seccomp-BPF dans un binaire
- v1.0 : gVisor optionnel pour déploiements production sensibles

---

## 7. Audit trail

Chaque invocation d'outil est tracée dans `~/.apollia/audit.db` (SQLite) :

```sql
CREATE TABLE tool_invocations (
    id              TEXT PRIMARY KEY,
    agent_id        TEXT NOT NULL,
    task_id         TEXT NOT NULL,
    tool_name       TEXT NOT NULL,
    input_hash      TEXT NOT NULL,        -- SHA256 des paramètres
    sandbox_profile TEXT NOT NULL,
    started_at      DATETIME NOT NULL,
    duration_ms     INTEGER,
    exit_code       INTEGER,
    success         BOOLEAN NOT NULL,
    error_code      TEXT,
    resources_used  TEXT,                 -- JSON : cpu_ms, memory_peak_kb
    args_json       TEXT,                 -- paramètres complets JSON
    stdout          TEXT,                 -- sortie standard tronquée
    stderr          TEXT                  -- sortie erreur tronquée
);
```

L'intégrité de la table est renforcée par deux triggers SQLite déclarés au schéma :

```sql
CREATE TRIGGER IF NOT EXISTS audit_no_update
    BEFORE UPDATE ON tool_invocations
    BEGIN SELECT RAISE(ABORT, 'audit trail is append-only'); END;

CREATE TRIGGER IF NOT EXISTS audit_no_delete
    BEFORE DELETE ON tool_invocations
    BEGIN SELECT RAISE(ABORT, 'audit trail is append-only'); END;
```

Ces triggers bloquent toute tentative de `UPDATE` ou `DELETE` sur `tool_invocations` au niveau du moteur SQLite. L'audit trail est strictement append-only : une invocation enregistrée ne peut jamais être modifiée ni supprimée par programme.

### 7.1 Observabilité des appels outils

Chaque invocation d'outil persiste les paramètres d'entrée et les sorties stdout/stderr, tronqués selon `ObservabilityConfig` :

```rust
pub struct ObservabilityConfig {
    pub max_input_bytes: usize,        // défaut 32768 (32 KB)
    pub max_output_bytes: usize,       // défaut 32768 (32 KB)
    pub max_tool_output_bytes: usize,  // défaut 10240 (10 KB)
    pub debug_log_prompt: bool,        // défaut false
}
```

La troncature utilise `truncate_with_marker` qui garantit des frontières UTF-8 valides et ajoute le marqueur `[TRONQUÉ - N octets total]` si le contenu dépasse la limite. Un flag `*_truncated` accompagne chaque champ tronqué.

Les colonnes `args_json`, `stdout`, `stderr` sont ajoutées par migration idempotente (`ALTER TABLE ADD COLUMN IF NOT EXISTS`). Les invocations antérieures ont ces colonnes à `NULL`.

**Consultation via CLI :**

```bash
$ apollia-os audit --last 10
  HEURE          AGENT            TÂCHE    OUTIL             DURÉE   RÉSULTAT
  10:00:03       devis-gen        t-009    file_read         8ms     ✔
  10:00:02       devis-gen        t-009    python_executor   234ms   ✔
  09:58:11       devis-gen        t-008    file_write        14ms    ✔
  09:48:05       crm-qual         t-007    http_fetch        1000ms  ✗ TIMEOUT

$ apollia-os audit stats
  Période    : dernières 24h
  Tâches     : 89 terminées, 3 échouées (96.7% succès)
  Temps moy  : 2.8s par tâche
  Outil +    : python_executor (34 appels)
  Outil -    : http_fetch (2 timeouts)
```

---

## 8. Introspection d'outils

, le `ToolRegistryHandle` expose une méthode `describe()` qui retourne le `ToolDescriptor` complet d'un outil enregistré :

```rust
impl ToolRegistryHandle {
    pub async fn describe(&self, name: &str) -> Option<ToolDescriptor>;
}
```

**Usage côté agent Python :**

```python
async def run(self, task: AIPTask, ctx: RuntimeContext) -> AIPResult:
    schema = await ctx.tools.describe("file_read")
    if schema:
        # schema contient : name, version, description, kind,
        # input_schema, output_schema, permissions
        input_fields = schema["input_schema"]
```

**Endpoints REST :**

```
GET /api/v1/tools           → Liste tous les outils actifs (ToolSummary[])
GET /api/v1/tools/:name     → Détail complet d'un outil (ToolDescriptor)
```

> Note : Les outils dépréciés non enregistrés (`file_io`) n'apparaissent pas dans ces endpoints.

**Commande Tauri :** `describe_tool(name)` retourne un `ToolDescriptorView` avec name, version, description, kind, input_schema, output_schema, permissions.

**Composant Svelte :** `ToolSchemaPanel` affiche le JSON schema, les permissions et le type d'outil dans la vue Memory/Tools du desktop.

---

## 9. ToolExecutor - Interface unifiée d'exécution

Le introduit un trait `ToolExecutor` et un routeur `ToolDispatcher` pour unifier l'invocation des outils natifs via une interface JSON générique. Voir ADR-043.

### 9.1 Trait `ToolExecutor`

Chaque outil natif implémente ce trait, découplant la logique métier du registry et du dispatch :

```rust
/// Interface d'exécution unifiée pour tous les outils natifs.
/// Chaque implémentation est stateless et thread-safe (Send + Sync).
pub trait ToolExecutor: Send + Sync {
    /// Identifiant unique de l'outil - doit correspondre au champ `name` du ToolDescriptor.
    fn tool_name(&self) -> &'static str;

    /// Exécute l'outil avec un input JSON arbitraire.
    /// Retourne un Value JSON ou une ToolExecutionError typée.
    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError>;
}

/// Erreurs d'exécution typées, exposées dans l'audit trail et retournées à l'agent.
pub enum ToolExecutionError {
    /// Input malformé ou paramètre manquant/invalide.
    InvalidInput { message: String },
    /// Échec pendant l'exécution (sandbox, réseau, fs, etc.).
    ExecutionFailed { code: String, message: String },
}
```

**Conventions :**
- `code` dans `ExecutionFailed` est une chaîne machine lisible (`"NOT_FOUND"`, `"NOT_UNIQUE"`, `"DOMAIN_NOT_ALLOWED"`, `"TRAVERSAL_ATTEMPTED"`, `"TIMEOUT"`, etc.)
- L'implémentation ne doit jamais `unwrap` ni `panic!` - toute erreur prévisible retourne `ToolExecutionError`
- `execute` reçoit et retourne `serde_json::Value` : la sérialisation/désérialisation des types métier est interne à l'implémentation

### 9.2 `ToolDispatcher` - Routeur par nom

`ToolDispatcher` maintient une map `tool_name → Box<dyn ToolExecutor>` et route les appels JSON entrants :

```rust
/// Routeur central qui dispatch les invocations d'outils par nom.
/// Construit une seule fois au démarrage, immuable pendant l'exécution.
pub struct ToolDispatcher {
    // map interne : tool_name → Arc<dyn ToolExecutor>
}

impl ToolDispatcher {
    /// Construit le dispatcher avec les executors enregistrés.
    pub fn new(executors: Vec<Arc<dyn ToolExecutor>>) -> Self;

    /// Route un appel vers l'executor correspondant.
    /// Retourne None si le nom est inconnu (outil non enregistré).
    pub async fn dispatch(
        &self,
        tool_name: &str,
        input: Value,
    ) -> Option<Result<Value, ToolExecutionError>>;
}
```

**Intégration dans le registry :** Le `ToolDispatcher` est construit à `INITIALIZING` après la résolution des outils. Il est passé en lecture seule aux acteurs qui gèrent l'exécution des tâches.

### 9.3 `SandboxRoot` - Validation des chemins

Le module `sandbox_path` centralise toute la logique de validation des traversals pour les outils fichiers :

```rust
/// Racine sandbox d'un agent. Toutes les opérations fichiers sont validées contre cette racine.
pub struct SandboxRoot(PathBuf);

impl SandboxRoot {
    /// Construit la racine à partir du répertoire sandbox de l'agent.
    pub fn new(root: PathBuf) -> Self;

    /// Résout `relative` contre la racine et valide qu'il reste dans le sandbox.
    /// Retourne le chemin absolu canonique si valide, SandboxPathError sinon.
    pub fn resolve(&self, relative: &Path) -> Result<PathBuf, SandboxPathError>;
}

pub enum SandboxPathError {
    /// Tentative de traversal hors sandbox détectée.
    TraversalAttempted { path: String, root: String },
}
```

**Utilisé par :** `file_read`, `file_write`, `file_edit`, `file_list`, `file_glob`, `file_grep`. Tout chemin est passé par `SandboxRoot::resolve` avant toute opération disque.

---

## 10. Outils MCP

Le introduit la crate `apollia-mcp` qui connecte le Tool Registry aux serveurs MCP externes. Un serveur MCP est un processus tiers (Node.js, Python, ou autre) qui expose des outils via le protocole JSON-RPC MCP.

### 10.1 Naming - `mcp:{server}/{tool}`

Chaque outil découvert sur un serveur MCP est enregistré dans le Tool Registry avec la convention :

```
mcp:{server_name}/{tool_name}
```

Exemples :

| Serveur | Outil MCP | Nom dans le Tool Registry |
|---|---|---|
| `notion` | `search()` | `mcp:notion/search` |
| `notion` | `create_page` | `mcp:notion/create_page` |
| `sqlite` | `query` | `mcp:sqlite/query` |
| `brave-search` | `brave_web_search` | `mcp:brave-search/brave_web_search` |

Le `server_name` provient du champ `name` de `mcp.toml`. Le `tool_name` est l'identifiant retourné par la réponse `tools/list` du serveur.

### 10.2 `ToolKind::McpServer`

Les outils MCP sont enregistrés avec `ToolKind::McpServer` :

```rust
pub enum ToolKind {
    Native,
    McpServer {
        server_url: String,       // nom du serveur (ex. "notion")
        transport: McpTransport,  // McpTransport::Stdio en V1
        tool_name: String,        // nom local de l'outil côté serveur (ex. "search")
    },
    Custom { .. },
}
```

`McpTransport::Stdio` est le seul transport supporté en V1 - le serveur MCP est un sous-processus local géré par le runtime.

### 10.3 Enregistrement automatique par `McpClientManager`

Au démarrage, `McpClientManagerHandle::start` :

1. Lit `~/.apollia/mcp.toml` et itère sur les serveurs déclarés.
2. Pour chaque serveur, démarre le processus (`command` + `args` + `env`) et effectue le handshake `initialize`.
3. Envoie `tools/list` au serveur et récupère les définitions d'outils.
4. Enregistre chaque outil découvert dans le `ToolRegistryHandle` avec un `ToolDescriptor` construit à partir de la définition MCP.
5. Un serveur qui échoue à démarrer est loggué et ignoré - les autres serveurs continuent.

L'enregistrement est idempotent par redémarrage de session : le manager gère les ajouts et suppressions dynamiques via les routes API (`POST /api/v1/mcp/servers`, `DELETE /api/v1/mcp/servers/:name`).

### 10.4 `McpToolExecutor` - Interface d'exécution unifiée

Chaque outil MCP découvert est encapsulé dans un `McpToolExecutor` qui implémente le trait `ToolExecutor` :

```rust
impl ToolExecutor for McpToolExecutor {
    fn tool_name(&self) -> &'static str { /* "mcp:notion/search" */ }

    async fn execute(&self, input: Value) -> Result<Value, ToolExecutionError>;
}
```

`execute` :
1. Vérifie si une approbation HITL est requise (serveur ou agent) - si oui, suspend et attend.
2. Sérialise `input` comme `arguments` du `tools/call` JSON-RPC.
3. Achemine la requête via `McpClientManagerHandle` vers la session du serveur.
4. Retourne le `content` de la réponse MCP comme `Value` JSON.

### 10.5 `SandboxProfile` selon `requires_approval`

Les outils MCP utilisent le profil sandbox suivant :

| Condition | `SandboxProfile` |
|---|---|
| `requires_approval = false` | `SandboxProfile::NetworkRestricted` |
| `requires_approval = true` | `SandboxProfile::Full` (l'approbation HITL tient lieu de garde-fou) |

Contrairement aux outils natifs dont le sandbox est appliqué par le runtime, le sandbox d'un outil MCP est déclaratif : le code s'exécute dans le processus serveur externe. `SandboxProfile` ici reflète la politique de confiance accordée au serveur.

### 10.6 `McpConfigWriter` - Mutations persistées

`McpConfigWriter` gère les mutations de `mcp.toml` depuis les routes API :

```rust
impl McpConfigWriter {
    pub fn add_server(&self, config: &McpServerConfig) -> Result<(), McpConfigWriteError>;
    pub fn remove_server(&self, name: &str) -> Result<(), McpConfigWriteError>;
    pub fn update_server(&self, name: &str, config: &McpServerConfig) -> Result<(), McpConfigWriteError>;
}
```

Chaque méthode : lit le fichier courant, applique la mutation en mémoire, valide, puis réécrit. L'ordre des serveurs est préservé par `update_server`. Les commentaires TOML ne sont pas préservés en V1 (TOML roundtrip via serde).

---

## 11. Décisions architecturales clés

| Décision | Justification |
|---|---|
| Schéma outil aligné MCP | Interopérabilité native avec l'écosystème MCP (16K+ serveurs) |
| Résolution uniquement à `INITIALIZING` | Fail fast - erreurs prévisibles, pas de surprises runtime |
| `tools_required` vs `tools_optional` | Distinction criticité explicite : `STOPPED` vs `DEGRADED` |
| Sandbox par profil prédéfini | Simple à comprendre et tester, pas de config per-outil |
| MVP sans Docker | Zéro dépendance, fonctionne sur tout Linux, évolutif vers nsjail |
| Audit log SQLite local | Souveraineté complète, format lisible, zéro service externe |
| `network_allowlist` dans manifest | Principe du moindre privilège - whitelist explicite |
| Décomposition atomique des outils fichiers (ADR-043) | `file_io` monolithique remplacé par 6 outils à responsabilité unique - testabilité, composabilité, erreurs typées par cas d'usage |
| `ToolExecutor` trait + `ToolDispatcher` (ADR-043) | Interface JSON unifiée - découplage registry/dispatch, ajout d'outils sans modifier le routeur |
| Feature flags `http` et `memory-search` | `http_fetch` et `memory_search` sont opt-in à la compilation - binaire minimal par défaut, zéro surface d'attaque réseau inutile |
| `SandboxRoot` comme type dédié | Centralisation de la logique anti-traversal - un seul endroit à auditer, impossibilité d'oublier la validation |
| Naming `mcp:{server}/{tool}` | Namespace explicite - évite les collisions avec les outils natifs, lisible dans les manifests agents et les logs audit |
| `McpClientManager` comme acteur unique | Pattern acteur Tokio strict - zéro état partagé, toutes les mutations de sessions passent par le channel `mpsc` |
| `McpConfigWriter` séparé de `McpClientManager` | Séparation I/O disque / état runtime - le writer est synchrone et stateless, le manager ne touche jamais le disque directement |
| `McpToolExecutor` implémente `ToolExecutor` | Les outils MCP sont indiscernables des outils natifs pour le `ToolDispatcher` - ajout de l'intégration MCP sans modifier le chemin d'exécution existant |
| Transport stdio V1 uniquement (ADR-043) | Local-first : le serveur MCP est un subprocess local, zéro appel réseau initié sans action explicite de l'utilisateur |

---

## 12. Concurrence d'outils *(ADR-059)*

### 12.1 Champ `is_read_only` sur `ToolDescriptor`

```rust
pub struct ToolDescriptor {
    // champs existants...
    pub is_read_only: bool,  // Défaut : false (conservateur)
}
```

**Défaut `false` = conservateur :** tout nouvel outil est considéré avec effets de bord jusqu'à annotation explicite.

**Outils marqués `is_read_only = true` :**

| Outil | Justification |
|-------|---------------|
| `file_read` | Lecture seule |
| `file_glob` | Parcours d'arborescence, lecture seule |
| `file_grep` | Recherche regex, lecture seule |
| `file_list` | Listage, lecture seule |
| `memory_search` | SELECT-only FTS5 |
| `git_status` | `git status --porcelain`, lecture seule |
| `permission_rule_list` | SELECT-only sur `permission_rules` |

**Outils NON marqués (`is_read_only = false`) :**

| Outil | Justification |
|-------|---------------|
| `bash_executor` | Effets de bord arbitraires |
| `file_write` | Écriture disque |
| `file_edit` | Modification disque |
| `persistent_bash` | Shell stateful avec effets de bord |
| `mcp:*` | Effets de bord inconnus côté serveur MCP |

### 12.2 `execute_batch` sur `ToolDispatcher`

```rust
impl ToolDispatcher {
    /// Exécute un batch d'invocations d'outils.
    ///
    /// - Si TOUS les outils sont `is_read_only = true` → `join_all` + `Semaphore(10)`
    /// - Si au moins un outil n'est pas read-only → exécution sérielle (ordre garanti)
    ///
    /// Les résultats sont toujours dans l'ordre d'entrée.
    pub async fn execute_batch(
        &self,
        calls: Vec<ToolCall>,
    ) -> Vec<Result<Value, ToolExecutionError>>;
}
```

**Règle fondamentale : batch mixte → sériel obligatoire.** Un seul outil avec effets de bord force l'exécution séquentielle de l'ensemble - l'ordre des effets est garanti.

**`Semaphore(10)` pour les batches read-only :** limite la concurrence pour éviter la saturation des file descriptors et les pics CPU.

**Gain mesuré :** 20 appels `file_grep` sériels × 50ms = 1 000ms → `join_all` ≈ 50ms (facteur 20×).

> **Référence technique :** [ADR-059](../adr/ADR-059-concurrent-tool-execution.md)

---

## 13. `persistent_bash` - Shell Persistant

`PersistentBashExecutor` maintient un processus shell (`/bin/bash`) vivant entre les steps. L'état du shell (répertoire courant, variables d'environnement, fonctions définies) est préservé.

### 13.1 Protocole marker UUID

La détection de fin de commande utilise un marqueur UUID unique pour distinguer la fin de l'output de la commande de la fin du processus :

```bash
# Commande exécutée par PersistentBashExecutor
<commande utilisateur>; echo "__APOLLIA_DONE_<uuid>__:$?"
```

`PersistentBashExecutor` lit stdout ligne par ligne jusqu'à trouver la ligne `__APOLLIA_DONE_<uuid>__:<exit_code>`. La partie avant le marqueur est le stdout de la commande ; l'`exit_code` est extrait du marqueur.

L'UUID est regénéré à chaque commande pour éviter les collisions si l'output de la commande contient une chaîne similaire.

### 13.2 `ShellSession` et `ShellSessionRegistry`

```rust
/// Session shell persistante pour un agent.
pub struct ShellSession {
    pub session_id: String,
    stdin: tokio::process::ChildStdin,
    stdout_reader: tokio::io::BufReader<tokio::process::ChildStdout>,
    marker_prefix: String,
}

impl ShellSession {
    /// Exécute une commande et attend le marqueur de fin.
    /// Retourne (stdout, stderr, exit_code).
    pub async fn run(
        &mut self,
        command: &str,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> Result<ShellOutput, ShellError>;
}

/// Registre des sessions actives par agent_id.
pub struct ShellSessionRegistry {
    sessions: HashMap<String, ShellSession>,
}
```

### 13.3 `is_read_only = false`

`PersistentBashExecutor` est toujours `is_read_only = false` - le shell peut avoir des effets de bord arbitraires. Il n'est jamais inclus dans un batch concurrent.

### 13.4 Exemple d'usage Python

```python
# Step 1 : change de répertoire
result = await ctx.tools.persistent_bash.run(command="cd /workspace/mon-projet && pwd")
# → "/workspace/mon-projet"

# Step 2 : la session se souvient du cwd
result = await ctx.tools.persistent_bash.run(command="ls -la")
# → liste le contenu de /workspace/mon-projet

# Step 3 : les variables sont persistées
result = await ctx.tools.persistent_bash.run(command="export API_KEY=test && echo $API_KEY")
# Step 4 :
result = await ctx.tools.persistent_bash.run(command="echo $API_KEY")
# → "test"
```

---

## 14. Nouvelles fonctionnalités

### 14.1 `BashValidator` - Validation pré-exécution

`BashValidator` valide la syntaxe bash et classe les risques **avant** l'exécution d'une commande. Il s'intègre dans `BashExecutor::execute` : risques d'abord (sync), syntaxe ensuite (async).

```rust
pub struct BashValidator {
    config: BashValidatorConfig,
}

impl BashValidator {
    pub fn new(config: BashValidatorConfig) -> Self { ... }
    /// Validation syntaxique via `bash -n -c` avec timeout 1s.
    pub async fn validate_syntax(&self, cmd: &str) -> Result<(), ToolError> { ... }
    /// Classification des risques - sync, rapide, avant validate_syntax.
    pub fn classify_risks(&self, cmd: &str) -> Vec<RiskCategory> { ... }
}
```

Nouvelles variantes `ToolError` :

```rust
#[error("bash syntax error in `{cmd}`: {detail}")]
SyntaxError { cmd: String, detail: String },

#[error("risky command blocked (category: {category:?}): {command}")]
RiskyCommand { command: String, category: RiskCategory },

#[error("bash syntax validation timed out")]
SyntaxValidationTimeout,
```

### 14.2 `RiskClassifier` - Classification des risques shell

Classifie les commandes shell selon 4 catégories documentées par des standards publics.

```rust
pub struct RiskClassifier;

#[derive(Debug, Clone, PartialEq)]
pub enum RiskCategory {
    /// Accès réseau sortant - OWASP A10:2021, Principe #1 Apollia (local-first)
    NetworkEgress,
    /// Destruction irréversible de données - NIST SP 800-190 §4.4
    DestructiveOp,
    /// Élévation de droits - CWE-269
    PrivilegeEscalation,
    /// Consommation non-contrôlée de ressources - CWE-400
    ResourceExhaustion,
}

impl RiskClassifier {
    /// Retourne les catégories de risque détectées. Liste configurable via apollia.toml.
    pub fn classify(command: &str, config: &BashValidatorConfig) -> Vec<RiskCategory> { ... }
}
```

Config `apollia.toml` :

```toml
[tools.bash]
block_network_egress = true          # OWASP A10:2021
block_destructive = true             # NIST SP 800-190
block_privilege_escalation = true    # CWE-269
block_resource_exhaustion = true     # CWE-400
# network_egress_patterns = ["curl", "wget", "nc", "ssh"]
# destructive_patterns = ["rm -rf /", "dd if="]
syntax_check_timeout_ms = 1000
```

**Philosophie :** aucune liste hardcodée - tout est configurable par l'opérateur. Comportement opt-in par catégorie.

### 14.3 `FilePathExtractor` - Extraction post-bash non-bloquante

Extrait les paths de fichiers depuis la sortie d'une commande bash via `LlmRouter::route_fast`. Tournant dans un `tokio::spawn` détaché - n'impacte pas la latence de `BashExecutor`.

```rust
pub struct FilePathExtractor {
    llm_router: Arc<LlmRouter>,
}

impl FilePathExtractor {
    pub fn new(llm_router: Arc<LlmRouter>) -> Self { ... }
    /// Lance l'extraction en arrière-plan - non-bloquant pour BashExecutor.
    pub fn extract_detached(
        &self,
        command: String,
        output: String,
        event_tx: tokio::sync::mpsc::Sender<RuntimeEvent>,
    ) { ... }
}
```

Nouveau `RuntimeEvent` :

```rust
/// Paths de fichiers extraits depuis la sortie d'une commande bash.
BashFilePathsExtracted {
    paths: Vec<std::path::PathBuf>,
},
```

ORIA reçoit cet event pour invalider les caches de plan stale sur les fichiers affectés.

> **Voir aussi :** [apollia-permissions](./Briques-Permissions.md) - intégration `PermissionEngine::decide` dans `ToolRegistry::invoke`

---

## 15. Outils Notebook Jupyter

, deux outils natifs permettent aux agents de lire et d'éditer des notebooks Jupyter `.ipynb` (format ipynb v4).

### Types publics (`apollia-core`)

```rust
// crates/apollia-core/src/types.rs

/// Type d'une cellule Jupyter.
pub enum CellType { Code, Markdown, Raw }

/// Cellule d'un notebook - agnostique au format de sérialisation.
pub struct JupyterCell {
    pub cell_type: CellType,
    pub source: Vec<String>,   // lignes de code ou markdown
    pub outputs: Vec<serde_json::Value>, // sorties d'exécution (vides sur lecture)
}

/// Opération atomique sur un notebook.
pub enum NotebookEditOp {
    EditCell   { index: usize, new_source: Vec<String> },
    InsertCell { index: usize, cell_type: CellType, source: Vec<String> },
    DeleteCell { index: usize },
}
```

### `NotebookReadExecutor`

```rust
// crates/apollia-tools/src/tools/notebook_read.rs

/// Lit un fichier .ipynb et retourne ses cellules formatées pour le LLM.
/// is_read_only = true.
pub struct NotebookRead { /* sandbox_root: PathBuf */ }

pub struct NotebookReadInput {
    pub path: String,              // chemin relatif au sandbox_root
    pub cell_range: Option<(usize, usize)>, // None = toutes les cellules
}

pub struct NotebookReadOutput {
    pub cells: Vec<JupyterCell>,
    pub formatted: String,         // rendu texte pour le LLM
}
```

**Format de sortie :**

```
Cell 0 [code]:
  import pandas as pd
  df = pd.read_csv('data.csv')

Cell 1 [markdown]:
  # Analyse des données

Cell 2 [code]:
  df.describe()
  [output]: ...
```

### `NotebookEditExecutor`

```rust
// crates/apollia-tools/src/tools/notebook_edit.rs

/// Édite un fichier .ipynb via des opérations atomiques.
/// is_read_only = false.
pub struct NotebookEdit { /* sandbox_root: PathBuf */ }

pub struct NotebookEditInput {
    pub path: String,
    pub operation: NotebookEditOp,
}

pub struct NotebookEditOutput {
    pub cells_count: usize,
    pub operation_applied: String,
}
```

**Comportements garantis :**
- `EditCell` : modifie `source` et **vide les `outputs`** (résultats d'exécution précédents)
- `InsertCell` : insère à l'index spécifié, décale les cellules suivantes
- `DeleteCell` : supprime et recalcule les indices
- Le fichier `.ipynb` original est relu, modifié en mémoire, puis réécrit atomiquement

**Nommage dans le Tool Registry :**

| Outil | `tool_name` |
|---|---|
| Lecture | `notebook_read` |
| Édition | `notebook_edit` |

**Utilisation depuis un agent Python :**

```python
# Lire un notebook
result = await ctx.tools.notebook_read.run(path="analyse.ipynb")
print(result.formatted)

# Modifier une cellule
result = await ctx.tools.notebook_edit.run(
    path="analyse.ipynb",
    operation={"EditCell": {"index": 2, "new_source": ["df.head(10)"]}}
)
```

---

## Décisions architecturales clés (mises à jour)

| Décision | Justification |
|---|---|
| `is_read_only = false` par défaut | Conservateur - pas de régression sur les outils futurs (ADR-059) |
| Batch mixte → sériel | Ordre des effets garanti - sécurité > performance sur les batches hétérogènes |
| Semaphore(10) sur les batches read-only | Évite la saturation des fd système et les pics CPU sur les machines contraintes |
| Marqueur UUID dans `persistent_bash` | Détection fiable de fin de commande même si l'output contient des chaînes arbitraires |
| `ShellSessionRegistry` par agent_id | Isolation stricte - deux agents ne partagent jamais une session shell |
| Absent de `tools` = actif par défaut | La table `tools` de `governance.db` est une liste d'exception - tout outil inconnu reste activé, seul `enabled = FALSE` désactive |
| AES-256-GCM pour les credentials | Chiffrement symétrique authentifié - le ciphertext intègre le MAC, toute altération échoue au déchiffrement |

---

## 16. Gouvernance des outils natifs

### 16.1 Rôle

`apollia_tools::tool_registry` expose deux composants persistés dans `governance.db` :

- **`ToolRegistry`** - état `enabled` / `disabled` par outil natif.
- **`ToolCredentialStore`** - secrets chiffrés par outil (ex. clé Brave Search).

Au démarrage du runtime, `load_governance_snapshot` lit ces deux composants et produit un `GovernanceSnapshot` injecté dans `NativeDispatcherConfig`. Les outils désactivés sont exclus du `ToolDispatcher` - tout appel à un tel outil retourne `UnknownTool`.

### 16.2 `ToolRegistry` - activation / désactivation

```rust
/// Registre persisté des outils activés/désactivés et de leur config JSON.
pub struct ToolRegistry {
    conn: Connection,  // connexion SQLite vers governance.db
}

impl ToolRegistry {
    pub fn new(db_path: &Path) -> Result<Self, ToolGovernanceError>;

    /// Absent de la table → actif (défaut). Seulement `enabled = FALSE` désactive.
    pub fn is_enabled(&self, tool_name: &str) -> Result<bool, ToolGovernanceError>;

    /// Upsert atomique : insère ou met à jour + `updated_at = unixepoch()`.
    pub fn set_enabled(&mut self, tool_name: &str, enabled: bool) -> Result<(), ToolGovernanceError>;

    /// Lit la config JSON spécifique à l'outil (`None` si absente).
    pub fn get_config(&self, tool_name: &str) -> Result<Option<serde_json::Value>, ToolGovernanceError>;

    /// Upsert de la config JSON.
    pub fn set_config(&mut self, tool_name: &str, config: &serde_json::Value) -> Result<(), ToolGovernanceError>;

    /// Retourne le statut de tous les outils natifs connus (`NATIVE_TOOL_NAMES`)
    /// union les entrées de la table `tools`.
    pub fn list(&self) -> Result<Vec<ToolStatus>, ToolGovernanceError>;
}
```

**`NATIVE_TOOL_NAMES`** - liste canonique des 16 outils natifs du runtime :

```rust
pub const NATIVE_TOOL_NAMES: &[&str] = &[
    "bash_executor", "python_executor",
    "file_read", "file_write", "file_list", "file_edit", "file_glob", "file_grep",
    "http_fetch",
    "web_search", "web_read",
    "memory_search",
    "ask_user",
    // ADR-086 - gouvernance agent-driven des permissions.
    "permission_rule_add",
    "permission_rule_remove",
    "permission_rule_list",
];
```

**`ToolStatus`** - snapshot d'un outil :

```rust
pub struct ToolStatus {
    pub name: String,
    pub enabled: bool,
    pub config: Option<serde_json::Value>,
    pub updated_at: i64,   // Unix seconds, 0 si pas d'entrée en base
}
```

### 16.3 `ToolCredentialStore` - secrets chiffrés AES-256-GCM

```rust
pub struct ToolCredentialStore {
    conn: Connection,     // connexion SQLite vers governance.db
    key: [u8; 32],        // clé AES-256 chargée depuis ~/.apollia/.keyfile
}

impl ToolCredentialStore {
    /// Ouvre le store. Crée le `.keyfile` (32 octets aléatoires, chmod 600)
    /// s'il n'existe pas encore.
    pub fn new(db_path: &Path, keyfile: &Path) -> Result<Self, ToolGovernanceError>;

    /// Chiffre `value` en AES-256-GCM et l'insère ou met à jour.
    /// Le nonce 12 octets est regénéré aléatoirement à chaque écriture
    /// et préfixé au ciphertext dans la colonne `value_encrypted`.
    pub fn set(&mut self, tool_name: &str, key_name: &str, value: &str)
        -> Result<(), ToolGovernanceError>;

    /// Lit et déchiffre la valeur. Retourne `None` si absente.
    pub fn get(&self, tool_name: &str, key_name: &str)
        -> Result<Option<String>, ToolGovernanceError>;

    /// Supprime la credential. Retourne `true` si une ligne a été effacée.
    pub fn delete(&mut self, tool_name: &str, key_name: &str)
        -> Result<bool, ToolGovernanceError>;

    /// Liste les credentials (métadonnées uniquement - valeur jamais exposée).
    /// `tool_name_filter = None` retourne toutes les credentials.
    pub fn list(&self, tool_name_filter: Option<&str>)
        -> Result<Vec<CredentialEntry>, ToolGovernanceError>;
}

pub struct CredentialEntry {
    pub tool_name: String,
    pub key_name: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}
```

**Protocole de chiffrement :**

```
stocké = nonce(12 octets) || AES-256-GCM(key, nonce, plaintext)
```

La clé maître est stockée dans `<data_dir>/.keyfile` (32 octets bruts, `chmod 600` à la création). Elle n'est jamais stockée dans `governance.db`.

### 16.4 `GovernanceSnapshot` et `load_governance_snapshot`

```rust
/// Snapshot léger chargé une fois au démarrage, injecté dans NativeDispatcherConfig.
#[derive(Default)]
pub struct GovernanceSnapshot {
    /// Noms des outils dont `enabled = FALSE` dans governance.db.
    pub disabled_tools: Vec<String>,
    /// Clé Brave Search déchiffrée depuis le credential store (`web_search/brave.api_key`).
    pub brave_api_key: Option<String>,
}

/// Charge le snapshot depuis `<base_dir>/governance.db` et `<base_dir>/.keyfile`.
/// Retourne `GovernanceSnapshot::default()` (tous outils actifs, pas de clé Brave)
/// si `governance.db` n'existe pas encore - le runtime fonctionne avant la première écriture.
pub fn load_governance_snapshot(base_dir: &Path)
    -> Result<GovernanceSnapshot, ToolGovernanceError>;
```

**Chemin de données :** `<data_dir>/governance.db` et `<data_dir>/.keyfile`.

`data_dir` est le répertoire de données du runtime (typiquement `~/.apollia/`). La table `tools` de `governance.db` doit être initialisée via `GovernanceDb::open` (crate interne) avant toute utilisation de `ToolRegistry`.

### 16.5 Intégration dans `NativeDispatcherConfig`

`build_native_dispatcher` accepte les champs de gouvernance et les configurations d'outils web :

```rust
pub struct NativeDispatcherConfig {
    // ... champs existants ...
    /// Outils exclus du dispatcher - tout appel retourne `UnknownTool`.
    /// Produit par `merge_disabled` : union de la liste statique (`apollia.toml`)
    /// et de la liste dynamique (`governance.db`). Un outil absent des deux est actif.
    pub disabled_tools: Vec<String>,
    /// Clé Brave Search issue du credential store ; `None` → fallback env configuré
    /// dans `web_search_config.brave.api_key_env_var` (défaut `BRAVE_SEARCH_API_KEY`).
    pub brave_api_key: Option<String>,
    /// Configuration de l'outil `web_search` issue de `[tools.web_search]` dans `apollia.toml`.
    /// Pilote le choix du backend (auto/duckduckgo/brave), les timeouts et les limites.
    pub web_search_config: WebSearchConfig,
    /// Configuration de l'outil `web_read` issue de `[tools.web_read]` dans `apollia.toml`.
    /// Pilote le timeout HTTP, la taille maximale de réponse et le garde anti-SSRF.
    pub web_read_config: WebReadConfig,
    /// Chemin vers `governance.db` pour les outils `permission_rule_*` (ADR-086).
    /// Quand `None`, ces trois outils ne sont pas enregistrés dans le dispatcher.
    pub governance_db_path: Option<PathBuf>,
}
```

**Règle d'exclusion :** chaque outil est conditionné par `is_active(name) = !disabled_tools.contains(name)`. Un outil absent de `disabled_tools` est toujours inséré dans le `ToolDispatcher`.

**Fusion des listes de désactivation :** la fonction `merge_disabled` réalise l'union des deux sources :

```rust
/// Union de la liste statique (apollia.toml) et de la liste dynamique (governance.db).
/// Un outil désactivé dans l'une ou l'autre source est inactif - les deux sources
/// sont complémentaires.
fn merge_disabled(static_disabled: &[String], mut runtime_disabled: Vec<String>) -> Vec<String> {
    for name in static_disabled {
        if !runtime_disabled.iter().any(|n| n == name) {
            runtime_disabled.push(name.clone());
        }
    }
    runtime_disabled
}
```

`static_disabled` provient de `ToolsConfig.disabled` (section `[tools]` de `apollia.toml`). `runtime_disabled` provient de `GovernanceSnapshot.disabled_tools` (table `tools` de `governance.db`).

### 16.6 `ToolGovernanceError` - erreurs typées

```rust
#[derive(Debug, thiserror::Error)]
pub enum ToolGovernanceError {
    #[error("governance database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("keyfile I/O error at {path}: {source}")]
    Keyfile { path: PathBuf, #[source] source: std::io::Error },

    #[error("keyfile is corrupted: expected 32 bytes, found {found}")]
    KeyfileCorrupted { found: usize },

    #[error("encrypted value is corrupted (too short)")]
    CiphertextCorrupted,

    #[error("invalid tool config JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),

    #[error("decryption failed (wrong key or tampered ciphertext)")]
    DecryptFailed,

    #[error("encryption failed")]
    EncryptFailed,
}
```

---

*Prochaine lecture recommandée : [Memory Engine](./Briques-Memory-Engine)*

> Voir aussi [MCP Integration](./MCP-Integration) pour la configuration utilisateur des serveurs MCP (ajout/suppression à chaud, `mcp.toml`).
> Voir aussi [Briques-Workspace](./Briques-Workspace) pour le ContextProvider trait et les providers de contexte.
