# Protocoles & Standards - MCP, A2A, ACP

> *Comment Apollia OS s'aligne avec les standards émergents de l'écosystème agents IA sans les réinventer.*

---

## 1. Philosophie - Aligner, pas réinventer

La règle fondamentale du projet est simple : **si un standard existe et est adopté par la communauté, Apollia OS le respecte plutôt que d'en inventer un concurrent.**

Cette philosophie a plusieurs bénéfices :

- **Interopérabilité gratuite** : Un utilisateur qui a déjà des outils MCP, des agents A2A, ou un déploiement ACP bénéficie immédiatement de la compatibilité.
- **Réduction du scope** : Définir un protocole de zéro est un travail massif de standardisation communautaire. Implémenter un protocole existant est un travail d'ingénierie.
- **Crédibilité** : Un projet qui respecte les standards est plus facile à intégrer dans des pipelines existants.

---

## 2. MCP - Model Context Protocol

### 2.1 Ce qu'est MCP

MCP (Model Context Protocol) est un standard développé initialement par Anthropic (novembre 2024) et passé sous l'égide de la Linux Foundation (début 2025). Il standardise la connexion entre un agent IA et ses outils/données.

**Techniquement :**
- JSON-RPC 2.0 inspiré du Language Server Protocol
- 3 primitives serveur : `Resources` (données contextuelles), `Prompts` (templates), `Tools` (fonctions exécutables)
- Spécification 2024-11-05 : exécution asynchrone, Streamable HTTP, orientation production
- Écosystème en croissance rapide avec des milliers de serveurs MCP disponibles

**Schéma d'un outil MCP :**
```json
{
  "name": "read_file",
  "description": "Lit le contenu d'un fichier",
  "inputSchema": {
    "type": "object",
    "properties": {
      "path": { "type": "string" }
    },
    "required": ["path"]
  }
}
```

### 2.2 Comment Apollia OS intègre MCP

**Consommation native via `mcp_consumer`**

L'outil natif `mcp_consumer` permet à un agent de consommer n'importe quel serveur MCP :

```python
# Dans le manifest
AgentManifest(
    tools_required=["mcp:filesystem", "mcp:database"],
    ...
)

# Dans l'agent
result = await ctx.tools.mcp_filesystem.read_file(path="/docs/rapport.pdf")
```

Le préfixe `mcp:` dans `tools_required` indique au runtime de résoudre l'outil via un serveur MCP. La connexion est établie à `INITIALIZING`, maintenue pendant toute la durée de vie de l'agent.

**Alignement du Tool Registry sur le schéma MCP**

Le `ToolDescriptor` d'Apollia OS utilise le même schéma d'outil que MCP (`inputSchema` JSON Schema). Un outil natif Apollia OS et un outil MCP sont syntaxiquement interchangeables dans le catalogue.

**Enregistrement de serveurs MCP custom**

```toml
# apollia.toml
[[mcp_servers]]
name = "mon-erp"
type = "http"
url = "http://localhost:3000/mcp"

[[mcp_servers]]
name = "filesystem"
type = "stdio"
command = "npx @modelcontextprotocol/server-filesystem /workspace"
```

### 2.3 Ce que MCP ne résout pas (et qu'Apollia OS résout)

MCP est un protocole de communication, pas un runtime d'exécution. Il définit comment un agent parle à un outil - pas comment l'outil est isolé, audité, ou protégé par des circuit breakers.

Apollia OS enveloppe les outils MCP dans son `ResilienceLayer` et son `AuditTrail` - toute invocation MCP est auditée et protégée exactement comme un outil natif.

---

## 3. A2A - Agent-to-Agent Protocol

### 3.1 Ce qu'est A2A

A2A est un standard de communication agent-à-agent co-développé par Google et adopté par la Linux Foundation (v1.0-rc, 2025). Il définit comment un agent découvre et communique avec d'autres agents.

**Composants clés :**
- `AgentCard` : métadonnées JSON de découverte exposées à `/.well-known/agent.json`
- `Task` : unité de travail stateful avec lifecycle (submitted → working → completed...)
- `Message` : tour de communication
- `Artifact` : sortie d'une tâche
- `Part` : unité minimale de données (TextPart, FilePart, DataPart)

**Exemple AgentCard A2A :**
```json
{
  "name": "devis-generator",
  "description": "Génère des devis commerciaux",
  "version": "1.0.0",
  "url": "http://localhost:7771/a2a/devis-generator",
  "capabilities": {
    "streaming": false,
    "pushNotifications": false
  },
  "skills": [
    {
      "id": "generate-quote",
      "name": "Génération de devis",
      "description": "Génère un devis à partir d'une description"
    }
  ]
}
```

### 3.2 Comment Apollia OS intègre A2A

**Génération automatique d'AgentCard**

Si un agent déclare `supports_a2a=True` dans son manifest, Apollia OS génère automatiquement une AgentCard A2A compatible et l'expose à `/.well-known/agent.json`. L'agent n'écrit pas de code A2A.

```python
AgentManifest(
    name="devis-generator",
    supports_a2a=True,  # ← Apollia OS fait le reste
    skills=[
        AgentSkill(id="generate-quote", name="Génération de devis")
    ]
)
```

**Alignement du TaskState**

Les états de tâche AIP (`submitted`, `working`, `completed`, `failed`, `input_required`, `canceled`) sont directement alignés sur le `TaskState` A2A. Pas de mapping - ce sont les mêmes états.

**Communication inter-agents**

Un agent dans Apollia OS peut soumettre des tâches à d'autres agents A2A via l'`http_client` natif :

```python
# L'agent "orchestrateur" délègue à un agent "spécialiste" A2A externe
response = await ctx.tools.http_client.post(
    url="http://agent-specialist/.well-known/agent.json",
    json={"task": "..."}
)
```

### 3.3 Ce que A2A ne résout pas

A2A est un protocole de découverte et communication. Il ne définit pas comment l'agent est exécuté, isolé, ou supervisé localement. Apollia OS est le substrate d'exécution local ; A2A est l'interface de communication externe.

---

## 4. ACP - Agent Communication Protocol

### 4.1 Ce qu'est ACP

ACP (Agent Communication Protocol) a été développé par IBM dans le contexte du framework BeeAI, puis a fusionné avec A2A sous l'égide de la Linux Foundation (septembre 2025). Sa contribution principale est la définition du **lifecycle processus** d'un agent en tant que service.

**Lifecycle processus ACP :**
```
INITIALIZING → ACTIVE → DEGRADED → RETIRING → RETIRED
```

**Caractéristiques :**
- Communication REST (pas JSON-RPC)
- Découverte offline (métadonnées dans les packages)
- Structure de message MIME-type extensible

### 4.2 Comment Apollia OS intègre ACP

**Alignement du ProcessState**

Le `ProcessState` d'Apollia OS est directement inspiré du lifecycle ACP :

| ACP | Apollia OS | Sémantique |
|---|---|---|
| `INITIALIZING` | `INITIALIZING` | Démarrage, résolution dépendances |
| `ACTIVE` | `ACTIVE` | Prêt à traiter des tâches |
| `DEGRADED` | `DEGRADED` | Actif avec capacités réduites |
| `RETIRING` | `STOPPING` | Drain des tâches en cours |
| `RETIRED` | `STOPPED` | Arrêt complet |

**Distinction critique : deux machines d'état**

ACP définit le lifecycle du **processus**. A2A définit le lifecycle de la **tâche**. Ce sont deux machines d'état indépendantes. Un processus `ACTIVE` peut avoir zéro ou plusieurs tâches `working` simultanément.

Apollia OS maintient cette distinction explicitement - pas d'ambiguïté entre "est-ce que l'agent est prêt ?" (ProcessState) et "est-ce que la tâche est terminée ?" (TaskState).

---

## 5. Tableau de synthèse

| Standard | Rôle | Intégration Apollia OS |
|---|---|---|
| **MCP** | Communication agent ↔ outil/données | `mcp_consumer` natif, `ToolDescriptor` aligné MCP, serveurs MCP configurable dans `apollia.toml` |
| **A2A** | Communication agent ↔ agent | Génération automatique AgentCard si `supports_a2a=True`, TaskState aligné A2A |
| **ACP** | Lifecycle processus agent | ProcessState aligné ACP, distinction processus/tâche explicite |

**Ce qu'Apollia OS n'implémente pas (par design) :**
- Pas de registry MCP central hébergé - les serveurs MCP sont configurés localement
- Pas de discovery A2A cloud - l'AgentCard est exposée localement
- Pas de compatibilité ACP REST complète en v0.1 - uniquement l'alignement sémantique des états

Ces fonctionnalités enterprise sont dans la [Roadmap](./Roadmap).

---

*Prochaine lecture recommandée : [Tool Registry](./Briques-Tool-Registry)*
