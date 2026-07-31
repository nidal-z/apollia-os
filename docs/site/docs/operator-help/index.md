---
sidebar_position: 0
title: Operator help center
---

# Apollia, help center

Step-by-step guides for setting up, automating and keeping control of your AI
day to day.

This corpus covers the desktop application from the operator's side: install it,
connect a model provider, run agents, automate work, keep a hand on sensitive
actions, and follow what happens.

## Where to start

- [Follow the guided tour](./transversal/suivre-la-visite-guidee.md), six short
  tours launched from the "Getting started" band on the dashboard.
- [Set up your profile](./installation/configurer-votre-profil.md), the initial flow.
- [Connect a remote model](./installation/connecter-un-modele-distant.md), wire a cloud provider or an Ollama server.

## The main blocks

- **Get going** : [connect a model](./installation/connecter-un-modele-distant.md), [install an agent](./agents/installer-un-agent.md), [create a project](./projets/creer-un-projet.md).
- **Talk** : [free conversation and voice dictation](./chat/discuter-avec-votre-ia.md), automatic project context.
- **Automate** : [schedule a trigger](./automatisations/programmer-un-trigger.md), follow the firing history.
- **Keep control** : [approve or refuse an action](./controle/approuver-ou-refuser-une-action.md), [set file permissions](./controle/configurer-les-permissions-de-fichiers.md), [choose an autonomy tier](./agents/choisir-un-palier-d-autonomie.md), [measure an agent with eval](./agents/mesurer-un-agent-avec-eval.md).
- **Connect** : wire your tools through the [MCP catalog](./integrations/connecter-un-serveur-mcp.md).
- **Follow** : [dashboard and timeline](./observabilite/consulter-l-historique-des-taches.md), [LLM costs](./observabilite/surveiller-les-couts-llm.md), [audit trail](./observabilite/consulter-l-audit-trail.md).
- **Maintain** : [update Apollia](./installation/mettre-a-jour-apollia.md).
- **When something jams** : [diagnosing the common cases](./troubleshooting/le-fournisseur-d-ia-ne-repond-pas.md), a silent AI, a stuck agent, a refused action, dictation that produces nothing.

## How to read this help center

Every page follows the same shape :

- **Prerequisites**, what you need before you start.
- **Steps**, numbered interface actions with screenshots.
- **Verification**, how to confirm it worked.
- **When it does not work**, the frequent error cases.

When a page mentions a technical concept, the link goes either to the
**Explanation** (how it works) or to the **Reference** (the exhaustive spec). One
jump, to the right place.

## Going further with your agents

- [Choose an autonomy tier](./agents/choisir-un-palier-d-autonomie.md), adjust how far an agent goes on its own before asking for your agreement.
- [Measure an agent's performance with eval](./agents/mesurer-un-agent-avec-eval.md), build a reproducible test suite and read the results report.

## A developer rather than an operator ?

This help center is about using the application. If you want to **build** Python
agents or understand the internal architecture :

- the [tutorials](/tutorials), learning to build agents step by step.
- the [technical reference](/reference), CLI, HTTP API, SDK contract, configuration.
