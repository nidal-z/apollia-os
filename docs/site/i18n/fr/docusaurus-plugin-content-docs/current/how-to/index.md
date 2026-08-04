---
sidebar_position: 0
title: Guides pratiques
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

## Construire des agents

- [Écrire un director](/how-to/write-a-director) : un agent qui raisonne et
  pilote des workers.
- [Écrire un worker](/how-to/write-a-worker) : un agent à skill unique
  exposé via A2A.
- [Exécuter un agent orchestré](/how-to/run-an-orchestrated-agent) : laissez
  le moteur planifier, encadrer et vérifier une exécution multi-étapes.
- [Tester vos agents](/how-to/test-your-agents) : testez unitairement les
  skills face à un contexte simulé, puis testez leur intégration face à un
  runtime réel.
- [Packager et distribuer un agent](/how-to/package-and-distribute-an-agent) :
  empaquetez, installez et partagez un agent.

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
- [Garder un humain dans la boucle](/how-to/human-in-the-loop) : exigez une
  approbation avant l'exécution d'une action à conséquences.
- [Auditer et vérifier une exécution](/how-to/audit-and-verify) : lisez la
  piste d'audit, vérifiez le journal, et annulez les modifications apportées
  au système de fichiers.
- [Déployer en production](/how-to/deploy-in-production) : exécutez Apollia
  comme un service managé avec TCP, authentification et TLS.
