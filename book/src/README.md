# Introduction

Vous avez un agent Python. Il fonctionne sur votre machine, dans un notebook ou dans un script. Vous voudriez le faire tourner en production — de manière fiable, sécurisée, sans que vos données ne quittent votre infrastructure.

C'est exactement ce pour quoi Apollia OS a été construit.

Ce book vous guide de l'installation jusqu'à une solution multi-agents complète en production. Pas de théorie abstraite : chaque concept est introduit au moment où vous en avez besoin, ancré dans un agent que vous construisez vous-même.

---

## Ce qu'est Apollia OS

Apollia OS est un **runtime Rust** pour l'exécution souveraine d'agents IA autonomes. Il joue le rôle de la couche d'exécution qui manque entre votre framework Python et la production :

```
Vos agents Python (LangGraph, CrewAI, custom)
            ↓
      Apollia OS Runtime
   ─────────────────────────
   Isolation   │  Garde-fous
   Outils      │  Audit trail
   Mémoire     │  HITL
   A2A         │  Pipelines
            ↓
     Votre machine — zéro cloud obligatoire
```

Le contrat est minimal : deux fonctions Python, `manifest()` et `run()`, et votre agent devient un citoyen de première classe du runtime.

---

## Pourquoi pas un SaaS ou un framework ?

Les solutions existantes couvrent bien deux cas extrêmes :

- **Les frameworks** (LangGraph, CrewAI, AutoGen) vous aident à écrire la logique agent. Ils ne gèrent ni l'isolation, ni les garde-fous runtime, ni l'audit. Vous construisez vous-même la couche opérationnelle.
- **Les SaaS** (LangServe Cloud, Dify, Modal) gèrent l'infrastructure pour vous — mais vos données transitent par leurs serveurs. Pour une PME européenne ou tout contexte où la souveraineté des données compte, ce n'est souvent pas acceptable.

Apollia OS occupe l'espace entre les deux : il s'exécute **sur votre machine**, en un seul binaire, sans Docker, sans Kubernetes, sans compte cloud obligatoire.

| Critère | Apollia OS | LangServe | Dify | CrewAI |
|---|---|---|---|---|
| Exécution | Local (binaire) | Cloud | Cloud/Self-hosted | Framework only |
| Données | Restent sur la machine | Transitent | Transitent/local | Dépend |
| Isolation outils | Sandbox Linux natif | Non | Non | Non |
| Garde-fous runtime | StepBudget + CircuitBreaker | Non | Basique | Non |
| Framework agnostic | Oui (duck typing) | LangChain only | Propre format | CrewAI only |

**Quand l'utiliser :**
- Vos agents Python tournent déjà ; vous cherchez une couche d'exécution production-ready
- La souveraineté des données est un critère (RGPD, données sensibles, clients PME)
- Vous voulez des garde-fous (budget de steps, isolation, audit) sans les construire vous-même

**Quand ne pas l'utiliser :**
- Vous voulez un SaaS entièrement managé et vous n'avez pas de contrainte de souveraineté
- Vous cherchez un framework pour écrire la logique de raisonnement — Apollia n'en est pas un
- Vous avez besoin d'un cluster multi-nœuds dès maintenant — Apollia est single-node (v1)

---

## À qui s'adresse ce book

Ce book s'adresse à **trois profils** :

### Le développeur Python avec des agents

Vous utilisez déjà LangGraph, CrewAI, ou un agent custom. Vous avez des prototypes qui fonctionnent et vous voulez les passer en production sans réécrire votre logique.

**Ce que vous cherchez :** comment brancher votre agent existant sur Apollia, quels outils natifs vous gagnez, comment gérer la mémoire et les garde-fous.

→ Commencez par le [Chapitre 1](ch01-00-getting-started.md). Sautez au [Chapitre 3](ch03-00-aip-contract.md) pour comprendre le contrat AIP en détail.

### Le développeur qui construit de zéro

Vous démarrez un nouveau projet et vous voulez adopter les bonnes pratiques dès le début. Vous n'avez pas encore d'agent, ou vous avez un agent simple que vous voulez faire évoluer.

**Ce que vous cherchez :** une progression complète, des exemples que vous pouvez copier-coller, une architecture solide à suivre.

→ Lisez ce book de façon linéaire. Les chapitres projets (2, 8, 15) sont conçus pour vous.

### Le tech lead ou l'architecte

Vous évaluez Apollia pour votre équipe ou votre client. Vous avez besoin de comprendre les compromis, la sécurité, et comment intégrer Apollia dans une architecture existante.

**Ce que vous cherchez :** le contrat AIP, les garde-fous, l'A2A, les pipelines, et la vision long-terme.

→ Lisez cette introduction, parcourez le [Chapitre 7](ch07-00-guardrails.md) (garde-fous), le [Chapitre 11](ch11-00-a2a.md) (A2A), et le [Chapitre 16](ch16-00-runtime.md) (le runtime Rust). Les annexes [C](appendix-c-principles.md) et [F](appendix-f-vision.md) résument les principes et la vision.

---

## Comment est organisé ce book

Le book suit le pattern de *The Rust Programming Language* : des chapitres de concepts alternent avec des chapitres projets où vous construisez quelque chose de concret.

**Trois projets fil rouge :**

1. **Chapitre 2 — L'assistant fichier** : votre premier agent. Il lit un fichier, le résume via LLM, et sauvegarde le résumé. Vous verrez le cycle complet manifest → run → outils → mémoire.

2. **Chapitre 8 — Un Worker Agent** : un agent spécialisé que vous concevez, testez, et publiez dans le registre communautaire. Vous apprendrez les patterns de conception, les guardrails de domaine, et les benchmarks.

3. **Chapitre 15 — Une solution PME complète** : une architecture multi-agents de bout en bout avec un Director Agent, des Workers spécialisés, un pipeline et des triggers. C'est l'aboutissement de tout ce que vous aurez appris.

Entre ces projets, les chapitres de concepts approfondissent chaque brique : le contrat AIP, les outils, la mémoire, le LLM, les garde-fous, l'A2A, HITL, les pipelines, les triggers.

---

## Conventions de ce book

Les blocs de code sont exécutables tels quels :

```bash
# Commandes shell — à lancer dans votre terminal
apollia-os run mon-agent "ma tâche"
```

```python
# Code Python — fichier agent complet ou extrait
async def run(ctx, task):
    result = await ctx.llm.chat("Résume : " + task.input)
    return result
```

> **Note** : les encadrés comme celui-ci attirent l'attention sur un point important ou une erreur courante.

Les termes introduits pour la première fois apparaissent en **gras**. Le [Glossaire](appendix-b-glossary.md) en donne la définition formelle.

---

## Pré-requis

- Python 3.10+
- Rust (pour compiler depuis les sources) — ou téléchargez le binaire précompilé
- Un LLM accessible : Ollama en local, ou une clé API (OpenAI, Anthropic, etc.)

Pas de Docker. Pas de Kubernetes. Pas de compte cloud.

---

Commençons.

→ [Chapitre 1 : Mise en route](ch01-00-getting-started.md)
