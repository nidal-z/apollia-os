# Apollia OS — Wiki du Projet

> Runtime open-source Rust pour l'exécution souveraine d'agents IA autonomes.

---

## Navigation

### Vision & Contexte

| Document | Description |
|---|---|
| [Pivot & Renouveau](./Vision-Pivot-et-Renouveau) | Pourquoi on pivote, les enseignements du projet SaaS, la nouvelle direction |
| [Problème & Solution](./Vision-Probleme-et-Solution) | Le problème ciblé, l'espace libre sur le marché, la solution proposée |
| [Ambition Open-Source](./Vision-Ambition-Open-Source) | Stratégie open-source, proposition de valeur, modèle économique freelance |
| [Positionnement Concurrentiel](./Vision-Positionnement-Concurrentiel) | Analyse des concurrents, espace libre validé, différenciateurs |

### Architecture & Principes

| Document | Description |
|---|---|
| [Principes Architecturaux](./Architecture-Principes) | Les décisions fondamentales qui guident tout le projet |
| [Vue d'ensemble technique & AIP](./Architecture-Vue-Ensemble) | Stack, workspace Rust, interactions entre briques, Agent Interface Protocol |
| [Protocoles & Standards](./Architecture-Protocoles-Standards) | MCP, A2A, ACP — alignement avec l'écosystème |

### Briques Fondamentales

| Document | Description |
|---|---|
| [Tool Registry](./Briques-Tool-Registry) | Catalogue d'outils, sandbox, audit trail |
| [Memory Engine](./Briques-Memory-Engine) | Persistance souveraine multi-types, SQLite, FTS5 |
| [ORIA Engine](./Briques-ORIA-Engine) | Observer-Reasoner-Actor, modes d'exécution, résilience |
| [Runtime Core](./Briques-Runtime-Core) | Supervision, routing, API, EventBus |
| [Apollia CLI](./Briques-CLI) | Interface d'administration et de debug |

### Roadmap

| Document | Description |
|---|---|
| [Roadmap d'Implémentation](./Roadmap-Implementation) | 6 sprints, livrables démo-ables, risques |
| [Décisions Architecturales](./Decisions-Log) | Log de toutes les décisions techniques majeures |

---

## Statut du Projet

**Phase actuelle :** Spécification complète — Implémentation à venir
**Version cible :** v0.1.0 (runtime local fonctionnel)
**Langage principal :** Rust (Tokio, PyO3)
**Bridge agent :** Python (via PyO3 + pyo3-async-runtimes)

---

## En 3 lignes

Apollia OS est un **runtime Rust open-source** qui permet à n'importe quel agent IA Python (CrewAI, LangGraph, custom) de s'exécuter de manière **isolée, souveraine, et outillée** — sans dépendance cloud, sans Docker obligatoire, avec mémoire persistante locale.

C'est l'infrastructure que tout développeur d'agents devrait avoir, et que personne n'a encore construite sous cette forme.

---

*Auteur : Nidal — CTO & Co-fondateur Apollia*
*Dernière mise à jour : mars 2026*
