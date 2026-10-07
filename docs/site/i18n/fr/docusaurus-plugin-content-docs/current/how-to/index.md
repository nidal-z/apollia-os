---
sidebar_position: 0
title: Guides pratiques
description: "Des recettes par tâche : installer et lancer le runtime, écrire des outils et des workflows, les tester et les empaqueter, embarquer Apollia, garder l'humain dans la boucle."
---

# Guides pratiques

Des recettes orientées tâche. Chacune résout un problème unique et concret.

## Installer et exécuter

- [Installer l'application de bureau](/how-to/install-the-desktop-app) :
  téléchargez un installeur prêt à l'emploi et lancez l'application, sans
  build à réaliser.
- [Installer et exécuter le runtime](/how-to/install-and-run) : compilez
  depuis un checkout et obtenez un daemon (ainsi que l'application de bureau
  en mode développement) qui exécute un agent.
- [Tirer le meilleur parti de l'inférence locale](/how-to/accelerate-local-inference) :
  comment le moteur `llama-server` embarqué sert des modèles GGUF locaux, et
  comment l'alimenter.

## Construire des paquets

- [Écrire un outil](/how-to/write-a-worker) : une fonction Python qu'un agent
  appelle, déclarée avec `@tool` et accordée dans le manifeste du paquet.
- [Écrire un workflow](/how-to/write-a-director) : des étapes que vous écrivez
  en Python, chacune confiant un travail à un agent, à un autre workflow ou à
  l'utilisateur.
- [Faire planifier un agent d'abord](/how-to/run-an-orchestrated-agent) :
  faites écrire à un agent les étapes d'une tâche avant qu'il n'agisse.
- [Tester vos outils et vos workflows](/how-to/test-your-agents) : testez-les
  unitairement face à un contexte simulé, puis évaluez des tâches complètes
  face à un runtime réel.
- [Empaqueter et distribuer](/how-to/package-and-distribute-an-agent) :
  construisez, signez, installez et partagez une archive `.apo`.

## Intégrer et exploiter

- [Intégrer via le contrat de pilotage](/how-to/integrate-via-driving-contract) :
  pilotez le runtime depuis une application hôte via l'API HTTP.
- [Intégrer par fédération](/how-to/embed-via-federation) : exposez votre
  produit sous forme d'outils MCP et laissez un agent écrire en retour via
  votre API REST.
- [Configurer un client OAuth Google](/how-to/set-up-a-google-oauth-client) :
  enregistrez votre propre client dans la console Google Cloud, étape par
  étape, pour faire fonctionner le connecteur Gmail, Calendar et Drive.
  Microsoft 365 ne nécessite pas d'équivalent.
- [Garder un humain dans la boucle](/how-to/human-in-the-loop) : arrêtez un
  workflow sur une question, et sachez ce qui demande une approbation de
  lui-même.
- [Auditer et vérifier une exécution](/how-to/audit-and-verify) : lisez la
  piste d'audit et vérifiez le journal.
- [Déployer en production](/how-to/deploy-in-production) : exécutez Apollia
  comme un service managé avec TCP, authentification et TLS.
