# Annexe F — Vision et positionnement

---

## Le problème — La fracture entre développement et exécution

Depuis 2024, l'écosystème des agents IA Python a connu une croissance sans précédent. LangGraph, CrewAI, AutoGen, PydanticAI et des dizaines de variantes custom permettent à n'importe quel développeur de construire des agents sophistiqués en quelques heures.

Mais **construire un agent et l'exécuter en production de manière fiable** sont deux problèmes radicalement différents.

Chaque développeur qui dépasse le stade du notebook Jupyter se retrouve face aux mêmes obstacles :

| Problème | Manifestation |
|---|---|
| Isolation d'exécution | Code bash non maîtrisé = vecteur de risque. Docker requis = complexité opérationnelle |
| Gestion des outils | Chaque agent réimplémente file I/O, bash executor, HTTP client. Aucune standardisation |
| Mémoire persistante | Les solutions existantes sont soit des bases vectorielles cloud (coût, latence) soit des SQLite artisanaux sans structure |
| Résilience | Les agents sans circuit breakers et step budgets plantent silencieusement ou bouclent indéfiniment |
| Souveraineté | Les entreprises européennes refusent que leurs données transitent par des APIs cloud |
| Interopérabilité | MCP, A2A, ACP — des standards émergent mais leur adoption dans les runtimes est fragmentée |

---

## L'espace libre

```
                    LOCAL-FIRST
                        ▲
                        │
              Apollia OS│
              (cet espace│est libre)
                        │
FRAMEWORK    ───────────┼─────────────  FRAMEWORK
SPÉCIFIQUE              │               AGNOSTIC
                        │
              AgentScope │  (cloud-based
              (partiel)  │   solutions)
                        │
                        ▼
                    CLOUD-REQUIRED
```

Les solutions existantes couvrent soit le cloud (E2B, Daytona, Modal — sandboxes cloud), soit des frameworks spécifiques (LangGraph, CrewAI — pas de runtime d'exécution), soit de la complexité opérationnelle (K8s, Docker Swarm — inaccessible hors grande entreprise).

La combinaison **local-first + framework-agnostic + Tool Registry pluggable + mémoire SQLite ouverte + `cargo install`** n'existait pas.

---

## La solution — Apollia OS Runtime

**En une phrase :** Apollia OS est un runtime Rust open-source qui permet à n'importe quel agent IA Python de s'exécuter de manière isolée, souveraine, et outillée — avec un `pip install apollia_os` côté agent, et un binaire unique côté infrastructure.

### Ce que le runtime fournit

**Un contrat d'interface universel : l'AIP**

Duck typing Python. Deux méthodes. Zéro classe de base obligatoire.

```python
class MonAgent:
    def manifest(self):
        return {"name": "mon-agent", "version": "1.0.0", "tools_required": ["file_io"]}

    async def run(self, task, ctx):
        result = await ctx.tools.file_io.read("/data/rapport.txt")
        return {"task_id": task["task_id"], "status": "completed",
                "output": [{"type": "text", "text": result}]}
```

**Un catalogue d'outils prêts à l'emploi** — 10 outils natifs (`bash_executor`, `python_executor`, `file_read`, `file_write`, `http_client`, `mcp_consumer`…) + enregistrement d'outils custom.

**Une mémoire persistante souveraine** — 4 types (Working, Episodic, Semantic, Procedural) en SQLite local avec FTS5. Pas de cloud, pas de base vectorielle externe.

**Un moteur d'exécution intelligent : ORIA** — Mode Direct (boucle ReAct) + Mode Orchestré (Reasoner LLM + ActorLoop). StepBudget tri-dimensionnel et ResilienceLayer appliqués automatiquement.

**Ce que le runtime ne fait pas (par design) :** Pas de LLM intégré. Pas de framework imposé. Pas de cloud obligatoire. Pas de multi-tenancy. Pas d'interface graphique obligatoire.

---

## Pour qui

**Le développeur d'agents freelance ou en startup** — Installe le runtime une fois. Déploie ses agents via AIP. Se concentre sur la logique métier. Propose la mémoire persistante et l'isolation sandbox comme valeur ajoutée.

**L'entreprise européenne avec contraintes de souveraineté** — Installe le runtime sur un serveur Linux interne. Connecte des agents à des LLMs locaux (Ollama). Les données ne quittent jamais le réseau interne. L'audit trail SQLite donne la traçabilité réglementaire.

**Le développeur qui intègre le marketplace d'agents** — Publie un paquet PyPI qui implémente AIP. N'importe qui avec Apollia OS installé peut utiliser son agent immédiatement.

**Le chercheur ou étudiant en agents IA** — Infrastructure locale complète pour expérimenter avec des agents persistants, des outils réels, et des patterns de résilience — sans coût cloud.

---

## La validation du problème

Le problème n'est pas hypothétique :

- Les issues GitHub de LangGraph, CrewAI, AutoGen mentionnent régulièrement les problèmes de sandbox, mémoire persistante, et isolation
- Le protocole **MCP** d'Anthropic (nov. 2025, adopté par Linux Foundation) valide le besoin de standardisation des outils
- Le protocole **A2A** de Google (v1.0-rc, Linux Foundation) valide le besoin de standards de communication agent-à-agent
- **AgentScope Runtime** d'Alibaba (v1.1, fév. 2026) valide le besoin de runtimes framework-agnostics — mais leur solution reste couplée à leur écosystème et non conçue pour le déploiement local-first

L'espace reste ouvert pour une solution indépendante, local-first, et genuinement open-source.
