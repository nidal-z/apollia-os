---
title: Centre d'aide operateur
description: "Des guides pas à pas pour utiliser Apollia Desktop au quotidien : l'installer, brancher vos outils, créer des automatisations, garder la main sur les agents."
slug: /operator-help
sidebar_position: 0
---

# Apollia Desktop, centre d'aide

Guides pas à pas pour configurer, automatiser et contrôler votre IA au quotidien.

Ce corpus est en français. Il couvre Apollia Desktop, l'application de bureau, côté opérateur :
installer, connecter un modèle, confier des tâches à des agents, automatiser,
garder la main sur les actions sensibles et suivre ce qui se passe.

## Par où commencer

- [Installer Apollia Desktop](/how-to/install-the-desktop-app), du
  téléchargement à un premier agent au travail, sur macOS, Windows ou Linux.
- [Faire connaissance](./screens/first-launch.md), les quelques étapes par
  lesquelles Apollia commence la première fois. L'application s'ouvre ensuite sur le Chat.
- [Configurer votre profil](./installation/set-up-your-profile.md), le parcours initial.
- [Connecter un modèle distant](./installation/connecter-un-modele-distant.md), ajouter le serveur d'IA de votre organisation : tout serveur compatible OpenAI.

## Les grands blocs

- **Démarrer** : [connecter un modèle](./installation/connecter-un-modele-distant.md), [installer un agent](./agents/install-an-agent.md), [lui confier une tâche](./agents/start-an-agent.md), [ce que sont devenus les projets](./projets/what-became-of-projects.md).
- **Discuter** : [conversation libre](./chat/chat-with-your-ai.md) et [dictée vocale](./chat/enable-voice-dictation.md).
- **Automatiser** : [programmer un trigger](./automatisations/schedule-a-trigger.md), suivre l'historique des déclenchements.
- **Garder la main** : [approuver ou refuser une action](./controle/approve-or-reject-an-action.md), [revoir ce que chaque agent peut faire](./controle/manage-tool-permissions.md), [inspecter un outil](./controle/inspect-a-tool.md), [choisir le palier d'autonomie](./agents/choose-an-autonomy-level.md), [mesurer un agent avec eval](./agents/measure-an-agent-with-eval.md).
- **Connecter** : relier vos comptes, dossiers et outils sur l'[écran Connexions](./integrations/vue-d-ensemble-integrations.md), ou [connecter un serveur MCP](./integrations/connecter-un-serveur-mcp.md).
- **Suivre** : [chronologie d'activité](./observabilite/read-the-activity-timeline.md), [coûts LLM](./observabilite/monitor-ai-costs.md), [journal](./observabilite/read-the-audit-trail.md).
- **Maintenir** : [mettre à jour Apollia](./installation/mettre-a-jour-apollia.md), [retrouver sa version et ses données](./transversal/find-your-version-and-data.md).
- **Si ça coince** : [diagnostic des cas fréquents](./troubleshooting/the-ai-provider-does-not-answer.md), IA muette, agent bloqué, action refusée, dictée qui ne transcrit rien.

## Comment lire ce centre d'aide

Chaque page suit le même format :

- **Prérequis**, ce qu'il faut avoir avant de commencer.
- **Étapes**, actions numérotées dans l'interface, avec captures.
- **Vérification**, comment confirmer que ça a marché.
- **Si ça ne marche pas**, les cas d'erreur fréquents.

Quand une page mentionne un concept technique, le lien renvoie soit vers
l'**Explication** (comprendre comment ça fonctionne), soit vers la **Référence**
(spécification exhaustive). Un seul saut, vers le bon endroit.

## Aller plus loin avec vos agents

- [Choisir un palier d'autonomie](./agents/choose-an-autonomy-level.md), ajuster jusqu'où l'agent agit seul avant de demander votre accord.
- [Mesurer les performances d'un agent avec eval](./agents/measure-an-agent-with-eval.md), créer une suite de tests reproductibles et lire le rapport de résultats.

## Vous êtes développeur, pas opérateur ?

Le centre d'aide est conçu pour utiliser l'application. Si vous voulez **créer**
des paquets, des agents avec des outils et des workflows Python, ou comprendre
l'architecture interne :

- les [tutoriels](/tutorials), apprendre à construire des agents pas à pas.
- la [référence technique](/reference), CLI, API HTTP, contrat SDK, configuration.
