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
    pub name: String,                // "bash_executor", "mcp_filesystem"
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

### 3.3 `file_io`

Lecture/écriture dans le répertoire sandbox de l'agent.

```python
# Lecture
content = await ctx.tools.file_io.read(path="rapport.pdf", encoding="bytes")
text = await ctx.tools.file_io.read(path="config.json")

# Écriture
await ctx.tools.file_io.write(path="output/result.json", content=json.dumps(data))

# Navigation
files = await ctx.tools.file_io.list(path=".", pattern="*.json")

# Export vers espace partagé
await ctx.tools.file_io.export(
    src="output/devis-042.json",
    dest_namespace="shared"  # Accessible par d'autres agents
)
```

**Isolation filesystem :** Chaque agent a son répertoire sandbox dédié `~/.apollia/sandboxes/<agent_id>/workspace/`. Tout path traversal (`../`) est rejeté avec une erreur `SandboxViolation`. L'espace `shared` est géré par le runtime avec ACL par namespace.

### 3.4 `http_client`

Requêtes HTTP avec whitelist de domaines.

```python
response = await ctx.tools.http_client.get(
    url="https://api.exemple.com/data",
    headers={"Authorization": "Bearer token"},
    timeout=15
)
# response.status_code, response.json(), response.text
```

**Contrôle réseau :** Par défaut, aucun accès réseau. L'agent déclare ses domaines autorisés dans le manifest :

```python
AgentManifest(
    network_allowlist=["api.exemple.com", "*.googleapis.com"]
    # None (défaut) = pas d'accès réseau
    # ["*"] = accès complet (warning au démarrage, entrée audit)
)
```

### 3.5 `mcp_consumer`

Pont vers n'importe quel serveur MCP de l'écosystème.

```python
# Le nom d'accès = "mcp_" + nom du serveur configuré
result = await ctx.tools.mcp_filesystem.read_file(path="/docs/rapport.pdf")
result = await ctx.tools.mcp_database.query(sql="SELECT * FROM clients")
```

**Fonctionnement :** Connexion persistante au serveur MCP (stateful client) ouverte à `INITIALIZING`, fermée à `STOPPING`. Pour les serveurs stdio, le processus est géré par Apollia OS.

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
  file_io            natif    0.1.0    ✔       FileSystem
  http_client        natif    0.1.0    ✔       NetworkRestricted
  mcp_consumer       natif    0.1.0    ✔       FileSystem
  erp_acme           custom   1.0.0    ✔       NetworkRestricted
```

---

## 5. Résolution au démarrage

La résolution se déroule uniquement pendant `INITIALIZING`. C'est la phase où le runtime valide que l'agent dispose de tout ce dont il a besoin.

### 5.1 Distinction `tools_required` vs `tools_optional`

```python
AgentManifest(
    tools_required=["file_io", "python_executor"],  # BLOQUANT : absent → STOPPED
    tools_optional=["mcp_erp_acme"],                # NON-BLOQUANT : absent → DEGRADED
)
```

| Outil | Disponibilité | Résultat |
|---|---|---|
| `tools_required` présent | → | Démarrage normal → `ACTIVE` |
| `tools_required` absent | → | Erreur claire → `STOPPED` |
| `tools_optional` présent | → | Démarrage normal → `ACTIVE` |
| `tools_optional` absent | → | Warning + démarrage → `DEGRADED` |

### 5.2 Erreurs de résolution

```rust
pub enum ToolResolutionError {
    NotFound(String),               // Outil déclaré mais inexistant
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
  10:00:03       devis-gen        t-009    file_io           12ms    ✔
  10:00:02       devis-gen        t-009    python_executor   234ms   ✔
  09:48:05       crm-qual         t-007    http_client       1000ms  ✗ TIMEOUT

$ apollia-os audit stats
  Période    : dernières 24h
  Tâches     : 89 terminées, 3 échouées (96.7% succès)
  Temps moy  : 2.8s par tâche
  Outil +    : python_executor (34 appels)
  Outil —    : http_client (2 timeouts)
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
    schema = await ctx.tools.describe("bash_executor")
    if schema:
        # schema contient : name, version, description, kind,
        # input_schema, output_schema, permissions
        input_fields = schema["input_schema"]
```

**Endpoints REST (Sprint 20) :**

```
GET /api/v1/tools           → Liste tous les outils (ToolSummary[])
GET /api/v1/tools/:name     → Détail complet d'un outil (ToolDescriptor)
```

**Commande Tauri :** `describe_tool(name)` retourne un `ToolDescriptorView` avec name, version, description, kind, input_schema, output_schema, permissions.

**Composant Svelte :** `ToolSchemaPanel` affiche le JSON schema, les permissions et le type d'outil dans la vue Memory/Tools du desktop.

---

## 9. Décisions architecturales clés

| Décision | Justification |
|---|---|
| Schéma outil aligné MCP | Interopérabilité native avec l'écosystème MCP (16K+ serveurs) |
| Résolution uniquement à `INITIALIZING` | Fail fast — erreurs prévisibles, pas de surprises runtime |
| `tools_required` vs `tools_optional` | Distinction criticité explicite : `STOPPED` vs `DEGRADED` |
| Sandbox par profil prédéfini | Simple à comprendre et tester, pas de config per-outil |
| MVP sans Docker | Zéro dépendance, fonctionne sur tout Linux, évolutif vers nsjail |
| Audit log SQLite local | Souveraineté complète, format lisible, zéro service externe |
| `network_allowlist` dans manifest | Principe du moindre privilège — whitelist explicite |

---

*Prochaine lecture recommandée : [Memory Engine](./Briques-Memory-Engine)*
