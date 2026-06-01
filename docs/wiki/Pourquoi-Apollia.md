# Pourquoi Apollia OS ?

> Quand utiliser Apollia OS, quand ne pas l'utiliser, et comment il se positionne par rapport aux alternatives.

---

## Le problème

Vous voulez exécuter des agents IA en production. Pas un prototype Jupyter, pas un chatbot Streamlit - un agent qui tourne 24/7, appelle des outils, manipule des fichiers, et interagit avec vos systèmes métier.

Les options actuelles :

1. **Cloud SaaS** (LangServe, Dify, Modal) - vos données transitent par des serveurs tiers. Pour une PME européenne avec des données clients, ce n'est souvent pas acceptable.
2. **Self-hosted ML platforms** (MLflow, BentoML) - conçus pour le serving de modèles, pas pour des agents autonomes avec des boucles de raisonnement et des outils.
3. **Frameworks agents** (LangGraph, CrewAI, AutoGen) - excellents pour définir la logique agent, mais ils ne gèrent ni l'isolation, ni l'audit, ni les garde-fous runtime. C'est à vous de construire la couche opérationnelle.

## Ce qu'apporte Apollia OS

Apollia OS est le **runtime** qui manque entre votre agent Python et la production :

- **Local-first** : le binaire tourne sur votre machine. Zéro données vers le cloud (sauf si votre agent appelle un LLM externe, ce qui est votre choix).
- **Isolation** : chaque outil s'exécute dans un sandbox Linux (namespaces), sans Docker.
- **Garde-fous** : StepBudget, circuit breakers, audit trail - appliqués par le runtime Rust, non contournables par l'agent Python.
- **Agnostique du framework** : votre agent LangGraph, CrewAI, ou custom fonctionne tel quel. Deux fonctions suffisent : `manifest()` et `run()`.
- **Outillé** : CLI, API REST, application desktop, triggers, notifications, mémoire persistante - tout inclus.

## Quand utiliser Apollia OS

- Vous avez des agents Python et vous voulez les exécuter en production de manière fiable
- La souveraineté des données est un critère (RGPD, clients PME, données sensibles)
- Vous voulez des garde-fous runtime (budget, isolation, audit) sans les construire vous-même
- Vous utilisez déjà LangGraph, CrewAI, ou un framework custom et vous cherchez une couche d'exécution

## Quand NE PAS utiliser Apollia OS

- **Vous voulez un SaaS clé-en-main** : Apollia OS est un binaire à déployer. Si vous ne voulez pas gérer l'infrastructure, regardez Dify, LangServe Cloud, ou des plateformes managées.
- **Vous n'avez pas de contrainte de souveraineté** : si vos données peuvent aller dans le cloud et que vous voulez le chemin le plus court, un SaaS est plus rapide.
- **Vous avez besoin d'un cluster multi-nœuds** : Apollia OS est single-node. Pour du scaling horizontal, il faudra gérer la distribution vous-même (ou attendre la roadmap).
- **Vous cherchez un framework agent** : Apollia OS n'est pas un framework - il ne vous aide pas à écrire la logique de raisonnement. Il exécute la logique que vous avez écrite avec le framework de votre choix.

## Comparaison avec les alternatives

| Critère | Apollia OS | LangServe | Dify | CrewAI | Modal |
|---|---|---|---|---|---|
| **Exécution** | Local (binaire) | Cloud | Cloud/Self-hosted | Framework only | Cloud |
| **Données** | Restent sur la machine | Transitent | Transitent / local | Dépend du déploiement | Transitent |
| **Isolation outils** | Sandbox Linux natif | Non | Non | Non | Container |
| **Garde-fous runtime** | StepBudget + CircuitBreaker | Non | Basique | Non | Non |
| **Audit trail** | SQLite intégré | Non | Basique | Non | Logs cloud |
| **Framework agnostic** | Oui (duck typing) | LangChain only | Propre format | CrewAI only | Agnostic |
| **LLM embarqué** | Oui (whisper, llama.cpp) | Non | Non | Non | Non |
| **Desktop app** | Oui (Tauri) | Non | Oui | Non | Non |

---

## En résumé

Apollia OS n'essaie pas de remplacer votre framework agent. Il fournit le terrain de jeu sécurisé, audité et outillé sur lequel vos agents s'exécutent. Si vous avez besoin de souveraineté des données et de garde-fous en production, c'est pour ça qu'il existe.
