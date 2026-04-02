# Outils MCP externes

Le Model Context Protocol (MCP) est un standard ouvert qui permet à n'importe quel outil externe d'être consommé par n'importe quel agent. L'écosystème MCP compte des milliers de serveurs couvrant Notion, GitHub, bases de données, APIs propriétaires, et plus encore.

Apollia OS intègre nativement les serveurs MCP : une fois configuré dans `mcp.toml`, un serveur MCP est indiscernable d'un outil natif depuis votre agent Python.

---

## Comment ça fonctionne

Au démarrage, Apollia OS lit `~/.apollia/mcp.toml`. Pour chaque serveur déclaré, il :

1. Démarre le processus serveur (stdio) et effectue le handshake MCP
2. Demande la liste des outils via `tools/list`
3. Enregistre chaque outil dans le Tool Registry avec le préfixe `mcp:<serveur>/<outil>`

Un outil MCP comme `search` sur le serveur `notion` devient `mcp:notion/search` dans le registry. Depuis votre agent, vous l'appelez exactement comme un outil natif :

```python
result = await ctx.tools.call("mcp:notion/search", {"query": "rapport Q3"})
```

---

## Configurer un serveur MCP

Créez ou éditez `~/.apollia/mcp.toml` :

```toml
# Serveur filesystem MCP (Node.js)
[[servers]]
name      = "filesystem"
transport = "stdio"
command   = "npx"
args      = ["-y", "@modelcontextprotocol/server-filesystem", "/data"]

# Serveur Brave Search avec approbation humaine requise
[[servers]]
name              = "brave-search"
transport         = "stdio"
command           = "npx"
args              = ["-y", "@modelcontextprotocol/server-brave-search"]
requires_approval = true

[servers.env]
BRAVE_API_KEY = "${BRAVE_API_KEY}"

# Serveur SQLite local
[[servers]]
name    = "sqlite"
transport = "stdio"
command = "npx"
args    = ["-y", "@modelcontextprotocol/server-sqlite", "--db-path", "/data/app.db"]
```

Chaque entrée `[[servers]]` a :

| Champ | Obligatoire | Description |
|---|---|---|
| `name` | oui | Identifiant unique — devient le préfixe `mcp:<name>/` |
| `transport` | oui | `"stdio"` uniquement en v0.2 |
| `command` | oui | Exécutable du serveur MCP |
| `args` | non | Arguments passés au serveur |
| `requires_approval` | non | `true` → approbation humaine avant chaque appel |
| `[servers.env]` | non | Variables d'environnement injectées |

---

## Déclarer un outil MCP dans le manifest

```python
def manifest(self):
    return {
        "name": "file-assistant-v2",
        "version": "2.0.0",
        "description": "Assistant fichier avec recherche Brave",
        "tools_required": ["file_read", "file_write"],
        "tools_optional": ["mcp:brave-search/brave_web_search"],  # optionnel
        "max_concurrent_tasks": 1,
        "step_budget": 15,
    }
```

Les outils MCP peuvent être `tools_required` ou `tools_optional` — les mêmes règles s'appliquent. Si le serveur MCP ne démarre pas au démarrage et que l'outil est `required`, l'agent passe en `STOPPED`. Si `optional`, il passe en `DEGRADED`.

---

## Appeler un outil MCP

```python
async def run(self, task, ctx):
    user_text = task["input"]["parts"][0]["text"]

    # Vérifier que l'outil optionnel est disponible
    if "mcp:brave-search/brave_web_search" in ctx.tools.list_tools():
        # Chercher des informations complémentaires en ligne
        search = await ctx.tools.call("mcp:brave-search/brave_web_search", {
            "query": f"{user_text} site:gov.fr filetype:pdf",
            "count": 5,
        })
        web_context = search.get("results", [])
    else:
        web_context = []

    # Continuer avec les outils natifs...
```

La syntaxe est identique à un outil natif — `ctx.tools.call("mcp:<serveur>/<outil>", {...})`. Le runtime gère le transport JSON-RPC MCP de manière transparente.

---

## Gérer requires_approval

Quand un serveur est configuré avec `requires_approval = true`, chaque appel à un de ses outils déclenche une suspension automatique de la tâche — le runtime attend une approbation humaine avant d'exécuter l'appel.

```bash
$ apollia-os run mon-agent "Cherche des infos sur Apollia OS"
  -> Task t-abc123 submitted
  Executing...
  ⏸ En attente d'approbation : appel mcp:brave-search/brave_web_search

$ apollia-os task approve t-abc123
  ✔ Approuvé — exécution reprise
  Done in 3.2s (1 step, 1 tool call)
```

Cela implémente le **HITL au niveau outil** — sans modifier le code de l'agent. Le champ `requires_approval` dans `mcp.toml` est une politique d'opérateur, pas une décision de l'agent.

---

## Ajouter et supprimer des serveurs à chaud

Les serveurs MCP peuvent être ajoutés ou retirés sans redémarrer le runtime :

```bash
# Ajouter
apollia-os mcp add \
  --name filesystem \
  --command "npx" \
  --args "@modelcontextprotocol/server-filesystem" \
  --args "/data"

# Tester la connexion
apollia-os mcp test filesystem

# Lister les serveurs actifs
apollia-os mcp list

# Supprimer
apollia-os mcp remove filesystem
```

La CLI met à jour `mcp.toml` et notifie le runtime qui connecte ou déconnecte le transport en temps réel. Les agents déjà démarrés voient les nouveaux outils apparaître dans `ctx.tools.list_tools()` après reconnexion.

---

## Trouver des serveurs MCP

L'écosystème MCP est ouvert. Points de départ :

- **Serveurs officiels Anthropic** : `@modelcontextprotocol/server-*` sur npm — filesystem, GitHub, Brave Search, Puppeteer, SQLite, et plus
- **Registre communautaire MCP** : `mcp.so` — catalogue de serveurs tiers
- **Construire le sien** : n'importe quel processus qui implémente le protocole MCP JSON-RPC peut être utilisé comme serveur

---

## Limites en v0.2

**Transport stdio uniquement** — les serveurs MCP HTTP+SSE ne sont pas encore supportés. Tous les serveurs MCP sont des sous-processus locaux gérés par le runtime.

**Pas d'exposition en serveur MCP** — Apollia OS peut consommer des serveurs MCP mais ne peut pas encore s'exposer lui-même comme serveur MCP. Cette fonctionnalité est prévue en v0.3.

**Naming `mcp:<serveur>/<outil>`** — le nom complet d'un outil MCP dépend du nom de serveur dans `mcp.toml` et du nom d'outil retourné par le serveur. Consultez `apollia-os tools list` pour voir les noms exacts disponibles.
