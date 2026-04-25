# MCP — Intégration — Apollia OS

> Comment Apollia OS s'aligne avec le Model Context Protocol (MCP) et comment l'utiliser pour consommer des serveurs MCP depuis vos agents.
> Public cible : développeur d'agent, intégrateur

---

## Vue d'ensemble

Le Model Context Protocol (MCP, Anthropic, 2024) standardise la manière dont les outils sont exposés aux agents LLM. Apollia OS s'aligne sur MCP pour le Tool Registry : les outils sont décrits avec un schéma JSON-Schema compatible MCP, et les agents peuvent consommer des serveurs MCP via le `mcp_consumer`.

---

## Alignement Tool Registry ↔ MCP

Chaque outil dans le Tool Registry d'Apollia OS est décrit par un `ToolDescriptor` dont la structure reflète les outils MCP :

```rust
// apollia-tools/src/descriptor.rs
pub struct ToolDescriptor {
    pub name: String,              // identifiant unique — même convention que MCP
    pub version: String,           // semver
    pub description: String,       // texte humain pour le LLM
    pub kind: ToolKind,            // Native | Mcp | Custom
    pub input_schema: serde_json::Value,   // JSON Schema — compatible MCP
    pub output_schema: Option<serde_json::Value>, // JSON Schema optionnel
    pub sandbox_profile: SandboxProfile,
    pub tags: Vec<String>,
    pub dangerous: bool,
}
```

La différence principale : Apollia OS ajoute `sandbox_profile` et `dangerous` — des métadonnées de sécurité absentes de MCP.

---

## Consommer un serveur MCP depuis un agent

Pour utiliser un outil exposé par un serveur MCP, déclarez-le dans le manifest avec le préfixe `mcp:` :

```python
def manifest(self):
    return {
        "name": "mon-agent",
        "tools_required": ["file_io"],       # outil natif
        "tools_optional": ["mcp:filesystem"], # outil MCP optionnel
    }
```

Le runtime tente de se connecter au serveur MCP `filesystem` à `INITIALIZING`. Si la connexion échoue et que l'outil est `optional`, l'agent passe en `DEGRADED` plutôt que de ne pas démarrer.

### Configuration du serveur MCP

Dans `~/.apollia/mcp.toml` (disponible) :

```toml
[[servers]]
name      = "filesystem"
transport = "stdio"
command   = "npx"
args      = ["-y", "@modelcontextprotocol/server-filesystem", "/data"]

[[servers]]
name    = "brave-search"
transport = "stdio"
command = "npx"
args    = ["-y", "@modelcontextprotocol/server-brave-search"]
requires_approval = true

[servers.env]
BRAVE_API_KEY = "${BRAVE_API_KEY}"
```

Voir [MCP — Guide utilisateur](./MCP-Guide-Utilisateur) pour la référence complète des champs.

### Appeler un outil MCP depuis l'agent

```python
async def run(self, task, ctx):
    # Identique à un outil natif — le runtime gère le transport MCP
    result = await ctx.tools.call("mcp:filesystem", {
        "action": "read_file",
        "path": "/data/rapport.pdf"
    })

    # Lister les outils MCP disponibles
    tools = await ctx.tools.list_tools()
    mcp_tools = [t for t in tools if t["name"].startswith("mcp:")]
```

> Le préfixe `mcp:` est ajouté automatiquement par le runtime. Les outils MCP passent par le même `ResilienceLayer` et `AuditTrail` que les outils natifs. Voir [Tool Registry — MCP](./Briques-Tool-Registry) pour l'architecture interne.

---

## Ajouter / supprimer un serveur MCP à chaud

Les serveurs MCP peuvent être ajoutés ou retirés sans redémarrer le runtime :

### Via CLI

```bash
# Ajouter un serveur MCP
apollia-os mcp add --name filesystem --command "npx" --args "@modelcontextprotocol/server-filesystem" --args "/data"

# Tester la connexion
apollia-os mcp test filesystem

# Supprimer un serveur MCP
apollia-os mcp remove filesystem
```

### Via API REST

```bash
# Ajouter
curl -X POST http://localhost:7771/api/v1/mcp/servers \
  -H "Content-Type: application/json" \
  -d '{"name": "filesystem", "command": "npx", "args": ["@modelcontextprotocol/server-filesystem", "/data"]}'

# Supprimer
curl -X DELETE http://localhost:7771/api/v1/mcp/servers/filesystem
```

### Via l'application Desktop

Page **Intégrations** → onglet MCP → bouton "Ajouter un serveur" ou icône de suppression sur un serveur existant.

Le `McpConfigWriter` valide la configuration (échec si le serveur existe déjà ou n'est pas trouvé), met à jour `mcp.toml`, et notifie le `McpClientManager` qui connecte/déconnecte le transport en temps réel.

> Voir aussi [Tool Registry — MCP](./Briques-Tool-Registry) pour l'architecture interne (`McpClientManager`, `McpToolExecutor`).

---

## Exposer Apollia OS comme serveur MCP

Un agent Apollia OS avec `supports_a2a: True` génère automatiquement une AgentCard A2A. L'exposition comme serveur MCP est une fonctionnalité prévue en v0.3.

---

## État de l'implémentation

| Fonctionnalité | Statut |
|---|---|
| Tool Registry aligné JSON Schema MCP | ✅ v0.1 |
| Client MCP natif (`apollia-mcp`) | ✅ v0.2 |
| Transport stdio MCP | ✅ v0.2 |
| HITL gate à deux niveaux (serveur + agent) | ✅ v0.2 |
| Mutations dynamiques à chaud (API REST) | ✅ v0.2 |
| Configuration `~/.apollia/mcp.toml` | ✅ v0.2 |
| Exposition serveur MCP | v0.3 (planifié) |
| Transport HTTP+SSE MCP | v0.3 (planifié) |

---

## Voir aussi

- [A2A ACP Alignement](./A2A-ACP-Alignement) — alignement avec les autres standards
- [Architecture Protocoles Standards](./Architecture-Protocoles-Standards) — vision globale de l'alignement
- [Briques Tool Registry](./Briques-Tool-Registry) — Tool Registry complet
