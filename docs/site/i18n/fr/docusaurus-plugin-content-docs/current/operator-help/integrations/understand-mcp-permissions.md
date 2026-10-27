---
title: Comprendre les permissions MCP
slug: /operator-help/integrations/understand-mcp-permissions
sidebar_position: 7
---

# Comprendre les permissions MCP

> Pour tout operator qui veut savoir pourquoi un outil demande une approbation, comment retirer un accord, et ce que la politique de votre organisation interdit.

## Prérequis

- Au moins un connecteur ou un serveur MCP connecté.
- Vous connaissez la différence entre connecteur natif et serveur MCP (voir [Vue d'ensemble des intégrations](vue-d-ensemble-integrations.md)).

## Pourquoi cet outil demande une approbation

Chaque outil exposé par un connecteur ou un MCP appartient à une **famille d'action**, et c'est la famille qui porte la règle :

| Famille | Règle | Exemples |
|---|---|---|
| **Lire** | **Sans demander** : l'outil s'exécute immédiatement. | Un outil d'un serveur sans compte, que le serveur déclare en lecture seule. |
| **Lire des données personnelles**, **Écrire ou supprimer**, **Envoyer vers l'extérieur** | **Votre accord est demandé** : une carte apparaît, vous décidez. | Lecture d'un compte connecté, `gcal.list_events`, `outlook.search`. Écritures et suppressions, `gcal.create_event`, `gcal.delete_event`. Envois, `gmail.send`, `outlook.send`. |
| **Exécuter du code** | **Accord à chaque fois**. | Une commande ou un script que l'agent veut lancer. |

La carte porte trois réponses, **Une fois**, **Toujours pour cet agent** et **Refuser**. Il n'y a ni portée à choisir ni mot à taper. **Toujours pour cet agent** vaut pour l'agent qui a demandé et pour la famille d'action de l'outil ; vous le retrouvez, et le retirez, sur l'écran **Autorisations**. Un refus accepte une raison facultative, transmise à l'agent. Pour le code, la réponse durable est **Toujours autoriser cette commande** : elle est proposée avec le mode développeur activé, pour une commande simple et unique, et toute autre exécution redemande. Un outil que son serveur ne décrit pas est traité comme sensible et demandé à chaque fois, sans **Toujours pour cet agent**.

## D'où vient la famille d'un outil

- Pour un service de la sélection d'Apollia, la sélection donne sa famille à chaque outil.
- Pour tout autre service, la famille vient de ce que le serveur déclare sur l'outil : un outil qu'il déclare en lecture seule est une lecture (une lecture de données personnelles derrière un compte), un outil qu'il déclare destructeur est une écriture ou suppression. Cela vaut que l'outil soit montré d'emblée à l'agent ou trouvé par `tool_search` : un outil en lecture seule d'un tel service s'exécute donc sans demander, ou peut recevoir **Toujours pour cet agent** quand il lit un compte.
- Sur la fiche d'un tel service, dans **Connexions**, **Changer** à côté d'un outil lui donne une autre famille, et **Appliquer** l'enregistre. Votre choix l'emporte sur ce que déclare le serveur, reste quand le service est ajouté à nouveau, et est inscrit au **Journal** avec la famille d'avant et celle d'après. Un outil de la sélection d'Apollia ne se change pas ainsi.

Il n'existe pas d'écran d'approbations propre au MCP, ni de file d'attente de demandes MCP à traiter. Un outil exposé par un serveur MCP passe par la même porte qu'un outil natif, celle qu'ouvre la boucle d'exécution : la carte dans la conversation si vous y êtes, l'écran **À valider** pour une tâche d'agent en attente. Tout ce que le reste de ce centre d'aide dit sur l'approbation d'une action s'y applique sans changement.

## Voir et changer les règles

Ouvrez **Autorisations** dans la barre latérale. L'écran liste les agents, et pour chacun :

- **Accès accordés, par famille** : ce que l'agent peut utiliser, avec la règle de chaque famille.
- **« Toujours » donnés** : les accords auxquels vous avez répondu **Toujours pour cet agent**, chacun avec un bouton **Retirer**.
- **Règles fines** : les règles sur une commande ou un outil précis, chacune avec un bouton **Retirer**. Les **Règles pour tous les agents** sont listées à part.
- Les décisions elles-mêmes ne sont pas listées sur cet écran : chaque consentement donné et chaque refus est une entrée du **Journal**.

Pour en retirer plusieurs d'un coup, **Tout révoquer** demande ce qui est retiré (les règles fines, les « toujours » ou les deux) et pour quelle portée (tous les agents ou un agent), puis affiche combien sont concernés. Il ne demande aucune confirmation à taper. Chaque retrait est inscrit au **Journal**.

Il n'existe pas de niveau d'approbation par serveur. Dans **Connexions**, la fiche d'un service contient ses réglages, ses délais d'attente, **Mettre en pause** et **Retirer**, et rien sur l'approbation : un outil se décide sur sa famille d'action, quel que soit le serveur dont il vient.

Un « toujours » apparaît tout seul, quand vous répondez **Toujours pour cet agent** à une demande d'accord. Une règle fine s'écrit à la main depuis cet écran, via **Ajouter une règle fine**, avec le mode développeur activé.

## Ce que la politique de l'organisation interdit

Interdire le cloud est une décision de votre organisation, pas une règle par
outil. Elle est écrite dans la politique signée que l'informatique peut déployer
sur le poste, sous la forme `connections.remote = "deny"`, et c'est le runtime
qui la vérifie.

- **Aucune politique** : vous êtes libre. Aucune connexion n'est refusée parce qu'elle est en ligne.
- **Une politique avec `connections.remote = "deny"`** : ajouter une connexion à un service en ligne est refusé, qu'il s'agisse d'un compte Google ou Microsoft ou d'un serveur MCP distant. La fenêtre affiche « Refusé par la politique de l'organisation » et rien n'est envoyé.

La politique est lue au démarrage d'Apollia. Une connexion qu'elle interdit n'est pas démarrée : elle indique **Bloquée par l'organisation** dans **Connexions**, avec la raison *interdite par la politique*, et un appel à l'un de ses outils est refusé. Une politique peut aussi interdire un type d'action ou un outil nommé, et ce refus tient quoi que vous ayez répondu auparavant.

## Ce qu'un serveur MCP peut, et ne peut pas, vous demander

La spécification permet à un serveur de rappeler son client de trois façons.
Apollia n'en honore aucune aujourd'hui, et en annonce une.

- **Roots**, annoncé mais sans réponse : Apollia déclare la capacité pendant la poignée de main, et rien ne répond à une requête `roots/list`. Aucun répertoire n'est déclaré : ce n'est donc pas une frontière filesystem et il ne faut pas la lire comme telle. Ce qui borne réellement un serveur local, c'est la commande et les arguments que vous lui avez donnés.
- **Sampling**, non implémenté : un serveur ne peut pas demander à Apollia de faire un appel LLM pour lui.
- **Elicitation**, non implémenté : un serveur ne peut pas vous demander une saisie structurée.

Les deux dernières ne sont pas annoncées pendant la poignée de main : un serveur
découvre donc leur absence à la connexion, et non en envoyant une requête qui
reste sans réponse. Roots est le seul cas où l'annonce devance l'implémentation.

## Vérification

- Ouvrez **Autorisations** dans la barre latérale, la liste des agents s'affiche.
- Dans le chat, déclenchez une écriture (envoi de mail), la demande d'accord apparaît.
- Répondez **Toujours pour cet agent**, et vérifiez qu'une nouvelle ligne apparaît sous **« Toujours » donnés** pour cet agent.

## Si ça ne marche pas

- **Un outil qui ne fait que lire demande votre accord** : il lit un compte connecté, donc des données personnelles, ou son serveur ne l'a pas décrit et il est traité comme sensible. Répondez **Toujours pour cet agent** pour que la question cesse pour cet agent.
- **Un outil sensible s'exécute sans demande** : vous avez répondu **Toujours pour cet agent** un jour, pour sa famille. Ouvrez **Autorisations** et retirez cet accord.
- **Une connexion ne répond plus après le déploiement d'une politique** : vérifiez si elle indique **Bloquée par l'organisation** dans **Connexions**. Si ce n'est pas le cas, regardez la connexion elle-même, via [Vérifier une connexion et la réparer](test-an-mcp-connection.md).
- **Un outil demande toujours alors qu'il ne fait que lire** : son serveur ne le décrit pas. S'il s'agit d'un service hors de la sélection d'Apollia, donnez-lui la famille **Lire** avec **Changer** sur la fiche de la connexion.

> **Référence technique :** [Référence Apollia](/reference) , gouvernance complète, audit trail, format des règles dans `governance.db`.
