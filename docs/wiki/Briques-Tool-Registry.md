# Tool Registry — Catalogue, Sandbox, Audit Trail

> *Spécification complète du système de gestion des outils : catalogue, résolution, sandbox, et traçabilité.*

---

## 1. Rôle dans l'architecture

Le Tool Registry est la **couche d'outillage** d'Apollia OS. Il répond à 4 questions :

1. **Catalogue** : Quels outils sont disponibles pour les agents ?
2. **Résolution** : Comment valider qu'un agent dispose de tous ses outils requis au démarrage ?
3. **Injection** : Comment fournir à l'agent une interface propre vers ses outils via `RuntimeContext` ?
4. **Exécution** : Comment invoquer un outil de manière isolée et tracer cette invocation ?

---

## 2. ToolDescriptor — L'unité du catalogue

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

**Isolation :** Virtualenv dédié dans `~/.apollia/sandboxes/<agent_id>/venv/`. Les packages déclarés dans le manifest sont installés à `INITIALIZING` — une tentative d'installer un package à l'exécution échoue avec une erreur claire.

**Décision de design :** Les packages sont installés au démarrage (fail fast) et non à l'exécution pour éviter les surprises de performance et les installations silencieuses non auditées.

### 3.3 `file_read` *(Sprint 25)*

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

### 3.4 `file_write` *(Sprint 25)*

Écrit un fichier dans le répertoire sandbox. Crée les répertoires parents manquants.

```python
await ctx.tools.file_write.run(
    path="output/result.json",
    content=json.dumps(data, indent=2)
)
# Crée output/ s'il n'existe pas — opération atomique (write + rename)
```

**Sandbox :** Chemin validé par `SandboxRoot`. Écriture hors sandbox rejetée avant toute opération disque.

### 3.5 `file_edit` *(Sprint 25)*

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

**Cas d'erreur :** La non-unicité est une protection explicite — fournir un contexte plus large dans `old_str` pour disambiguïser.

### 3.6 `file_list` *(Sprint 25)*

Liste les entrées d'un répertoire, triées. Supporte un paramètre `depth` pour les arborescences.

```python
entries = await ctx.tools.file_list.run(path=".", depth=2)
# entries : liste de { name, path, kind: "file"|"dir", size_bytes, modified_at }
```

**Sandbox :** Limité à l'arborescence sandbox de l'agent.

### 3.7 `file_glob` *(Sprint 25)*

Recherche de fichiers par pattern glob dans le sandbox.

```python
matches = await ctx.tools.file_glob.run(pattern="**/*.json")
# matches : liste de chemins relatifs à la racine sandbox
# Exemple : ["output/result.json", "config/agent.json"]
```

**Sandbox :** Le glob est ancré à la racine sandbox — un pattern absolu est rejeté par `SandboxRoot`.

### 3.8 `file_grep` *(Sprint 25)*

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

### 3.9 `http_fetch` *(Sprint 25 — feature flag `http`)*

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

**Feature flag :** Compilé uniquement si `features = ["http"]` dans `apollia-tools`. Absent du binaire par défaut.

### 3.10 `memory_search` *(Sprint 25 — feature flag `memory-search`)*

Recherche FTS5/BM25 dans la mémoire de l'agent, isolée par namespace. Respecte le Principe #6 (mémoire à initiative de l'agent — jamais d'injection automatique).

```python
results = await ctx.tools.memory_search.run(
    query="contrat client Acme",
    namespace="agent-devis-gen",  # Isolation stricte par namespace
    limit=10                       # Défaut 5, max 50
)
# results : liste de { id, content, score, metadata, created_at }
```

**Isolation namespace :** La requête FTS5 est toujours filtrée par `namespace = ?` avant évaluation du score BM25. Un agent ne peut jamais accéder aux souvenirs d'un autre agent.

**Principe #6 :** L'agent appelle explicitement `memory_search` — le runtime n'injecte jamais de contexte mémoriel automatiquement. L'initiative est toujours côté agent.

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

> **DEPRECATION (Sprint 25)** — `file_io` est déprécié et remplacé par les outils atomiques `file_read`, `file_write`, `file_edit`, `file_list`, `file_glob`, `file_grep`.
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
  NOM                TYPE     VERSION  SANTÉ   SANDBOX
  bash_executor      natif    0.1.0    ✔       FileSystem
  python_executor    natif    0.1.0    ✔       FileSystem
  file_read          natif    0.2.0    ✔       FileSystem
  file_write         natif    0.2.0    ✔       FileSystem
  file_edit          natif    0.2.0    ✔       FileSystem
  file_list          natif    0.2.0    ✔       FileSystem
  file_glob          natif    0.2.0    ✔       FileSystem
  file_grep          natif    0.2.0    ✔       FileSystem
  http_fetch         natif    0.2.0    ✔       NetworkRestricted
  memory_search      natif    0.2.0    ✔       ReadOnly
  mcp_consumer       natif    0.1.0    ✔       FileSystem
  erp_acme           custom   1.0.0    ✔       NetworkRestricted
```

> Note : `file_io` n'apparaît plus dans cette liste (déprécié, non enregistré depuis Sprint 25).

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
    Full,                // Tout autorisé — nécessite dangerous=true dans ToolDescriptor
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
- v0.2 : `nsjail` (Google, open-source) — namespaces + seccomp-BPF dans un binaire
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
    args_json       TEXT,                 -- paramètres complets JSON (Sprint 13)
    stdout          TEXT,                 -- sortie standard tronquée (Sprint 13)
    stderr          TEXT                  -- sortie erreur tronquée (Sprint 13)
);
```

### 7.1 Observabilité des appels outils *(Sprint 13)*

Depuis le Sprint 13, chaque invocation d'outil persiste les paramètres d'entrée et les sorties stdout/stderr, tronqués selon `ObservabilityConfig` :

```rust
pub struct ObservabilityConfig {
    pub max_input_bytes: usize,        // défaut 32768 (32 KB)
    pub max_output_bytes: usize,       // défaut 32768 (32 KB)
    pub max_tool_output_bytes: usize,  // défaut 10240 (10 KB)
    pub debug_log_prompt: bool,        // défaut false
}
```

La troncature utilise `truncate_with_marker()` qui garantit des frontières UTF-8 valides et ajoute le marqueur `[TRONQUÉ — N octets total]` si le contenu dépasse la limite. Un flag `*_truncated` accompagne chaque champ tronqué.

Les colonnes `args_json`, `stdout`, `stderr` sont ajoutées par migration idempotente (`ALTER TABLE ADD COLUMN IF NOT EXISTS`). Les invocations antérieures au Sprint 13 ont ces colonnes à `NULL`.

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
  Outil —    : http_fetch (2 timeouts)
```

---

## 8. Introspection d'outils *(Sprint 20)*

Depuis le Sprint 20 (STORY-224), le `ToolRegistryHandle` expose une méthode `describe()` qui retourne le `ToolDescriptor` complet d'un outil enregistré :

```rust
impl ToolRegistryHandle {
    pub async fn describe(&self, name: &str) -> Option<ToolDescriptor>;
}
```

**Usage côté agent Python (STORY-225) :**

```python
async def run(self, task: AIPTask, ctx: RuntimeContext) -> AIPResult:
    schema = await ctx.tools.describe("file_read")
    if schema:
        # schema contient : name, version, description, kind,
        # input_schema, output_schema, permissions
        input_fields = schema["input_schema"]
```

**Endpoints REST (Sprint 20) :**

```
GET /api/v1/tools           → Liste tous les outils actifs (ToolSummary[])
GET /api/v1/tools/:name     → Détail complet d'un outil (ToolDescriptor)
```

> Note : Les outils dépréciés non enregistrés (`file_io`) n'apparaissent pas dans ces endpoints.

**Commande Tauri :** `describe_tool(name)` retourne un `ToolDescriptorView` avec name, version, description, kind, input_schema, output_schema, permissions.

**Composant Svelte :** `ToolSchemaPanel` affiche le JSON schema, les permissions et le type d'outil dans la vue Memory/Tools du desktop.

---

## 9. ToolExecutor — Interface unifiée d'exécution *(Sprint 25)*

Le Sprint 25 introduit un trait `ToolExecutor` et un routeur `ToolDispatcher` pour unifier l'invocation des outils natifs via une interface JSON générique. Voir ADR-043.

### 9.1 Trait `ToolExecutor`

Chaque outil natif implémente ce trait, découplant la logique métier du registry et du dispatch :

```rust
/// Interface d'exécution unifiée pour tous les outils natifs.
/// Chaque implémentation est stateless et thread-safe (Send + Sync).
pub trait ToolExecutor: Send + Sync {
    /// Identifiant unique de l'outil — doit correspondre au champ `name` du ToolDescriptor.
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
- L'implémentation ne doit jamais `unwrap()` ni `panic!()` — toute erreur prévisible retourne `ToolExecutionError`
- `execute` reçoit et retourne `serde_json::Value` : la sérialisation/désérialisation des types métier est interne à l'implémentation

### 9.2 `ToolDispatcher` — Routeur par nom

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

### 9.3 `SandboxRoot` — Validation des chemins

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

**Utilisé par :** `file_read`, `file_write`, `file_edit`, `file_list`, `file_glob`, `file_grep`. Tout chemin est passé par `SandboxRoot::resolve()` avant toute opération disque.

---

## 10. Décisions architecturales clés

| Décision | Justification |
|---|---|
| Schéma outil aligné MCP | Interopérabilité native avec l'écosystème MCP (16K+ serveurs) |
| Résolution uniquement à `INITIALIZING` | Fail fast — erreurs prévisibles, pas de surprises runtime |
| `tools_required` vs `tools_optional` | Distinction criticité explicite : `STOPPED` vs `DEGRADED` |
| Sandbox par profil prédéfini | Simple à comprendre et tester, pas de config per-outil |
| MVP sans Docker | Zéro dépendance, fonctionne sur tout Linux, évolutif vers nsjail |
| Audit log SQLite local | Souveraineté complète, format lisible, zéro service externe |
| `network_allowlist` dans manifest | Principe du moindre privilège — whitelist explicite |
| Décomposition atomique des outils fichiers (ADR-043) | `file_io` monolithique remplacé par 6 outils à responsabilité unique — testabilité, composabilité, erreurs typées par cas d'usage |
| `ToolExecutor` trait + `ToolDispatcher` (ADR-043) | Interface JSON unifiée — découplage registry/dispatch, ajout d'outils sans modifier le routeur |
| Feature flags `http` et `memory-search` | `http_fetch` et `memory_search` sont opt-in à la compilation — binaire minimal par défaut, zéro surface d'attaque réseau inutile |
| `SandboxRoot` comme type dédié | Centralisation de la logique anti-traversal — un seul endroit à auditer, impossibilité d'oublier la validation |

---

*Prochaine lecture recommandée : [Memory Engine](./Briques-Memory-Engine)*
