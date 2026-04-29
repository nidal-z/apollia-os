# Matrice de décision — Capabilities Apollia OS

> Quand utiliser un MCP Tool, un Worker Agent, le Mode Direct ou le Mode Orchestré ?

Ce document est destiné aux **builders** qui créent ou composent des agents avec Apollia OS. Il synthétise les décisions architecturales formalisées dans ADR-048 et ADR-049.

---

## Quand utiliser quoi ?

| # | Situation | Mécanisme recommandé | Pourquoi | Exemple |
|---|---|---|---|---|
| 1 | Tâche atomique, 1 seul appel, résultat déterministe | **MCP Tool** | Zéro overhead agent, latence minimale, pas besoin de LLM | Convertisseur de format, extraction d'un champ JSON, appel API REST |
| 2 | Tâche agentique standard, logique métier dans le code `run()`, modèle moyen ou frontier | **ORIA Mode Direct** | L'agent contrôle sa propre boucle ReAct, guardrails codés dans `run()` | Code reviewer automatique, agent d'onboarding, analyse de logs |
| 3 | Tâche complexe single-agent, plan LLM incertain, résilience runtime critique | **ORIA Mode Orchestré** | ORIA applique StepBudget par step, persistance SQLite, replanification × 2 — indépendamment du code Python de l'agent | Agents déclaratifs purs, tâches longues où une panne partielle ne doit pas tout relancer |
| 4 | Domaine spécialisé (Excel, SQL, PDF…), modèle léger (7-14B), guardrails non-négociables | **Worker Agent** | Expertise encodée dans le code — model-agnostic, résistant à la fenêtre de contexte courte | `excel-worker`, `csv-data-worker`, `sql-worker`, `pdf-worker` |
| 5 | Composition dynamique, plusieurs agents, routing piloté par le LLM à runtime | **A2A** | Discovery via `skill_id`, invocation synchrone, résultat structuré — le routing est porté par le modèle, pas par le développeur | Director Agent → `excel-worker` via `skill_id="read-excel"` |

---

## Critères formels — Worker Agent vs MCP Tool

Un **Worker Agent** est justifié si **au moins 2 des 3 conditions** suivantes sont réunies :

### Condition 1 — Séquence non-triviale

Le LLM doit ordonnancer plusieurs étapes spécifiques au domaine (ouverture, lecture, transformation, sauvegarde). Sur un modèle 7-14B, cet ordonnancement peut être mal exécuté : méthodes hallucintées, étapes oubliées, ordre inversé.

> Exemple positif : lecture d'un `.xlsx` → sélection de la feuille → calcul de totaux → réécriture. Chaque étape dépend de la précédente avec des contraintes `openpyxl` précises.

### Condition 2 — Guardrails de domaine critiques

Des règles non-négociables s'appliquent à ce domaine et **doivent être dans le code**, pas dans le contexte (le contexte peut être ignoré ou tronqué sur modèles légers).

> Exemple positif : "Ne jamais modifier un `.xlsx` avec `bash_executor` — le format est une archive ZIP et toute écriture directe corrompt le fichier." Cette règle dans un `SYSTEM_PROMPT` constant ne peut pas être oubliée.

### Condition 3 — Pattern d'erreur domaine récurrent

Des erreurs métier prévisibles et spécifiques à ce domaine doivent être gérées dans le code, pas découvertes à runtime par le LLM.

> Exemple positif : `zipfile.BadZipFile` pour `.xlsx` corrompu, encodage `latin-1` pour CSV anciens, timeout réseau avec retry pour SQL distant.

### Règle de décision

```
2 conditions sur 3 → Worker Agent
moins de 2 → MCP Tool (ou ORIA Mode Direct si séquence légère)
```

---

## Arbre de décision

```
La tâche est-elle atomique (1 appel, résultat direct) ?
  │ oui → MCP Tool
  │ non
  ▼
La tâche nécessite-t-elle plusieurs agents composés dynamiquement ?
  │ oui → A2A (routing par skill_id)
  │ non — single-agent
  ▼
Domaine spécialisé + modèle léger + guardrails critiques (≥ 2/3 critères) ?
  │ oui → Worker Agent
  │ non
  ▼
La résilience runtime est-elle critique (tâche longue, panne partielle inacceptable) ?
  │ oui → ORIA Mode Orchestré
  │ non → ORIA Mode Direct
```

---

## Statut des mécanismes

| Mécanisme | Statut | Disponible |
|---|---|---|
| MCP Tool | ✅ Stable | |
| ORIA Mode Direct | ✅ Stable | |
| ORIA Mode Orchestré | ✅ Stable | |
| Worker Agent | ✅ Stable | |
| A2A (discovery + invocation) | ✅ Livré | |
| Pipeline TOML | ❌ Retiré v0.1.0 | — rebuild prévu v1.0 (spec n8n-like, voir ADR-085) |

---

## Références

- [ADR-048 — Worker Agents : expertise de domaine compilée](../adr/ADR-048-worker-agents-expertise-domaine.md)
- [ADR-049 — Routing A2A inter-agents](../adr/ADR-049-a2a-routing-inter-agents.md)
- [Benchmark : Worker Agent vs generic-agent](../benchmarks/worker-agent-benchmark.md)
- [Worker Agent Pattern](Worker-Agent-Pattern.md)
- [Architecture des Capabilities (idéation §5)](../internal/strategy/capabilities-architecture-ideation.md)
