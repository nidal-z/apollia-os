---
sidebar_position: 0
title: Centre d'aide operateur
---

# Apollia, centre d'aide

Guides pas-a-pas pour configurer, automatiser et controler votre IA au quotidien.

Ce corpus est en francais. Il couvre l'application desktop cote operateur :
installer, connecter un fournisseur d'IA, lancer des agents, automatiser,
garder la main sur les actions sensibles et suivre ce qui se passe.

## Par ou commencer

- [Suivre la visite guidee](./transversal/suivre-la-visite-guidee.md), six visites
  courtes lancees depuis la bande « Prise en main » de l'ecran Accueil.
- [Configurer votre profil](./installation/configurer-votre-profil.md), le parcours initial.
- [Connecter un modele distant](./installation/connecter-un-modele-distant.md), brancher un fournisseur cloud ou un serveur Ollama.

## Les grands blocs

- **Demarrer** : [connecter un modele](./installation/connecter-un-modele-distant.md), [installer un agent](./agents/installer-un-agent.md), [creer un projet](./projets/creer-un-projet.md).
- **Discuter** : [conversation libre et dictee vocale](./chat/discuter-avec-votre-ia.md), contexte projet automatique.
- **Automatiser** : [programmer un trigger](./automatisations/programmer-un-trigger.md), suivre l'historique des declenchements.
- **Garder la main** : [approuver ou refuser une action](./controle/approuver-ou-refuser-une-action.md), [configurer les permissions de fichiers](./controle/configurer-les-permissions-de-fichiers.md), [inspecter un outil](./controle/inspecter-un-outil.md), [choisir le palier d'autonomie](./agents/choisir-un-palier-d-autonomie.md), [mesurer un agent avec eval](./agents/mesurer-un-agent-avec-eval.md).
- **Connecter** : brancher vos outils via le [catalogue MCP](./integrations/connecter-un-serveur-mcp.md).
- **Suivre** : [Accueil et chronologie](./observabilite/consulter-l-historique-des-taches.md), [couts LLM](./observabilite/surveiller-les-couts-llm.md), [audit trail](./observabilite/consulter-l-audit-trail.md).
- **Maintenir** : [mettre a jour Apollia](./installation/mettre-a-jour-apollia.md), [retrouver sa version et ses donnees](./transversal/trouver-sa-version-et-ses-donnees.md).
- **Si ca coince** : [diagnostic des cas frequents](./troubleshooting/le-fournisseur-d-ia-ne-repond-pas.md), IA muette, agent bloque, action refusee, dictee KO.

## Comment lire ce centre d'aide

Chaque page suit le meme format :

- **Prerequis**, ce qu'il faut avoir avant de commencer.
- **Etapes**, actions UI numerotees avec captures.
- **Verification**, comment confirmer que ca a marche.
- **Si ca ne marche pas**, les cas d'erreur frequents.

Quand une page mentionne un concept technique, le lien renvoie soit vers
l'**Explication** (comprendre comment ca fonctionne), soit vers la **Reference**
(spec exhaustive). Un seul saut, vers le bon endroit.

## Aller plus loin avec vos agents

- [Choisir un palier d'autonomie](./agents/choisir-un-palier-d-autonomie.md), ajuster jusqu'ou l'agent agit seul avant de demander votre accord.
- [Mesurer les performances d'un agent avec eval](./agents/mesurer-un-agent-avec-eval.md), creer une suite de tests reproductibles et lire le rapport de resultats.

## Vous etes developpeur, pas operateur ?

Le centre d'aide est concu pour utiliser l'application. Si vous voulez **creer**
des agents Python ou comprendre l'architecture interne :

- les [tutoriels](/tutorials), apprendre a construire des agents pas a pas.
- la [reference technique](/reference), CLI, API HTTP, contrat SDK, configuration.
