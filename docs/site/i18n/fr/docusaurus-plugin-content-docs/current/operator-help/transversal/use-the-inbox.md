---
title: Utiliser la liste À valider
slug: /operator-help/transversal/use-the-inbox
sidebar_position: 2
---

# Utiliser la liste À valider

> Pour les operators qui veulent répondre au même endroit à tout ce que leurs agents attendent : demandes d'accord, questions, tâches en pause et éléments de lot.

## Prérequis

- Au moins un agent, une tâche ou un lot a demandé quelque chose.

## Où la trouver

Dans la barre latérale, l'entrée **À valider**. Le badge à côté du libellé compte ce qui attend votre réponse. Le même nombre figure en tête de l'écran : *« 3 éléments attendent votre réponse. »*

## Comment l'écran est organisé

**À valider** est une seule liste, sans onglets. Elle réunit les accords demandés par les agents dans les conversations, les questions des agents, les tâches qui se sont mises en pause pour vous attendre et les éléments de lot qui attendent un accès, du plus récent au plus ancien. Répondre ici reprend la conversation, la tâche ou le lot concerné.

Chaque élément porte, au-dessus de sa carte :

- **D'où il vient** : *Depuis la conversation « … »*, *Depuis une tâche, en pause* ou *Depuis un lot, élément 4*.
- **Un lien pour y retourner** : **Ouvrir la conversation**, **Ouvrir l'agent** ou **Ouvrir le lot**.
- **Quand il a été demandé**.

Quand rien n'attend, l'écran le dit : *Rien n'attend votre réponse*.

## Filtrer la liste

Quatre pastilles au-dessus de la liste, chacune avec son compteur :

- **Tout**
- **Décisions** : les demandes d'accord, pauses de tâches comprises.
- **Questions** : ce à quoi on répond avec des mots.
- **Éléments de lot** : les éléments d'un lot qui attendent un accès.

## Répondre à une demande d'accord

La carte est celle du chat, avec les mêmes réponses : **Une fois**, **Toujours pour cet agent** et **Refuser**, avec une raison facultative. Une tâche en pause n'offre que **Une fois** et **Refuser**. Voir [Approuver ou refuser une action d'agent](../controle/approve-or-reject-an-action.md) pour ce que couvre chaque réponse.

Une longue commande est repliée : la liste montre ses premières lignes, et les réponses apparaissent une fois que vous avez cliqué sur **Afficher la commande entière**. Vous répondez sur ce que vous avez lu en entier.

Quand vous répondez **Toujours pour cet agent** à un élément de lot, les autres éléments de cet agent qui attendent le même type d'accès repartent, et une ligne dit combien ont été repris.

## Répondre à une question

Quand un agent vous demande quelque chose, l'élément montre une carte intitulée *« … vous pose une question »*, avec le badge **Question**. Dessous, quand l'agent en a fourni une, une phrase donne le contexte.

Chaque question est présentée selon ce que l'agent attend :

- **Une question ouverte** : un champ de texte.
- **Un choix parmi des réponses proposées par l'agent** : une réponse, ou plusieurs quand l'agent le permet. Quand l'agent l'accepte, un champ **Autre réponse…** figure à côté des choix.
- **Une question par oui ou non** : **Oui** et **Non**.

Ensuite :

- **Répondre** envoie ce que vous avez rempli. Le bouton devient actif dès qu'une question porte une réponse.
- **Refuser de répondre** refuse, avec une raison facultative transmise à l'agent.

## Revenir sur une réponse

Une fois votre réponse donnée, l'élément se réduit à une ligne qui dit ce que vous avez répondu, avec la mention *Réponse enregistrée. Cette ligne se retire dans quelques secondes.* Pendant ces quelques secondes, **Annuler** reprend la réponse et l'élément attend de nouveau. Ensuite la réponse est envoyée et la ligne quitte la liste.

## Les décisions déjà prises

Il n'y a pas d'historique ici : une décision prise se lit dans le **Journal**, depuis la barre latérale. Voir [Consulter la piste d'audit](../observabilite/read-the-audit-trail.md).

Les notifications envoyées par Apollia ne sont pas listées ici non plus. Les dernières figurent dans **Réglages**, section **Notifications**, sous **Derniers envois**. Voir [Configurer un canal de notification](../notifications/set-up-a-channel.md).

## Vérification

- La liste se vide une fois que vous avez répondu à tout, et le badge de la barre latérale retombe à zéro.
- Une nouvelle demande fait monter le badge sans rafraîchissement manuel.
- Un élément auquel vous avez répondu dans le chat quitte la liste.

## Si ça ne marche pas

- **Le badge de la barre latérale reste à zéro alors qu'un agent attend** : vérifiez le **point d'état** à côté du mot *Apollia* dans la barre du haut. S'il est rouge, le moteur est déconnecté ; quittez et rouvrez l'application.
- **Les réponses d'une demande d'exécution de code ne s'affichent pas** : la commande est repliée. Cliquez sur **Afficher la commande entière**.
- **Le bouton Répondre reste grisé** : aucune question ne porte encore de réponse.
- **Vous voulez éviter de répondre sans cesse à la même demande** : répondez **Toujours pour cet agent**. Voir [Approuver ou refuser une action d'agent](../controle/approve-or-reject-an-action.md).

## Demandes provenant d'un serveur MCP

**Rien n'arrive ici depuis un serveur MCP.** La spécification définit
deux façons pour un serveur de rappeler son client, une demande de saisie
structurée (`elicitation/create`) et une demande d'appel LLM
(`sampling/createMessage`), et Apollia n'implémente ni l'une ni l'autre. Il ne
dit pas aux serveurs qu'il le fait : un serveur qui les prend en charge ne s'en
servira simplement pas avec Apollia.

Ce qui arrive dans cette liste vient des agents, des tâches et des lots :
demandes d'accord, questions et pauses.

> **Concept :** [Explication Apollia](/explanation)
