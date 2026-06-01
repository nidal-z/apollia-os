# ADR-044 - Client MCP : architecture, transport, lifecycle

**Date :** 2026-03-29
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 26

---

## Contexte

MCP (Model Context Protocol) est devenu le standard d'interopérabilité des agents IA :
Claude Desktop, Cursor, Cline, VS Code Copilot et OpenAI Agents SDK le supportent tous.
16 000+ serveurs MCP existent sur le marché (GitHub, Notion, Slack, PostgreSQL, Brave Search…).

Apollia OS dispose de 10 outils natifs (Sprint 25) mais d'aucune intégration externe.
Sans client MCP, les agents Apollia sont isolés de cet écosystème. L'interopérabilité MCP
est identifiée comme la prochaine priorité stratégique.

**Contraintes structurantes :**

1. **Principe #1 - Local-first** : les données utilisateur ne doivent pas quitter la machine
   sans action explicite. Le transport MCP doit favoriser les processus locaux.
2. **Principe #2 - Zéro dépendance externe** : pas de SDK MCP tiers dans le binaire. Le
   client doit être implémenté nativement.
3. **Principe #5 - Un acteur, une responsabilité** : le client MCP doit être encapsulé dans
   sa propre crate, géré par un acteur Tokio dédié.
4. **Principe #8 - CLI humaine, API machine** : les serveurs MCP configurés doivent être
   introspectables via l'API REST (`/mcp/servers`).

Le transport MCP supporte deux modes : **stdio** (subprocess local) et **HTTP/SSE** (remote).
~90 % des serveurs communautaires (d'après le registre officiel) sont distribués comme
processus stdio (`uvx`, `npx`, binaire natif). Les serveurs HTTP sont plus rares et
introduisent des risques de sécurité supplémentaires (appels réseau distants, secrets exposés).

## Décision

Nous créons la crate `apollia-mcp` qui implémente un client MCP natif en Rust,
avec **transport stdio comme seul mode supporté en V1**.

Les décisions structurantes sont les suivantes :

### 1. Crate dédiée `apollia-mcp`

Le client MCP réside dans sa propre crate workspace. Il n'est pas intégré à `apollia-tools`
ni à `apollia-runtime`. Cette séparation suit le Principe #5 et permet de tester le client
de façon isolée.

### 2. Transport stdio uniquement en V1

Le runtime lance chaque serveur MCP comme sous-processus et communique via ses pipes stdin/stdout.
Deux tâches Tokio dédiées gèrent la lecture (stdout reader) et l'écriture (stdin writer) pour
éviter tout blocage. HTTP/SSE est laissé à V2.

### 3. Configuration via `mcp.toml`

L'utilisateur déclare les serveurs MCP dans `~/.apollia/mcp.toml` (format TOML).
Chaque entrée spécifie la commande de lancement, les arguments, les variables d'environnement,
et les flags HITL (`requires_approval`). Les secrets ne sont jamais écrits en clair :
ils sont interpolés depuis les variables d'environnement shell (`${VAR}`).

### 4. Naming `mcp:{server}/{tool}`

Les outils exposés par les serveurs MCP sont enregistrés dans le ToolRegistry existant avec
le préfixe `mcp:{server_name}/{tool_name}`. Ce naming évite les collisions avec les outils
natifs et reste lisible dans les manifests agents et dans l'API REST.

### 5. HITL à deux niveaux

Le contrôle Human-in-the-Loop s'applique à deux granularités :
- **Niveau serveur** : `requires_approval = true` dans `mcp.toml` suspend tous les appels
  aux outils de ce serveur en attente de confirmation.
- **Niveau agent** : `tools_requiring_approval` dans le manifest agent liste les outils
  spécifiques (ex. `mcp:notion/create_page`) qui nécessitent une approbation.

### 6. Lazy start des processus serveurs

Les serveurs MCP ne sont pas démarrés au lancement du runtime. Un processus serveur est
spawné à la première invocation d'un de ses outils par un agent (lazy start). Cette approche
évite de consommer des ressources pour des serveurs non utilisés.

### 7. Implémentation native du protocole JSON-RPC 2.0 + MCP

Pas de dépendance à un SDK MCP tiers. Le protocole JSON-RPC 2.0 est implémenté directement
dans `apollia-mcp` avec des types Rust sérialisés via `serde_json`. Le handshake MCP
(`initialize` / `initialized`) est géré dans `McpSession`.

## Alternatives considérées

### Option A - Embarquer un SDK MCP existant en Rust (rejetée)

**Pour :** Moins de code à maintenir, conformité automatique aux évolutions du protocole.

**Contre :** **Viole le Principe #2.** Aucun SDK MCP Rust officiel n'existe à ce jour
(mars 2026). Les crates tierces disponibles (`mcp-rs`, `rmcp`) sont expérimentales, non
maintenues, et ajoutent ~2-5 Mo de dépendances transitives. Rejetée : dépendance externe
non maîtrisée sur un protocole encore en évolution.

### Option B - Transport HTTP/SSE uniquement (rejetée)

**Pour :** API plus simple (pas de gestion de subprocess), protocole standardisé HTTP.

**Contre :** ~90 % des serveurs MCP communautaires sont distribués en stdio. HTTP/SSE
nécessite que le serveur soit déjà démarré et accessible - ce qui implique une gestion de
lifecycle externe. Pour les serveurs locaux (SQLite, filesystem), stdio est systématiquement
préféré par les auteurs. Rejetée : couvrirait une minorité des cas d'usage réels.

### Option C - Intégration directe dans `apollia-tools` (rejetée)

**Pour :** Moins de crates, pas d'ajout au workspace.

**Contre :** Mélange de responsabilités : `apollia-tools` gère les outils natifs (Rust pur),
`apollia-mcp` gère le cycle de vie de sous-processus, un protocole réseau, et des sessions
d'état. Les deux ont des patterns d'erreur, de test, et de dépendances orthogonaux.
Rejetée : violerait le Principe #5.

### Option retenue - `apollia-mcp` natif, stdio V1, `mcp.toml`, naming `mcp:`

**Pour :**
- Zéro dépendance externe : implémentation native complète sous notre contrôle
- Transport stdio : couvre ~90 % des serveurs MCP existants, reste local-first
- Crate dédiée : testable indépendamment, responsabilité unique
- Naming clair : `mcp:{server}/{tool}` lisible dans les logs, manifests, API REST
- HITL intégré : aligné avec le mécanisme d'approbation existant (Sprint 12)

**Compromis acceptés :**
- HTTP/SSE non supporté en V1 - serveurs distants (Brave Search via HTTP) inaccessibles
  jusqu'à V2
- Pas de reconnection automatique en V1 - si le sous-processus crash, la session est close
  et doit être redémarrée manuellement
- `toml` et `async-trait` ajoutés comme dépendances workspace

## Conséquences

**Positives :**
- L'écosystème MCP (16 000+ serveurs) devient accessible depuis les agents Apollia
- Les serveurs locaux (SQLite MCP, filesystem MCP) fonctionnent sans Internet
- Le HITL à deux niveaux assure la conformité au Principe #1 : aucune donnée ne sort
  sans action explicite de l'utilisateur
- La crate `apollia-mcp` est testable avec un mock MCP server Python sans impacter
  le reste du workspace
- L'API REST `/mcp/servers` expose l'état des serveurs (Principe #8)

**Négatives / Compromis :**
- Nouvelle crate à maintenir - interface publique à stabiliser avant V1.0
- Gestion des pipes stdio async : deux tâches Tokio par session, corrélation requête/réponse
  par `id` JSON-RPC - complexité accrue vs un appel HTTP simple
- Pas de reconnection en V1 : un crash du sous-processus MCP doit être détecté et reporté
  à l'agent (erreur explicite plutôt que hang silencieux)
- `tokio::process::Command` : dépendance au module process de Tokio - pas de problème pour
  Linux/macOS, à valider pour les targets futures

**À surveiller :**
- **Adoption stdio vs HTTP** : si V2 nécessite HTTP, évaluer si une abstraction de transport
  (`trait McpTransport`) est préférable à un feature flag
- **Stabilité du protocole MCP** : le protocole est encore en évolution (spec officielle
  Anthropic, mars 2024). Surveiller les breaking changes entre versions du spec
- **Zombie processes** : vérifier que `McpSession::shutdown()` tue proprement le subprocess
  même en cas de panic dans le reader/writer

## Principes architecturaux impactés

- **Principe #1 - Local-first** : respecté - transport stdio = subprocess local, secrets via
  variables d'environnement, HITL gate avant tout appel
- **Principe #2 - Zéro dépendance externe** : respecté - implémentation native, zéro SDK
  MCP tiers dans le binaire
- **Principe #5 - Un acteur, une responsabilité** : respecté - `McpClientManager` est un
  acteur Tokio dédié, `McpSession` gère une connexion unique, `McpToolExecutor` délègue
  l'exécution sans état partagé
- **Principe #8 - CLI humaine, API machine** : respecté - état des serveurs MCP exposé via
  `/mcp/servers` REST, listing dans `apollia mcp list`

## Liens

- ADR connexe : ADR-010 (Tool Registry - architecture de base, ToolKind::McpServer déjà défini)
- ADR connexe : ADR-023 (HITL - PendingApprovals, mécanisme d'approbation réutilisé)
- ADR connexe : ADR-015 (ToolExecutor trait - McpToolExecutor l'implémente)
- Spec de référence : `docs/specs/sprint-26-spec.md`
