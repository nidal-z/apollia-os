# Problème & Solution — Ce qu'Apollia OS résout

> *Définition précise du problème, de l'espace libre sur le marché, et de la solution proposée.*

---

## 1. Le problème — La fracture entre développement et exécution d'agents

### 1.1 L'explosion des agents IA sans infrastructure adaptée

Depuis 2024, l'écosystème des agents IA Python a connu une croissance sans précédent. Des frameworks comme LangGraph, CrewAI, AutoGen, PydanticAI et des dizaines de variantes custom permettent à n'importe quel développeur de construire des agents IA sophistiqués en quelques heures.

Mais construire un agent et **l'exécuter en production de manière fiable** sont deux problèmes radicalement différents.

Chaque développeur qui dépasse le stade du notebook Jupyter se retrouve à devoir résoudre les mêmes problèmes fondamentaux :

**Problème #1 — L'isolation d'exécution**
> Un agent qui exécute du code bash ou Python non maîtrisé est un vecteur de risque. Comment l'isoler sans Docker obligatoire ? Sans Kubernetes ? Sans infrastructre cloud complexe ?

**Problème #2 — La gestion des outils**
> Chaque agent réimplémente son propre catalogue d'outils. File I/O, bash executor, HTTP client, connecteurs MCP — tout est réécrit, souvent mal, dans chaque projet. Aucune standardisation, aucun audit.

**Problème #3 — La mémoire persistante**
> Les agents sans mémoire recommencent à zéro à chaque exécution. Les solutions existantes sont soit des bases vectorielles cloud (dépendance externe, coût, latence), soit des implémentations SQLite artisanales sans structure claire.

**Problème #4 — La résilience**
> Comment gérer un outil qui tombe ? Un LLM saturé ? Un timeout ? Les agents naïfs plantent silencieusement ou boucle indéfiniment. Les circuit breakers, retry policies, et step budgets sont rarissimes dans les implémentations d'agents.

**Problème #5 — La souveraineté**
> De plus en plus d'entreprises, notamment en Europe, refusent que leurs données transitent par des APIs cloud. Mais les solutions "local" existantes sont soit incomplètes, soit trop complexes à déployer.

**Problème #6 — L'interopérabilité**
> MCP (Model Context Protocol), A2A (Agent-to-Agent), ACP (Agent Communication Protocol) — des standards émergent mais leur adoption dans les runtimes est fragmentée. Chaque framework gère son propre écosystème d'outils.

### 1.2 Le coût réel du problème

Ces problèmes ne sont pas théoriques. Ils ont un coût mesurable :

- **Temps de développement gaspillé** : Un développeur qui intègre un agent en entreprise passe en moyenne 40-60% de son temps sur la plomberie d'exécution (sandbox, outils, mémoire, résilience) plutôt que sur la logique métier.
- **Incidents de production** : Les agents sans budget de steps ou circuit breakers génèrent des coûts LLM incontrôlés et des pannes en cascade.
- **Blocages réglementaires** : Les projets IA en entreprise européenne sont bloqués par l'absence de solution locale viable — "on ne peut pas mettre nos données client dans une API américaine."
- **Fragmentation écosystème** : L'impossibilité de réutiliser des outils entre projets force chaque équipe à réinventer la roue, multipliant les bugs et la dette technique.

---

## 2. L'espace libre — Ce qui n'existe pas encore

### 2.1 Cartographie des solutions existantes

| Catégorie | Exemples | Ce qu'ils font | Limitation fondamentale |
|---|---|---|---|
| **Sandboxes cloud** | E2B, Daytona, Modal | Exécution isolée dans le cloud | Cloud-only, dépendance externe, latence, coût |
| **Frameworks d'orchestration** | LangGraph, CrewAI, AutoGen | Orchestration de LLMs et agents | Pas des runtimes d'exécution — pas de sandbox, pas de mémoire standard |
| **Runtimes K8s** | Agent Sandbox (Google) | Isolation par conteneur K8s | Complexité opérationnelle massive, inaccessible hors grande entreprise |
| **Protocoles MCP/A2A** | Anthropic MCP, Google A2A | Standards de communication agent↔outil | Standards uniquement — aucun runtime d'exécution |
| **AgentScope Runtime** | Alibaba (v1.1, fév. 2026) | Runtime multi-framework | Couplé à l'écosystème Alibaba, pas conçu pour déploiement local pur |
| **Solutions tout-en-un** | Dust.tt, LangSmith | Plateforme agents avec monitoring | SaaS-only, pas d'exécution locale, pas framework-agnostic |

### 2.2 La matrice de l'espace libre

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

La combinaison **local-first + framework-agnostic + Tool Registry pluggable + mémoire SQLite ouverte + installable via `cargo install`** n'existe pas. C'est l'espace que Apollia OS occupe.

### 2.3 Ce que les développeurs font aujourd'hui

Sans runtime standard, les développeurs adoptent un de ces contournements :

1. **Le contournement subprocess** : Exécution directe de subprocess Python sans aucune isolation. Ça fonctionne en dev, crée des incidents en prod.

2. **Le contournement Docker DIY** : Chaque projet construit sa propre gestion de conteneurs. Reinventing the wheel, sans les garanties d'un runtime robuste.

3. **L'abandon de la mémoire** : Faute de solution simple, les agents n'ont pas de mémoire persistante. Chaque session repart de zéro.

4. **Le cloud par défaut** : Utilisation d'E2B ou d'APIs cloud pour l'exécution isolée. Coûts récurrents, dépendance externe, problèmes de conformité RGPD.

5. **Le projet bloqué** : Certains projets d'agents en entreprise ne passent jamais en production à cause des contraintes de souveraineté des données. Ils restent des PoC.

---

## 3. La solution — Apollia OS Runtime

### 3.1 La proposition en une phrase

**Apollia OS est un runtime Rust open-source qui permet à n'importe quel agent IA Python de s'exécuter de manière isolée, souveraine, et outillée — avec un `pip install apollia_os` côté agent, et un binaire unique côté infrastructure.**

### 3.2 Ce que le runtime fournit

#### Un contrat d'interface universel : l'AIP

L'**Agent Interface Protocol (AIP)** est le contrat minimal qu'un agent doit implémenter pour fonctionner dans Apollia OS. Il est conçu pour le duck typing Python — pas de classe de base obligatoire, zéro friction pour les agents existants.

```python
# Tout ce qu'un agent doit implémenter
class MonAgent:
    def manifest(self) -> AgentManifest:
        return AgentManifest(
            name="mon-agent",
            version="1.0.0",
            tools_required=["file_io", "python_executor"]
        )

    async def run(self, task: AIPTask, ctx: RuntimeContext) -> AIPResult:
        # L'agent fait son travail — tout le reste est géré par le runtime
        result = await ctx.tools.python_executor.run("print('hello')")
        return AIPResult.completed(result.stdout)
```

Le runtime gère tout le reste : isolation, outils, mémoire, résilience, audit.

#### Un catalogue d'outils prêts à l'emploi

Apollia OS fournit des **outils natifs** immédiatement disponibles pour tout agent :

- `bash_executor` — exécution shell dans un sandbox Linux namespace isolé
- `python_executor` — exécution Python dans un virtualenv dédié par agent
- `file_io` — lecture/écriture dans le répertoire sandbox de l'agent
- `http_client` — requêtes HTTP avec whitelist de domaines configurable
- `mcp_consumer` — connexion à n'importe quel serveur MCP de l'écosystème

Et un système d'**enregistrement d'outils custom** pour les intégrations métier spécifiques :

```bash
$ apollia-os tools register ./tools/mon_erp_connector.py
✔ mon_erp_connector v1.0.0 enregistré
```

#### Une mémoire persistante souveraine

Le Memory Engine fournit 4 types de mémoire persistante via SQLite local :

- **Working** (scratchpad RAM, pas de persistance)
- **Episodic** (événements datés, historique des tâches)
- **Semantic** (connaissances factuelles, préférences)
- **Procedural** (workflows qui ont bien fonctionné)

Recherche FTS5 (unicode61 pour l'accentuation française) en standard. Recherche vectorielle optionnelle via sqlite-vec + modèle GGUF local — activée seulement si le modèle est présent, jamais téléchargé automatiquement.

#### Un moteur d'exécution intelligent : ORIA

L'**ORIA Engine** (Observer-Reasoner-Actor) pilote l'exécution de chaque agent :

- **Mode Direct** : boucle ReAct supervisée pour les tâches simples (≤ 10 steps)
- **Mode Orchestré** : Reasoner LLM + Actor découplés pour les tâches complexes multi-outils
- **StepBudget** : garde-fou tri-dimensionnel (steps, appels outils, temps horloge)
- **ResilienceLayer** : circuit breaker par outil, retry avec backoff exponentiel, classification d'erreurs

#### Une infrastructure de supervision robuste : Runtime Core

Le Runtime Core supervise tous les composants via des acteurs Tokio :

- **Supervisor** : démarrage ordonné, watchdog, restart policy
- **AgentRegistry** : inventaire des agents et de leurs états
- **TaskRouter** : dispatch des tâches vers le bon agent
- **APIServer** : REST local sur Unix socket + localhost
- **EventBus** : découplage interne par broadcast

#### Une CLI complète pour les opérateurs

```bash
apollia-os start                              # Démarrer le runtime
apollia-os agent start ./mon_agent.py         # Déployer un agent
apollia-os run mon-agent "Génère un rapport"  # Lancer une tâche
apollia-os status                             # Vue d'ensemble
apollia-os audit                              # Historique d'exécution
```

### 3.3 Ce que le runtime ne fait pas (par design)

Apollia OS est délibérément **minimal** sur ce qu'il impose :

- **Pas de LLM intégré** : l'agent choisit son LLM (Ollama, Anthropic, OpenAI, tout)
- **Pas de framework imposé** : LangGraph, CrewAI, ou agent custom — le runtime est agnostic
- **Pas de cloud obligatoire** : tout fonctionne en local, hors ligne si nécessaire
- **Pas d'interface graphique** : c'est de l'infrastructure, pas un produit end-user
- **Pas de multi-tenancy** : un runtime, un utilisateur, une machine (la complexité multi-tenant appartient à l'application qui l'utilise)

### 3.4 Le principe de déploiement

```bash
# Installation du runtime (binaire unique, zéro dépendance)
cargo install apollia-os
# ou : téléchargement du binaire depuis GitHub Releases

# Installation du SDK Python dans l'agent
pip install apollia_os

# C'est tout. Pas de Docker requis. Pas de base de données externe.
# Pas de configuration réseau. Un fichier apollia.toml optionnel.
```

Un seul binaire Rust. Une seule dépendance côté agent (`apollia_os` Python). Un fichier de config optionnel. C'est la promesse opérationnelle.

---

## 4. Pour qui — Les cas d'usage cibles

### 4.1 Le développeur d'agents freelance ou en startup

**Contexte** : Construit des agents IA pour des clients. Veut une infrastructure professionnelle sans la gérer lui-même.

**Problème actuel** : Réécrit la plomberie (sandbox, mémoire, outils) à chaque mission. Perd du temps, accumule de la dette technique.

**Avec Apollia OS** : Installe le runtime une fois. Déploie ses agents via AIP. Se concentre sur la logique métier. Peut proposer la mémoire persistante et l'isolation sandbox comme features à valeur ajoutée.

### 4.2 L'entreprise européenne avec contraintes de souveraineté

**Contexte** : Veut des agents IA internes mais les données ne peuvent pas quitter l'infrastructure on-premise. Les APIs cloud sont exclues par la DSI ou la réglementation.

**Problème actuel** : Les solutions d'exécution locale sont soit absentes, soit trop complexes (K8s, Docker Swarm) pour une équipe technique limitée.

**Avec Apollia OS** : Installe le runtime sur un serveur Linux interne. Connecte des agents à des LLMs locaux (Ollama). Les données ne quittent jamais le réseau interne. L'audit trail SQLite donne la traçabilité réglementaire.

### 4.3 Le développeur qui intègre le marketplace d'agents

**Contexte** : Veut proposer ses agents à d'autres utilisateurs d'Apollia OS. L'AIP est le contrat d'interopérabilité.

**Problème actuel** : Aucun standard d'empaquetage et de déploiement d'agents n'existe.

**Avec Apollia OS** : Publie un paquet PyPI qui implémente AIP. N'importe qui avec Apollia OS installé peut utiliser son agent immédiatement.

### 4.4 Le chercheur ou étudiant en agents IA

**Contexte** : Étudie les architectures d'agents, veut un environnement d'expérimentation robuste.

**Problème actuel** : Les notebooks sont fragiles pour des agents avec état persistant. Les environnements cloud sont coûteux.

**Avec Apollia OS** : Infrastructure locale complète pour expérimenter avec des agents persistants, des outils réels, et des patterns de résilience — sans coût cloud.

---

## 5. La validation du problème

Le problème n'est pas hypothétique. Il est documenté dans l'écosystème :

- Les issues GitHub de LangGraph, CrewAI, AutoGen mentionnent régulièrement les problèmes de sandbox, de mémoire persistante, et d'isolation
- Le protocole MCP d'Anthropic (nov. 2025, adopté par Linux Foundation) valide le besoin de standardisation des outils
- Le protocole A2A de Google (v1.0-rc, Linux Foundation) valide le besoin de standards de communication agent-à-agent
- AgentScope Runtime d'Alibaba (v1.1, fév. 2026) valide le besoin de runtimes framework-agnostics — mais leur solution reste couplée à leur écosystème et non conçue pour le déploiement local-first

L'espace reste ouvert pour une solution indépendante, local-first, et genuinement open-source.

---

*Prochaine lecture recommandée : [Ambition Open-Source](./Vision-Ambition-Open-Source)*
