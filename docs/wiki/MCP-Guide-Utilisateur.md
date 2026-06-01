# MCP - Guide utilisateur

> Comment configurer des serveurs MCP pour étendre vos agents avec des outils externes.
> Public cible : utilisateur d'Apollia OS qui veut connecter Notion, SQLite, Brave Search ou tout autre serveur MCP.

---

## 1. Introduction

Le Model Context Protocol (MCP) est le standard d'interopérabilité des agents IA. Il permet à Apollia OS de parler à n'importe quel serveur MCP - plus de 16 000 existent sur GitHub - sans écrire de code d'intégration.

Quelques exemples de ce que vous pouvez faire :

- **Notion** : créer des pages, rechercher dans votre workspace
- **SQLite** : interroger une base de données locale
- **Brave Search** : effectuer des recherches web depuis vos agents
- **Filesystem** : lecture/écriture sur le système de fichiers hôte
- **GitHub, Slack, PostgreSQL** : tout serveur qui implémente MCP

Apollia OS gère le cycle de vie du processus serveur MCP (démarrage, handshake, arrêt), la découverte des outils, et leur enregistrement automatique dans le Tool Registry.

---

## 2. Prérequis

Le transport V1 est **stdio** : Apollia OS démarre le serveur MCP comme un sous-processus et communique via ses pipes stdin/stdout. Les deux lanceurs suivants sont les plus courants :

| Lanceur | Usage | Installation |
|---|---|---|
| `npx` | Serveurs Node.js (la majorité) | Node.js ≥ 18 - [nodejs.org](https://nodejs.org) |
| `uvx` | Serveurs Python | `pip install uv` ou [docs.astral.sh/uv](https://docs.astral.sh/uv) |

Vérification :

```bash
node --version   # v18.x ou supérieur
npx --version

uvx --version
```

---

## 3. Configuration - `~/.apollia/mcp.toml`

### 3.1 Emplacement

```
~/.apollia/mcp.toml
```

Ce fichier est lu au démarrage du runtime. Son absence est silencieuse : Apollia OS démarre normalement sans serveur MCP. Créez le fichier dès que vous voulez configurer un premier serveur.

### 3.2 Format

Le fichier contient une liste de blocs `[[servers]]`, un par serveur MCP :

```toml
[[servers]]
name            = "notion"
command         = "npx"
args            = ["-y", "@notionhq/notion-mcp-server"]
transport       = "stdio"
requires_approval = false
init_timeout_secs = 30
call_timeout_secs = 60
tags            = ["productivity"]

[servers.env]
NOTION_API_KEY = "${NOTION_API_KEY}"
```

### 3.3 Référence des champs

| Champ | Type | Défaut | Description |
|---|---|---|---|
| `name` | string | - | **Requis.** Identifiant unique du serveur. Caractères autorisés : `a-z`, `0-9`, `_`, `-`. Exemple : `"notion"`. |
| `command` | string | - | **Requis.** Exécutable à lancer. Généralement `"npx"` ou `"uvx"`. |
| `args` | string[] | `[]` | Arguments passés à la commande. |
| `env` | table | `{}` | Variables d'environnement injectées dans le processus serveur. Les valeurs acceptent `${VAR}`. |
| `transport` | string | `"stdio"` | Transport MCP. Seul `"stdio"` est supporté en V1. |
| `requires_approval` | bool | `false` | Quand `true`, chaque appel d'outil sur ce serveur est suspendu jusqu'à approbation HITL. |
| `init_timeout_secs` | int | `30` | Délai maximum pour que le handshake `initialize` aboutisse. |
| `call_timeout_secs` | int | `60` | Délai maximum pour chaque `tools/call`. |
| `tags` | string[] | `[]` | Tags attachés à tous les outils de ce serveur dans le Tool Registry. |

**Contrainte de nommage :** `name` doit être unique dans le fichier. Deux serveurs avec le même nom provoquent une erreur de démarrage.

---

## 4. Exemples

### 4.1 Notion

Accès en lecture/écriture à votre workspace Notion.

**Prérequis :** créer une intégration Notion sur [notion.so/my-integrations](https://www.notion.so/my-integrations) et noter le token.

```toml
[[servers]]
name    = "notion"
command = "npx"
args    = ["-y", "@notionhq/notion-mcp-server"]

[servers.env]
NOTION_API_KEY = "${NOTION_API_KEY}"
```

Export du token avant de lancer Apollia OS :

```bash
export NOTION_API_KEY="secret_xxxxxxxxxxxxxxxxxxxx"
```

Outils disponibles après connexion (exemples) : `mcp:notion/search`, `mcp:notion/create_page`, `mcp:notion/append_block_children`.

Usage depuis l'agent :

```python
async def run(self, task, ctx):
    results = await ctx.tools.call("mcp:notion/search", {
        "query": "rapport Q1 2026"
    })
```

### 4.2 SQLite local

Requêtes SQL sur une base SQLite locale.

```toml
[[servers]]
name    = "sqlite"
command = "uvx"
args    = ["mcp-server-sqlite", "--db-path", "/home/user/.apollia/data/agents.db"]
```

Aucune variable d'environnement requise. La base est créée automatiquement si elle n'existe pas.

Outils disponibles : `mcp:sqlite/query`, `mcp:sqlite/execute`, `mcp:sqlite/list_tables`.

Usage depuis l'agent :

```python
rows = await ctx.tools.call("mcp:sqlite/query", {
    "sql": "SELECT * FROM tasks WHERE status = 'open'"
})
```

### 4.3 Brave Search

Recherches web via l'API Brave Search.

**Prérequis :** clé API Brave Search - [brave.com/search/api](https://brave.com/search/api).

```toml
[[servers]]
name              = "brave-search"
command           = "npx"
args              = ["-y", "@modelcontextprotocol/server-brave-search"]
requires_approval = true

[servers.env]
BRAVE_API_KEY = "${BRAVE_API_KEY}"
```

`requires_approval = true` est recommandé : chaque recherche web est soumise à validation HITL avant émission.

```bash
export BRAVE_API_KEY="BSAxxxxxxxxxxxxxxxxxxxx"
```

Outil disponible : `mcp:brave-search/brave_web_search`.

Usage depuis l'agent :

```python
results = await ctx.tools.call("mcp:brave-search/brave_web_search", {
    "query": "Rust async runtime 2026",
    "count": 5
})
```

---

## 5. Variables d'environnement

### 5.1 Interpolation `${VAR}`

Les valeurs du bloc `[servers.env]` supportent la syntaxe `${NOM_DE_VARIABLE}` :

```toml
[servers.env]
API_KEY  = "${MON_API_KEY}"
BASE_URL = "https://${API_HOST}/v1"
```

La résolution se fait depuis l'environnement shell au moment du démarrage du runtime. Si une variable référencée est absente, le démarrage échoue avec une erreur explicite :

```
ERROR apollia_mcp::config: server 'notion': unresolved environment variable: ${NOTION_API_KEY}
```

### 5.2 Sécurité

- Ne jamais écrire de secret en clair dans `mcp.toml` - utiliser exclusivement `${VAR}`.
- Les env keys sont exposées par l'API REST (pour audit), mais pas leurs valeurs.
- `mcp.toml` doit avoir les permissions `600` : `chmod 600 ~/.apollia/mcp.toml`.

---

## 6. Approbation HITL

### 6.1 `requires_approval` au niveau serveur

Quand `requires_approval = true` est positionné sur un serveur, **tous** ses outils requièrent une approbation humaine avant exécution. L'appel est suspendu jusqu'à ce que l'approbation soit accordée ou refusée via l'API ou le dashboard.

```toml
[[servers]]
name              = "brave-search"
command           = "npx"
args              = ["-y", "@modelcontextprotocol/server-brave-search"]
requires_approval = true
```

### 6.2 `tools_requiring_approval` au niveau agent

Le manifest agent peut déclarer une liste d'outils MCP qui requièrent approbation, indépendamment de la configuration serveur :

```python
def manifest(self):
    return {
        "name": "research-agent",
        "tools_required": ["mcp:brave-search/brave_web_search"],
        "tools_requiring_approval": ["mcp:brave-search/brave_web_search"],
    }
```

Les deux mécanismes sont cumulatifs : si l'un ou l'autre est actif, l'approbation est requise.

### 6.3 Flux HITL

1. L'agent appelle un outil soumis à HITL.
2. Le runtime crée une `PendingApproval` et suspend l'exécution.
3. L'approbation est accordée via `POST /api/v1/approvals/:id/approve` ou refusée via `/reject`.
4. L'exécution reprend (ou échoue proprement en cas de refus).

---

## 7. Troubleshooting

### 7.1 Le serveur ne démarre pas

**Symptôme :** `ERROR apollia_mcp::session: failed to spawn server 'notion'`

**Causes fréquentes :**

- La commande (`npx`, `uvx`) n'est pas dans le `PATH`.
- Le package n'est pas installé (pour `npx -y`, une connexion Internet est nécessaire au premier lancement).
- Un argument est incorrect.

**Diagnostic :**

```bash
# Tester manuellement la commande
npx -y @notionhq/notion-mcp-server

# Vérifier que la commande est accessible
which npx
which uvx
```

Si la commande fonctionne manuellement mais pas depuis le runtime, vérifiez que l'environnement shell est sourcé avant de lancer `apollia start`.

---

### 7.2 Variable d'environnement manquante

**Symptôme :** `ERROR apollia_mcp::config: server 'notion': unresolved environment variable: ${NOTION_API_KEY}`

Le runtime refuse de démarrer si une variable référencée dans `[servers.env]` est absente.

**Solution :**

```bash
# Exporter la variable avant de lancer le runtime
export NOTION_API_KEY="secret_xxxx"
apollia start

# Ou via .bashrc / .zshrc pour une persistance automatique
echo 'export NOTION_API_KEY="secret_xxxx"' >> ~/.zshrc
source ~/.zshrc
```

---

### 7.3 Timeout du handshake

**Symptôme :** `ERROR apollia_mcp::session: server 'notion' initialize timed out after 30s`

Le processus serveur a démarré mais n'a pas répondu au message `initialize` MCP dans les 30 secondes.

**Causes fréquentes :**

- Premier lancement d'un serveur `npx` : le téléchargement du package peut dépasser le timeout.
- Le serveur attend une entrée supplémentaire au démarrage.
- Ressources système insuffisantes (RAM, CPU).

**Solution :**

Augmenter `init_timeout_secs` pour laisser le temps au premier téléchargement :

```toml
[[servers]]
name             = "notion"
command          = "npx"
args             = ["-y", "@notionhq/notion-mcp-server"]
init_timeout_secs = 120
```

Après le premier lancement, `npx` utilise le cache local - le timeout par défaut de 30 secondes est suffisant.

---

### 7.4 Outils non découverts

**Symptôme :** le serveur se connecte mais `apollia mcp status` affiche `tools_count: 0`.

**Causes fréquentes :**

- Le serveur requiert une configuration supplémentaire avant d'exposer ses outils (ex : token d'authentification invalide).
- Le serveur répond au `initialize` mais retourne une liste vide à `tools/list`.

**Diagnostic :**

```bash
# Tester la connexion et inspecter les outils découverts
curl -s -X POST http://127.0.0.1:7771/api/v1/mcp/servers/test \
  -H "Content-Type: application/json" \
  -d '{
    "name": "notion",
    "command": "npx",
    "args": ["-y", "@notionhq/notion-mcp-server"],
    "transport": "stdio",
    "env": {"NOTION_API_KEY": "secret_xxxx"}
  }'
```

La route `/test` effectue un handshake éphémère et retourne la liste des outils sans persister de session. Si `tools` est vide, le problème vient du serveur lui-même (configuration, permissions).

---

## 8. API REST

L'API HTTP expose la gestion des serveurs MCP sous `/api/v1/mcp/`. Toutes les routes retournent du JSON.

### 8.1 Lister les serveurs connectés

```
GET /api/v1/mcp/servers
```

Retourne un tableau de statuts. Tableau vide si aucun serveur n'est configuré.

```bash
curl http://127.0.0.1:7771/api/v1/mcp/servers
```

```json
[
  {
    "name": "notion",
    "server_info": "notion-mcp-server 1.0.0",
    "tools_count": 8,
    "requires_approval": false,
    "connected": true,
    "pid": 12345,
    "uptime_secs": 3600,
    "last_call_at": "2026-06-15T10:30:00Z",
    "error": null,
    "package": "@notionhq/notion-mcp-server",
    "transport": "stdio"
  }
]
```

### 8.2 Détail d'un serveur

```
GET /api/v1/mcp/servers/:name
```

Retourne le statut complet, la liste des outils, et la configuration (secrets redactés).

```bash
curl http://127.0.0.1:7771/api/v1/mcp/servers/notion
```

### 8.3 Ajouter un serveur à chaud

```
POST /api/v1/mcp/servers
Content-Type: application/json
```

Démarre immédiatement le processus serveur et persiste la configuration dans `mcp.toml`.

```bash
curl -X POST http://127.0.0.1:7771/api/v1/mcp/servers \
  -H "Content-Type: application/json" \
  -d '{
    "name": "sqlite",
    "command": "uvx",
    "args": ["mcp-server-sqlite", "--db-path", "/home/user/data.db"],
    "transport": "stdio",
    "requires_approval": false
  }'
```

Retourne `201 Created` avec le statut du serveur.

### 8.4 Supprimer un serveur

```
DELETE /api/v1/mcp/servers/:name
```

Arrête le processus et supprime l'entrée de `mcp.toml`.

```bash
curl -X DELETE http://127.0.0.1:7771/api/v1/mcp/servers/sqlite
```

### 8.5 Redémarrer un serveur

```
POST /api/v1/mcp/servers/:name/restart
```

Arrête la session en cours et en démarre une nouvelle avec la configuration existante. Utile après un crash ou une mise à jour du package.

```bash
curl -X POST http://127.0.0.1:7771/api/v1/mcp/servers/notion/restart
```

### 8.6 Mettre à jour la configuration d'un serveur

```
PUT /api/v1/mcp/servers/:name/config
Content-Type: application/json
```

Remplace la configuration et redémarre automatiquement la session. L'ordre des serveurs dans `mcp.toml` est préservé.

### 8.7 Tester une configuration sans la persister

```
POST /api/v1/mcp/servers/test
Content-Type: application/json
```

Effectue un handshake éphémère, liste les outils, puis arrête le processus. Le Tool Registry n'est pas modifié. Utile pour valider une nouvelle configuration avant de l'ajouter.

```bash
curl -X POST http://127.0.0.1:7771/api/v1/mcp/servers/test \
  -H "Content-Type: application/json" \
  -d '{
    "name": "brave-search",
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-brave-search"],
    "transport": "stdio",
    "env": {"BRAVE_API_KEY": "${BRAVE_API_KEY}"}
  }'
```

---

## Voir aussi

- [Briques Tool Registry](./Briques-Tool-Registry) - architecture du Tool Registry et section outils MCP
- [MCP Integration](./MCP-Integration) - alignement Apollia OS ↔ standard MCP
- [Sécurité - Local-first](./Securite-Local-First) - principes de souveraineté des données
- [API HTTP - Index](./API-HTTP-Reference) - référence complète de l'API REST (voir [API-HTTP-Observability](./API-HTTP-Observability#mcp--adr-044) pour la section MCP)
