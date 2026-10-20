---
title: Help center
description: "Step-by-step guides for using Apollia Desktop day to day: install it, connect your tools, set up automations, and keep control of what agents do."
slug: /operator-help
sidebar_position: 0
---

# Apollia Desktop, help center

Step-by-step guides for setting up, automating and keeping control of your AI
day to day.

This corpus covers Apollia Desktop, the desktop application, from the
operator's side: install it,
connect a model, hand tasks to agents, automate work, keep a hand on sensitive
actions, and follow what happens.

## Where to start

- [Install Apollia Desktop](/how-to/install-the-desktop-app), from the download
  to a first agent at work, on macOS, Windows or Linux.
- [Getting acquainted](./screens/first-launch.md), the few steps Apollia
  opens with the first time. The application then opens on the Chat.
- [Set up your profile](./installation/set-up-your-profile.md), the initial flow.
- [Connect a remote model](./installation/connecter-un-modele-distant.md), add the AI server of your organisation: any OpenAI-compatible server.

## The main blocks

- **Get going** : [connect a model](./installation/connecter-un-modele-distant.md), [install an agent](./agents/install-an-agent.md), [hand it a task](./agents/start-an-agent.md), [what became of projects](./projets/what-became-of-projects.md).
- **Talk** : [free conversation](./chat/chat-with-your-ai.md) and [voice dictation](./chat/enable-voice-dictation.md).
- **Automate** : [schedule a trigger](./automatisations/schedule-a-trigger.md), follow the firing history.
- **Keep control** : [approve or refuse an action](./controle/approve-or-reject-an-action.md), [review what each agent may do](./controle/manage-tool-permissions.md), [inspect a tool](./controle/inspect-a-tool.md), [choose an autonomy tier](./agents/choose-an-autonomy-level.md), [measure an agent with eval](./agents/measure-an-agent-with-eval.md).
- **Connect** : link your accounts, folders and tools on the [Connections screen](./integrations/vue-d-ensemble-integrations.md), or [connect an MCP server](./integrations/connecter-un-serveur-mcp.md).
- **Follow** : [activity timeline](./observabilite/read-the-activity-timeline.md), [LLM costs](./observabilite/monitor-ai-costs.md), [activity log](./observabilite/read-the-audit-trail.md).
- **Maintain** : [update Apollia](./installation/mettre-a-jour-apollia.md), [find your version and your data](./transversal/find-your-version-and-data.md).
- **When something jams** : [diagnosing the common cases](./troubleshooting/the-ai-provider-does-not-answer.md), a silent AI, a stuck agent, a refused action, dictation that produces nothing.

## How to read this help center

Every page follows the same shape:

- **Prerequisites**, what you need before you start.
- **Steps**, numbered interface actions with screenshots.
- **Verification**, how to confirm it worked.
- **When it does not work**, the frequent error cases.

When a page mentions a technical concept, the link goes either to the
**Explanation** (how it works) or to the **Reference** (the exhaustive spec). One
jump, to the right place.

## Going further with your agents

- [Choose an autonomy tier](./agents/choose-an-autonomy-level.md), adjust how far an agent goes on its own before asking for your agreement.
- [Measure an agent's performance with eval](./agents/measure-an-agent-with-eval.md), build a reproducible test suite and read the results report.

## A developer rather than an operator?

This help center is about using the application. If you want to **build**
packages, agents with Python tools and workflows, or understand the internal
architecture:

- the [tutorials](/tutorials), learning to build agents step by step.
- the [technical reference](/reference), CLI, HTTP API, SDK contract, configuration.
