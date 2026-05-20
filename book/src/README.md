# Introduction

Vous avez un agent Python. Il fonctionne sur votre machine, dans un notebook ou dans un script. Vous voudriez le faire tourner en production — de manière fiable, sécurisée, sans que vos données ne quittent votre infrastructure.

C'est exactement ce pour quoi Apollia OS a été construit.

Ce book vous guide de l'installation à un projet multi-agent complet en production. Pas de théorie abstraite : chaque concept est introduit au moment où vous en avez besoin, ancré dans un agent que vous écrivez vous-même.

---

## Ce qu'est Apollia OS

Apollia OS est un **runtime Rust** pour l'exécution souveraine d'agents IA autonomes. Il joue le rôle de couche d'exécution qui manque entre votre code Python et la production :

```
Vos agents Python (decorator-first, typed, async)
            ↓
      Apollia OS Runtime
   ─────────────────────────
   Isolation   │  Garde-fous
   Outils      │  Audit trail
   Mémoire     │  HITL
   A2A         │  Triggers
            ↓
     Votre machine — zéro cloud obligatoire
```

Le contrat est minimal : vous écrivez une classe Python décorée avec `@agent`, vous annotez vos méthodes avec `@skill` ou `@on_message`, et votre agent devient un citoyen de première classe du runtime.

```python
from apollia import agent, skill, DomainError
from apollia.types import Ctx

@agent(name="hello", version="0.1.0", description="Greets people warmly.")
class Hello:
    @skill("hello.greet", description="Greet a person by name.")
    async def greet(self, name: str, ctx: Ctx) -> dict:
        if not name.strip():
            raise DomainError("EMPTY_NAME", "name must not be empty")
        return {"message": f"Bonjour, {name} !"}
```

C'est tout. Pas de fichier de configuration, pas d'héritage de classe, pas de boilerplate. La signature de la méthode devient un schéma JSON, les exceptions deviennent des résultats typés, et l'agent est immédiatement appelable depuis la CLI, l'API REST, l'app Desktop, ou un autre agent via A2A.

---

## Pourquoi pas un SaaS ou un framework ?

Les solutions existantes couvrent bien deux cas extrêmes :

- **Les frameworks** (LangGraph, CrewAI, AutoGen) vous aident à écrire la logique d'un agent. Ils ne gèrent ni l'isolation, ni les garde-fous runtime, ni l'audit. Vous construisez vous-même la couche opérationnelle.
- **Les SaaS** (LangServe Cloud, Dify, Modal) gèrent l'infrastructure pour vous — mais vos données transitent par leurs serveurs. Pour une PME européenne ou tout contexte où la souveraineté des données compte, ce n'est souvent pas acceptable.

Apollia OS occupe l'espace entre les deux : il s'exécute **sur votre machine**, en un seul binaire, sans Docker, sans Kubernetes, sans compte cloud obligatoire.

| Critère | Apollia OS | LangServe | Dify | CrewAI |
|---|---|---|---|---|
| Exécution | Local (binaire) | Cloud | Cloud / self-hosted | Framework |
| Données | Restent sur la machine | Transitent | Transitent / local | Dépend |
| Isolation outils | Sandbox natif | Non | Non | Non |
| Garde-fous runtime | StepBudget + circuit breakers | Non | Basique | Non |
| Framework-agnostic | Oui (SDK typé indépendant) | LangChain only | Format propre | CrewAI only |

**Quand l'utiliser :**

- Vous voulez livrer des agents IA à des PME et garder leurs données sur leur machine
- La souveraineté des données est un critère (RGPD, données sensibles)
- Vous voulez des garde-fous (budget de steps, isolation, audit, HITL) sans les écrire vous-même

**Quand ne pas l'utiliser :**

- Vous voulez un SaaS entièrement managé et la souveraineté n'est pas une contrainte
- Vous cherchez un framework de raisonnement (chaîne de prompts, graphes) — Apollia n'en est pas un, il s'appuie sur des LLM existants et expose le pattern ReAct comme utilitaire (`apollia.react`)
- Vous avez besoin d'un cluster multi-nœuds dès maintenant — Apollia est single-node sur la v0.1

---

## À qui s'adresse ce book

### Le développeur Python qui livre des agents en production

Vous prototypez avec un notebook ou un script. Vous voulez passer en production sans réécrire votre logique ni assembler vous-même une stack d'observabilité.

→ Démarrez par [l'Installation](part-i-getting-started/01-installation.md), puis les 4 quickstarts (Partie I). Vous aurez un agent fonctionnel en moins d'une heure.

### Le développeur qui construit de zéro

Vous démarrez un nouveau projet et voulez adopter les bonnes pratiques dès le début.

→ Lisez ce book de façon linéaire. Les quickstarts (Partie I) montrent chaque pattern de bout en bout, puis les Parties II–VII couvrent chaque brique en détail.

### Le tech lead ou l'architecte

Vous évaluez Apollia pour votre équipe ou votre client.

→ Lisez cette introduction, parcourez la [Partie III (Ctx Protocol)](part-iii-the-ctx-protocol/10-ctx-overview.md), la [Partie V (gestion d'erreurs)](part-v-error-handling/22-domain-errors.md), et la [Partie VIII (runtime Rust)](part-viii-runtime-rust/29-runtime-overview.md). Les annexes [C (principes)](annexes/C-principles.md) et [F (ADRs)](annexes/F-adr-index.md) résument l'architecture.

---

## Comment est organisé ce book

Le book suit le pattern de *The Rust Programming Language* : des chapitres courts, code-first, qui s'enchaînent.

- **Partie I — Premiers pas** : installation + 4 quickstarts (un par pattern d'agent). Vous écrivez et lancez 4 agents fonctionnels.
- **Partie II — Les décorateurs** : `@agent`, `@skill`, `@on_message`, `@orchestrated`. Le canon.
- **Partie III — Le protocole Ctx** : les 14 services injectés dans chaque agent (`ctx.llm`, `ctx.memory`, `ctx.a2a`, …).
- **Partie IV — Design LLM-friendly** : comment décrire vos skills pour qu'un LLM moyen génère des payloads valides du premier coup.
- **Partie V — Gestion des erreurs** : `DomainError`, `NeedHumanInput`, le boundary.
- **Partie VI — Tests** : `apollia.testing.mock` et les assertions isomorphiques.
- **Partie VII — Outillage** : `apollia inspect`, `apollia new`.
- **Partie VIII — Le runtime Rust** : ce qui se passe sous le capot (acteurs Tokio, API REST, sandbox, triggers).
- **Partie IX — Projet capstone** : un projet multi-agent end-to-end qui consolide tout ce que vous avez appris.

> **Convention book ↔ wiki.** Le book est pédagogique : chaque concept est introduit avec 1-2 exemples concrets. La [référence technique exhaustive](https://github.com/nidal-z/apollia-os/wiki) (specs complètes, tables de paramètres, codes d'erreur) vit dans le wiki. Quand un chapitre couvre 80 % d'un sujet, un lien `> **Référence technique :** [Page-Name](https://github.com/nidal-z/apollia-os/wiki/Page-Name)` vous renvoie au reste.

---

## Conventions

Les blocs de code sont exécutables tels quels :

```bash
# Commandes shell — à lancer dans votre terminal
apollia inspect agents/my-worker/agent.py
```

```python
# Code Python — fichier agent complet ou extrait
from apollia import agent, skill
from apollia.types import Ctx

@agent(name="my-worker", version="0.1.0", description="…")
class MyWorker:
    @skill("my.skill", description="…")
    async def my_skill(self, value: str, ctx: Ctx) -> dict:
        return {"echo": value}
```

> **Note** : les encadrés comme celui-ci attirent l'attention sur un point important, une erreur courante, ou un lien vers la référence wiki.

Les termes introduits pour la première fois apparaissent en **gras**. L'[Annexe B (Glossaire)](annexes/B-glossary.md) en donne la définition formelle.

---

## Pré-requis

- Python 3.10+
- Rust (pour compiler depuis les sources) — ou téléchargez le binaire précompilé
- Un LLM accessible : llama.cpp en local (bundled), Ollama, ou une clé API (Anthropic, OpenAI, …)

Pas de Docker. Pas de Kubernetes. Pas de compte cloud.

---

Commençons.

→ [Installation](part-i-getting-started/01-installation.md)
