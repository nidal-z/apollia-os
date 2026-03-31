# ADR-048 — Worker Agents : expertise de domaine compilée dans le code Python

**Date :** 2026-03-31
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 29 (Pré-implémentation)

---

## Contexte

MCP (Sprint 26) livre 16 000+ outils tiers accessibles de façon transparente via `ToolKind::McpServer`. La couche outil est fondamentalement complète — les primitives sont couvertes et l'écosystème tiers est accessible.

Cependant, pour les tâches de domaine complexes (Excel, CSV, PDF, SQL...), il ne suffit pas d'avoir accès aux outils : le LLM doit aussi savoir **comment** les utiliser correctement — dans quel ordre appeler les méthodes, quels guardrails respecter, comment gérer les erreurs spécifiques au format. Sur les modèles frontier (Claude Opus, GPT-4o), injecter ces instructions en contexte ("skills Markdown") fonctionne. Mais la réalité des utilisateurs finaux d'Apollia est différente :

- Modèles 7-14B (Llama, Mistral, Qwen) sur hardware modeste
- Fenêtre de contexte limitée (4K-8K tokens en pratique)
- Fidélité moindre aux instructions longues sur tâches multi-étapes

Concrètement : un Llama 13B instruite via un "skill Excel" peut halluciner `openpyxl.open_workbook()` au lieu de `load_workbook()`, ignorer la règle "ne jamais modifier un .xlsx avec bash" (le format est une archive ZIP), ou perdre le fil après 3-4 étapes. L'injection Markdown dépend de l'intelligence du modèle — elle ne scale pas vers le bas.

**Pourquoi maintenant :** Sprint 28 a complété la migration SQLite-first. L'infrastructure runtime est stable. C'est le bon moment pour définir la stratégie capability avant de construire les premiers agents de domaine.

## Décision

Nous adoptons le pattern **Worker Agent** pour les capabilities de domaine complexes : des agents Python built-in dont l'expertise est **compilée dans le code**, pas injectée en contexte LLM à chaque appel.

Un Worker Agent :
1. Étend `WorkerAgent(BaseReActAgent)` — classe utilitaire dans `sdk/apollia/agents/worker.py`
2. Encode son expertise dans un `SYSTEM_PROMPT` constant (guardrails, patterns, imports)
3. Déclare `packages: list[str]` dans son manifest → pip installé au `INITIALIZING`
4. Déclare `supports_a2a: True` → composable via routing inter-agents
5. Utilise `MAX_STEPS` bas (6-10) et `TEMPERATURE` faible (0.0-0.2) — déterminisme
6. Gère les erreurs domaine via des codes stables (`"corrupted_file"`, `"sheet_not_found"`)

Le champ `packages` est ajouté à `AgentManifest` dans `apollia-core`. Le Supervisor câble `python_executor.setup_venv(manifest.packages)` pendant la phase `INITIALIZING` de chaque agent.

La classe `WorkerAgent` est une **convention**, pas une contrainte — le contrat AIP reste `manifest() + run()`.

## Alternatives considérées

### Option A — Skills Markdown (rejetée)

**Pour :** Flexible, zéro développement par domaine, pattern utilisé par Claude/GPT.

**Contre :**
- Dégradation significative sur modèles 7-14B (hallucinations, oubli, guardrails ignorés)
- Fenêtre de contexte limitée — les instructions longues compressent le contexte utile
- Guardrails contournables par accident (le LLM peut "oublier" une règle après N étapes)
- Aucune valeur ajoutée pour les modèles locaux — avantage concurrentiel perdu

### Option B — Outils MCP spécialisés (rejetée)

**Pour :** Cohérent avec la couche outil existante, pas de nouveau concept.

**Contre :**
- Un outil MCP reste atomique — il ne sait pas dans quel ordre s'appeler
- Le LLM doit toujours "improviser" la séquence et les patterns d'erreur domaine
- Même problème que l'Option A : dépend de l'intelligence du modèle
- Les guardrails (ex. "jamais bash pour .xlsx") ne peuvent pas être encodés dans un outil MCP

### Option retenue — Worker Agents built-in

**Pour :**
- Expertise **model-agnostic** : encodée dans le code, pas dans le contexte
- Guardrails non-contournables : le `SYSTEM_PROMPT` est une constante compilée
- Fail-fast : `packages` → venv installé au `INITIALIZING`, erreur détectée tôt
- Composable : `supports_a2a: True` → invocable par n'importe quel orchestrateur
- Réutilisable : `WorkerAgent` helpers évitent le boilerplate dans chaque agent

**Compromis acceptés :**
- Effort de développement par domaine (pas de génération automatique)
- Bibliothèques Python tier par agent (openpyxl, pandas...) → temps INITIALIZING
- Bibliothèque à maintenir quand les APIs Python évoluent

## Conséquences

**Positives :**
- Les Worker Agents fonctionnent sur tous les modèles, y compris les plus petits (7B)
- Guardrails domaine non-contournables (const Python, pas du contexte)
- `packages` déclaré → dépendances visibles dans le manifest, auditables
- **Fondation A2A posée :** `supports_a2a: True` + skills déclarés dans les manifests → structure nécessaire au routing inter-agents. Le routing lui-même est implémenté dans STORY-392 (Sprint 30). Dans ce sprint, le champ est une déclaration d'intention.
- Pattern documenté → les builders tiers peuvent créer leurs propres Worker Agents

**Négatives / Compromis :**
- Temps d'installation du venv au premier `agent install` (openpyxl : ~5s, pandas : ~20s)
- Chaque Worker Agent est un fichier Python à maintenir
- `packages` dans le manifest = dépendances Python gérées hors de `pyproject.toml`

**Neutres / À surveiller :**
- Temps de préchauffage venv : mesurer sur hardware modeste, envisager préchauffage à l'install plutôt qu'au boot
- Distribution : décider combien de Worker Agents sont bundled avec le runtime par défaut (3-4 estimés)
- Compatibilité multiplateforme des packages pip (openpyxl : OK, pandas : OK, playwright : à vérifier)

## Principes architecturaux impactés

- **Principe #2 — Zéro dépendance externe** : les packages pip sont des dépendances de l'agent, pas du runtime. Le runtime fonctionne sans aucun Worker Agent installé. Conforme.
- **Principe #3 — Contrat minimal** : `WorkerAgent` est une convention de la couche SDK Python. Le contrat AIP reste `manifest() + run() async`. Un agent qui n'hérite pas de `WorkerAgent` reste valide. Conforme.
- **Principe #4 — Fail fast** : `packages` déclarés dans le manifest → `setup_venv()` appelé au `INITIALIZING`. Un package manquant → `ProcessState::Degraded` immédiat, pas une erreur à runtime. Renforcé.
- **Principe #6 — Mémoire à initiative de l'agent** : les Worker Agents appellent `ctx.memory` explicitement. Le runtime n'injecte pas de contexte mémoire automatiquement. Inchangé.

## Liens

- Story associée : STORY-386
- Document d'idéation source : `docs/internal/strategy/capabilities-architecture-ideation.md`
- ADR précédent sur la couche outil : ADR-043 (décomposition atomique des outils fichier)
- ADR sur le bridge Python : ADR-003 (duck typing AIP), ADR-014 (spawn_blocking + asyncio.run)
- Story de suivi A2A : STORY-392 (Sprint 30) — routing inter-agents
- ADR A2A à venir : ADR-049 (à créer en Sprint 30)
