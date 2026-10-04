---
title: Ce que sont devenus les projets
slug: /operator-help/projects/what-became-of-projects
sidebar_position: 1
---

# Ce que sont devenus les projets

> Pour les operators qui utilisaient l'écran Projets avant la mise à jour, et veulent savoir où sont passés leurs documents, leurs dossiers et leurs conversations.

L'écran Projets n'existe plus. Ce qu'un projet rassemblait vit désormais là où
tout agent lit : les sources, les connexions, et les conversations de l'agent
Apollia. Plus rien n'est injecté dans une conversation : un agent cherche dans
ses sources quand il en a besoin.

## Ce que fait la mise à jour

À son premier démarrage après la mise à jour, Apollia reprend chaque projet une
fois :

| Dans un projet | Désormais |
|---|---|
| Ses documents | Des sources de l'agent Apollia, et des agents que le projet nommait, rangées sous le nom du projet. |
| Ses instructions | Une source par projet, cherchable, jamais ajoutée d'office à une conversation. |
| Son dossier de travail | Une connexion dossier, accordée à aucun agent tant que vous ne l'accordez pas. |
| Ses conversations | Des conversations de leur agent ; une conversation libre revient à l'agent Apollia. |
| Sa mémoire | Fusionnée dans la mémoire du même nom. |

## L'avis

Une fois la migration faite, un avis dit ce que sont devenus vos projets et ce
qui n'a pas pu être repris, par exemple un document qui n'était plus là où le
projet le désignait. Il ne s'affiche qu'une fois. Chaque ligne nomme l'élément,
son projet et la raison.

Quand la migration n'a pas pu aller au bout, l'avis le dit et Apollia réessaie
au prochain démarrage. Vos anciens projets restent intacts tant qu'un passage
n'est pas complet : la base des projets n'est supprimée qu'une fois repris tout
ce qui pouvait l'être.

## Où les retrouver

1. **Les documents et les instructions** : **Agents**, puis l'agent Apollia,
   puis ses sources. Ils sont groupés par projet.
2. **Les dossiers de travail** : **Connexions**. Accordez un dossier aux agents
   qui doivent le lire.
3. **Les conversations** : **Chat**, avec l'agent Apollia ou avec leur agent.

## Vérification

L'avis ne s'affiche plus au démarrage, et les documents d'un ancien projet
figurent parmi les sources de l'agent Apollia.
