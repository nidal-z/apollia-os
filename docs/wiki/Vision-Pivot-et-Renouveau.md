# Pivot & Renouveau — Pourquoi Apollia OS Change de Direction

> *Ce document raconte honnêtement l'histoire du projet : ce qu'on a construit, ce qu'on a appris, et pourquoi le nouveau chemin est plus solide que l'ancien.*

---

## 1. Ce qu'était Apollia OS (version SaaS)

De septembre 2025 à début 2026, Apollia OS était un **workspace IA SaaS** — une application web complète ciblant les travailleurs du savoir dans les PME et ETI françaises.

### La vision initiale

L'idée de départ était solide : les professionnels passent plus de temps à orchestrer des outils qu'à penser. Chaque tâche de connaissance — comparer des rapports, synthétiser des données, produire des livrables — mobilise 3 à 5 outils différents, et le contexte se perd à chaque transition. L'humain joue le rôle de routeur manuel.

L'IA générative existait, mais restait passive : elle répondait à des prompts dans des sessions éphémères. Elle ne pouvait pas agir dans l'environnement réel de l'utilisateur.

La solution proposée : **ORIA**, un agent IA autonome avec un vrai runtime d'exécution — un sandbox Docker isolé avec des primitives composables (bash, Python, lecture/écriture fichiers) — opérant dans une interface web unifiée avec base de connaissances persistante et mémoire utilisateur.

### Ce qui a été construit

En 8 mois de développement intensif (soir et week-end en parallèle d'une activité salariée), le projet a produit :

- **8 epics complètes** avec plus de 1 500 tests passants
- Une **stack technique sophistiquée** : FastAPI + PostgreSQL + DragonflyDB + Qdrant + MinIO + Keycloak + LiteLLM + vLLM (Qwen3 30B)
- Un **pipeline de traitement documentaire** : 9 formats parsés, embeddings BGE-M3 locaux, recherche hybride sémantique + BM25
- Un **pipeline agent complet** : HITL (Human-in-the-Loop), plans multi-étapes, workspace documentaire avec édition en temps réel
- Un **système de mémoire utilisateur** GDPR-natif avec gestion des préférences
- L'**intégralité du socle d'identité** : Keycloak OIDC, MFA TOTP, multi-tenancy, RBAC, budgets de tokens
- Un **système de notifications WebSocket** temps réel complet
- Des **spécifications UX complètes** (9 epics UX additionnelles)

Ce n'était pas un prototype. C'était un produit à qualité de production, avec des patterns DDD rigoureux, une traçabilité complète des requirements, et une documentation technique exhaustive.

---

## 2. Les enseignements — ce que le SaaS a révélé

### 2.1 Le problème de la complexité opérationnelle

La stack technique choisie était puissante. Elle était aussi **opérationnellement lourde** pour une cible PME :

- PostgreSQL + DragonflyDB + Qdrant + MinIO + Keycloak + Traefik : **6 services d'infrastructure** à déployer, configurer, maintenir
- Un DSI de PME ne peut pas (et ne veut pas) gérer cette complexité
- La promesse de "souveraineté des données" s'érodait face à la réalité d'un déploiement cloud nécessaire pour simplifier l'opération
- La valeur perçue par la PME n'était pas dans l'infrastructure — elle était dans l'agent qui agissait sur ses documents

Le produit avait résolu le problème de l'utilisateur final. Il n'avait pas encore résolu le problème de l'acheteur IT.

### 2.2 Le problème du positionnement marché

En 8 mois, le marché des workspaces IA a évolué rapidement :

- **Notion AI, Microsoft Copilot, Google Gemini Workspace** ont accéléré leur intégration dans les outils existants
- Les PME adoptent ces solutions par défaut (friction zéro, budget logiciel déjà engagé)
- Concurrencer ces acteurs en frontal nécessiterait des ressources de distribution hors de portée d'une startup à 2 personnes
- Le marché cible (PME 10-500 employés France) est plus difficile à penetrer qu'anticipé : cycles de vente longs, frilosité au changement d'outil

### 2.3 Le problème de la fenêtre d'opportunité

Parallèlement à ces défis, quelque chose d'inattendu est apparu :

- L'**écosystème des agents IA Python** (CrewAI, LangGraph, AutoGen, custom) explosait
- Chaque développeur d'agents se heurtait aux mêmes problèmes : comment exécuter un agent de manière isolée ? Comment lui donner des outils sans coder une sandbox from scratch ? Comment lui donner de la mémoire persistante sans base vectorielle cloud ?
- **Aucun runtime universel open-source n'existait** pour répondre à ces besoins
- La stack technique que nous avions construite était une réponse partielle à ces problèmes — mais enfermée dans un produit SaaS fermé

### 2.4 Le vrai actif

En faisant le bilan, l'actif le plus précieux du projet n'était pas l'application web. C'était :

1. **La compréhension profonde des problèmes d'exécution d'agents** — sandbox, isolation, outils, mémoire, résilience
2. **L'architecture ORIA** (Observer-Reasoner-Actor) validée en conditions réelles
3. **Les patterns de gestion de mémoire** adaptés aux PME (pas de surfonctionnalité)
4. **La sensibilité à la souveraineté** — comprendre ce que "local-first" signifie vraiment opérationnellement

Ces actifs valaient beaucoup plus en open-source qu'enfermés dans une application SaaS.

---

## 3. La décision de pivot

### Ce qu'on abandonne (et ce qu'on ne perd pas)

On **abandonne** :
- L'application SaaS web complète (frontend SvelteKit, backend FastAPI)
- Le modèle de revenus SaaS par abonnement
- La cible "travailleur du savoir PME" comme premier client

On **conserve** :
- L'architecture ORIA et tous ses patterns
- La philosophie "local-first, souveraineté totale"
- Le nom Apollia et la vision long-terme
- Tous les enseignements techniques sur les agents IA

Ce n'est pas un abandon — c'est une **distillation**. On extrait le noyau technique le plus précieux du projet SaaS et on en fait le fondement d'un projet open-source.

### Ce qu'on construit maintenant

**Apollia OS v2 : un runtime Rust open-source** pour l'exécution souveraine d'agents IA autonomes.

Au lieu de concurrencer Notion et Microsoft, on fournit l'infrastructure manquante aux **développeurs d'agents IA** — un marché en croissance explosive, avec des besoins précis non satisfaits, et une communauté qui adopte rapidement les bons outils.

---

## 4. Pourquoi ce pivot est stratégiquement solide

### 4.1 Le marché est plus accessible

| Dimension | SaaS PME | Runtime Open-Source |
|---|---|---|
| Cycle de décision | 3-12 mois (DSI, budget, PoC) | Jours (développeur + `cargo install`) |
| Critère d'adoption | ROI démontrable, support, migration | Ça marche ? C'est open-source ? |
| Résistance | Forte (changement d'habitude, risque perçu) | Faible (essayer ne coûte rien) |
| Distribution | Commerciale (force de vente, partenaires) | Communautaire (GitHub, HN, Reddit) |
| Feedback loop | Lent (churn mensuel) | Rapide (issues, PRs, stars) |

### 4.2 La compétition est moins féroce

Dans l'espace "runtime universel pour agents locaux" :

- **Pas d'acteur dominant** avec une adoption massive
- Les solutions existantes sont soit trop couplées à un cloud (E2B, Daytona), soit trop complexes (K8s-based sandboxes), soit spécifiques à un framework (pas framework-agnostic)
- La combinaison **local-first + framework-agnostic + Tool Registry pluggable + mémoire SQLite + Rust** n'existe pas

### 4.3 Le modèle économique est cohérent avec le profil fondateur

Un runtime open-source génère de la valeur pour Nidal de plusieurs façons sans nécessiter une équipe de vente :

- **Crédibilité technique** immédiate (GitHub stars, contributions, citations)
- **Flux de leads entrants** pour des prestations d'intégration et de création d'agents custom
- **Positionnement d'expert** dans l'écosystème agents IA français/européen
- **Option de monétisation enterprise** plus tard (support, déploiements managés, agents certifiés)

### 4.4 La trajectoire temporelle est réaliste

Le SaaS complet nécessitait de gérer simultanément : produit, infrastructure, acquisition clients, support, ventes. Trop de fronts pour 2 personnes.

Le runtime open-source peut être **fonctionnel et démontrable en 20 semaines** de développement soir/week-end. Le premier commit qui fait tourner un agent Python dans un sandbox Rust est le premier livrable de valeur — pas besoin d'attendre une V1 complète pour commencer à créer de la traction.

---

## 5. Ce que le projet SaaS a transmis à Apollia OS

Le projet SaaS n'a pas été un échec — il a été une **phase de R&D intensive**. Voici ce qu'il a directement légué au nouveau projet :

| Acquis SaaS | Application dans Apollia OS Runtime |
|---|---|
| Architecture ORIA (Observer-Reasoner-Actor) | Moteur central ORIA Engine avec modes Direct/Orchestré |
| Patterns sandbox Docker | Sandbox Linux namespaces (sans Docker) — plus léger, zéro dépendance |
| Pipeline mémoire utilisateur | Memory Engine SQLite avec 4 types (Working/Episodic/Semantic/Procedural) |
| HITL (Human-in-the-Loop) | `input_required` dans le TaskContract AIP |
| Tool Registry interne | Tool Registry avec ToolDescriptor + SandboxProfile formalisés |
| Audit trail complet | Audit trail SQLite natif dans Tool Registry |
| Résilience circuit breaker | ResilienceLayer avec circuit breaker par outil |
| Philosophy "local-first souverain" | Principe #1 de l'architecture : zéro dépendance cloud |

Tout ce qui avait de la valeur a été préservé. Ce qui était lourd (multi-tenancy, auth, frontend, billing) a été retiré. Ce qui reste est le noyau pur.

---

*Prochaine lecture recommandée : [Problème & Solution](./Vision-Probleme-et-Solution)*
