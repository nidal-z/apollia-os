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

**Endpoint REST** : `GET /api/v1/a2a/agents` — liste les AgentCards avec leurs skills.

```bash
# CLI — lister uniquement les agents A2A
$ apollia-os agent list --supports-a2a
```

Décision architecturale : [ADR-049 — Routing A2A inter-agents](../adr/ADR-049-a2a-routing-inter-agents.md)

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
