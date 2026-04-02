# A2A / ACP — Alignement — Apollia OS

> Comment Apollia OS s'aligne avec Agent-to-Agent (Google A2A) et Agent Communication Protocol (ACP).
> Public cible : architecte, développeur d'agent

---

## Vue d'ensemble

Apollia OS s'aligne sur les standards émergents de l'écosystème agent sans en dépendre. Le principe est d'être compatible par construction, pas par implémentation complète. Cela permet d'interopérer avec l'écosystème sans introduire des dépendances qui contrediraient le Principe #2 (Zéro dépendance externe).

Pour le détail complet des choix d'alignement : [Architecture Protocoles Standards](./Architecture-Protocoles-Standards).

---

## Agent-to-Agent (A2A) — Google, 2025

### Ce qu'Apollia OS implémente

**TaskState alignée A2A :**

Les valeurs de `TaskStatus` dans Apollia OS correspondent directement aux états A2A :

| Apollia OS | A2A TaskState |
|---|---|
| `submitted` | `submitted` |
| `working` | `working` |
| `completed` | `completed` |
| `failed` | `failed` |
| `input_required` | `input-required` |
| `canceled` | `canceled` |

**AIPTask / AIPResult alignés A2A :**

La structure `AIPTask` reprend les concepts A2A (`task_id`, `context_id`, `parts`). `AIPResult` expose les `artifacts` (A2A-compatible).

**AgentCard automatique :**

Si un agent déclare `supports_a2a: True`, Apollia OS génère automatiquement une AgentCard A2A et l'expose à `/.well-known/agent.json` :

```bash
curl http://localhost:7771/.well-known/agent.json
```

```json
{
  "name": "devis-generator",
  "description": "Génère des devis commerciaux",
  "url": "http://localhost:7771",
  "skills": [
    {
      "id": "generate-quote",
      "name": "Génération de devis",
      "inputModes": ["text", "data"],
      "outputModes": ["file", "text"]
    }
  ]
}
```

> **Convention de nommage :** Le manifest Python utilise `snake_case` (`input_modes`, `output_modes`). L'AgentCard JSON exposé via A2A utilise `camelCase` (`inputModes`, `outputModes`) conformément à la spec A2A. La conversion est automatique lors de la sérialisation.

L'agent n'écrit pas de code A2A — il déclare ses capacités dans le manifest, Apollia OS fait le reste.

### Routing A2A V1 — livré Sprint 30

**A2AInvoker** — orchestrateur de haut niveau dans `apollia-runtime/src/a2a/` :

```python
# Depuis un Director Agent — invoquer un Worker par skill_id
result = await ctx.a2a_invoke("read-excel", {"text": "Lis ventes.xlsx"})
```

Flux : `SkillIndex.resolve(skill_id)` → validation état `Active` → construction contexte A2A (trust model) → délégation via `TaskRouter` avec timeout 120s → résultat `A2AInvocationResult`.

**SkillIndex** — index inversé `skill_id → agent_name` intégré à l'`AgentRegistry` :
- Alimenté automatiquement lors des `register()` / `unregister()` (agents avec `supports_a2a: True`)
- Conflit de skill_id détecté au `register()` — pas au runtime (Principe #4 — fail fast)
- `A2AError::SkillNotFound` inclut la liste des skills disponibles si résolution échoue

**Trust model A2A** (ADR-049) :
- L'agent invoqué lit la mémoire utilisateur globale (`__user__`) **en lecture seule**
- Les écritures restent confinées au namespace propre de l'agent invoqué
- Encodé dans `RuntimeContextConfig { user_memory_read_only: bool }`

**Endpoints REST** :

| Endpoint | Description |
|---|---|
| `GET /api/v1/a2a/agents` | Liste les agents A2A actifs avec leurs skills |
| `GET /api/v1/a2a/skills` | Liste plate de tous les skills A2A disponibles |
| `POST /api/v1/a2a/delegate` | Délégation synchrone bas-niveau (via `delegate_inner`) |
| `POST /api/v1/a2a/invoke` | Invocation haut-niveau avec garde-fous (via `A2AInvoker`) |

```bash
# CLI — lister uniquement les agents A2A
$ apollia-os agent list --supports-a2a
```

Décision architecturale : [ADR-049 — Routing A2A inter-agents](../adr/ADR-049-a2a-routing-inter-agents.md)

### Garde-fous A2A — livré Sprint 32

L'`A2AInvoker` applique trois garde-fous automatiques configurables via `A2AConfig` :

| Garde-fou | Défaut | Erreur retournée |
|---|---|---|
| Profondeur max de récursivité | 3 | `A2AError::MaxDepthExceeded` |
| Self-invocation (agent s'invoque lui-même) | Bloqué | `A2AError::SelfInvocation` |
| Timeout cumulé de la chaîne | 300s | `A2AError::ChainTimeoutExceeded` |

Configuration dans `apollia.toml` :

```toml
[a2a]
max_depth = 3
invocation_timeout_secs = 120
chain_timeout_secs = 300
```

Chaque déclenchement de garde-fou émet un `RuntimeEvent::A2AGuardTriggered` sur l'EventBus avec `guard_type`, `caller`, `skill_id` et `detail`.

Décision architecturale : [ADR-050 — Distribution Worker Agents](../adr/ADR-050-distribution-worker-agents.md)

### A2AToolsProvider — Workers comme outils ORIA — livré Sprint 32

`A2AToolsProvider` injecte dynamiquement les skills A2A comme des outils virtuels préfixés `a2a:` dans la boucle ReAct des agents ORIA :

```
ExecutionCoordinator
  └── start_task(agent_entry, aip_task)
        ├── ToolRegistry (outils natifs)
        └── A2AToolsProvider.build_tool_descriptors()
              → ajoute ToolDescriptor pour chaque skill A2A actif
```

Concrètement, un Director Agent ORIA voit les Workers comme des outils natifs :

```python
# Le LLM voit "a2a:read-excel" dans sa liste d'outils
# et peut l'appeler comme n'importe quel autre outil
ctx.tools.call("a2a:read-excel", {"text": "Lis ventes.xlsx"})
```

Le routing est transparent :
- Nom d'outil commençant par `a2a:` → `A2AInvoker.invoke(skill_id, ...)`
- Sinon → `ToolExecutor` natif (comportement inchangé)

**Backward-compatible** : sans agents A2A actifs, aucun outil `a2a:` n'apparaît dans la liste.

**Profondeur propagée** : le compteur `a2a_depth` est incrémenté à chaque invocation via outil A2A, empêchant les boucles infinies (Principe #7).

```rust
// crates/apollia-runtime/src/a2a/tools_provider.rs
pub struct A2AToolsProvider {
    a2a_invoker: Arc<A2AInvoker>,
}

impl A2AToolsProvider {
    /// Génère les descripteurs pour tous les skills A2A actifs.
    /// Chaque skill devient un ToolDescriptor avec :
    /// - name: "a2a:{skill_id}"
    /// - description: "{skill_description} (via {agent_name})"
    pub async fn build_tool_descriptors(&self) -> Vec<ToolDescriptor>;
}
```

### Erreurs A2A — Référence complète

Il existe deux enums d'erreur distincts selon la couche d'invocation :

#### `A2AError` — Invocateur haut niveau (`invoker.rs`)

Retourné par `A2AInvoker::invoke()` et `POST /api/v1/a2a/invoke`. Applique les garde-fous avant délégation.

| Variant | HTTP | Déclencheur |
|---|---|---|
| `SkillNotFound` | 404 | Aucun agent actif ne déclare le skill |
| `AgentNotActive` | 503 | Agent trouvé mais pas en état `Active` |
| `Timeout` | 504 | Délai d'invocation dépassé (`invocation_timeout_secs`) |
| `ExecutionFailed` | 502 | Worker Agent a retourné un échec |
| `RegistryError` | 500 | Erreur d'infrastructure (registry ou router) |
| `MaxDepthExceeded` | 429 | Profondeur max récursivité dépassée (`max_depth`) |
| `SelfInvocation` | 429 | Agent tente de s'invoquer lui-même |
| `ChainTimeoutExceeded` | 429 | Budget cumulé de chaîne dépassé (`chain_timeout_secs`) |

**Exemples JSON :**

```json
// SkillNotFound — 404
{
  "error": "skill 'read-excel' not found — available: [\"send-email\", \"parse-pdf\"]",
  "skill_id": "read-excel",
  "available_skills": ["send-email", "parse-pdf"]
}

// AgentNotActive — 503
{
  "error": "agent 'excel-reader' is not active (state: Degraded)"
}

// Timeout — 504
{
  "error": "A2A invocation timed out after 120s (skill: read-excel, agent: excel-reader)"
}

// ExecutionFailed — 502
{
  "error": "agent 'excel-reader' execution failed: file not found: ventes.xlsx"
}

// MaxDepthExceeded — 429
{
  "error": "a2a max depth 3 exceeded (current: 4, caller: director, skill: read-excel)"
}

// SelfInvocation — 429
{
  "error": "agent 'director' cannot invoke itself via skill 'summarize'"
}

// ChainTimeoutExceeded — 429
{
  "error": "a2a chain timeout exceeded (caller: director, skill: parse-pdf)"
}
```

#### `A2aError` — Délégation bas niveau (`mod.rs`)

Retourné par `delegate_inner()` et `POST /api/v1/a2a/delegate`. Couche sans garde-fous.

| Variant | HTTP | Déclencheur |
|---|---|---|
| `SkillNotFound` | 404 | Aucun agent actif ne déclare le skill (inclut liste des skills disponibles) |
| `AmbiguousSkill` | 409 | Plusieurs agents déclarent le même skill |
| `Registry` | 500 | Erreur du registry sous-jacent |
| `RouterDead` | 500 | TaskRouter mort ou indisponible |
| `Timeout` | 504 | Délégation expirée avant fin du Worker Agent |
| `WorkerFailed` | 502 | Worker Agent a retourné un échec |

**Exemples JSON :**

```json
// SkillNotFound — 404
{
  "error": "no active agent declares skill 'read-excel' (available: send-email, parse-pdf)",
  "skill_id": "read-excel",
  "available_skills": ["send-email", "parse-pdf"]
}

// AmbiguousSkill — 409
{
  "error": "skill 'parse-pdf' is declared by multiple agents: excel-reader, pdf-agent",
  "skill_id": "parse-pdf",
  "conflicting_agents": ["excel-reader", "pdf-agent"]
}

// WorkerFailed — 502
{
  "error": "worker agent failed: budget exhausted: 10/10 steps used"
}

// Timeout — 504
{
  "error": "delegation timed out after 120s"
}
```

> **Quelle surface utiliser ?** `POST /api/v1/a2a/invoke` (haut niveau) pour les intégrations externes — il applique les garde-fous. `POST /api/v1/a2a/delegate` (bas niveau) pour les scripts et les tests unitaires — pas de garde-fous, délégation directe.

### Ce qui n'est pas encore implémenté

- Authentification A2A (JWT, OAuth)
- Discovery de serveurs d'agents externes (cross-runtime)

---

## Agent Communication Protocol (ACP) — IBM Research

### Ce qu'Apollia OS implémente

**ProcessState alignée ACP :**

Le cycle de vie du processus agent est directement aligné sur ACP :

| Apollia OS | ACP AgentState |
|---|---|
| `INITIALIZING` | `initializing` |
| `ACTIVE` | `active` |
| `DEGRADED` | `degraded` |
| `STOPPING` | `stopping` |
| `STOPPED` | `stopped` |

C'est le modèle de lifecycle d'agent le plus complet disponible — cinq états distincts qui couvrent les cas de dégradation partielle.

### Ce qui n'est pas encore implémenté

- Endpoint ACP complet (`/agents/:id/state` format ACP)
- Négociation de capacités ACP

---

## Philosophie d'alignement

Apollia OS ne prétend pas implémenter complètement A2A ou ACP — ces standards évoluent encore rapidement. La stratégie est d'être **aligné sur les structures de données et les sémantiques** sans dépendre des SDKs ou des bibliothèques officielles.

Cela signifie que la migration vers A2A ou ACP complets, si et quand ces standards se stabilisent, sera un exercice d'ajout de routes et de formatage — pas une réécriture architecturale.

---

## Voir aussi

- [Architecture Protocoles Standards](./Architecture-Protocoles-Standards) — analyse complète des trois protocoles
- [Briques AIP Specification](./Briques-AIP-Specification) — AIPTask et AIPResult détaillés
- [Architecture Machines d'État](./Architecture-Machines-Etat) — ProcessState et TaskState
- [Worker Agent Pattern](./Worker-Agent-Pattern) — créer un Worker invocable via A2A
- [Matrice de décision — Capabilities](./Decision-Matrix-Capabilities) — quand utiliser A2A vs MCP vs Worker
- [ADR-049 — Routing A2A inter-agents](../adr/ADR-049-a2a-routing-inter-agents.md)
- [ADR-050 — Distribution Worker Agents](../adr/ADR-050-distribution-worker-agents.md)
- [Sécurité Guardrails](./Securite-Guardrails) — garde-fous A2A détaillés
- [Community Agent Registry](./Community-Agent-Registry) — registre communautaire
