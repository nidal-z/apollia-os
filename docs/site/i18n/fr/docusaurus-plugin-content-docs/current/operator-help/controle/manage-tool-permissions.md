---
title: Gérer les autorisations d'outils
slug: /operator-help/control/manage-tool-permissions
sidebar_position: 2
---

# Gérer les autorisations d'outils

> Pour les operators qui veulent relire ce que chaque agent a le droit de faire sur ce poste, et retirer un accord, sans attendre qu'une action déclenche une nouvelle demande.

## Prérequis

- L'application est ouverte et au moins un agent est installé. L'agent Apollia, celui du Chat, est toujours listé.

## Comment les autorisations sont-elles créées ?

L'écran **Autorisations** réunit trois choses pour chaque agent :

- **Ses accès, par famille.** Ils viennent de la fiche de l'agent : ce qui lui a été accordé à son installation ou à sa création, et ce que vous avez ajusté depuis. Les familles sont **Lire**, **Lire des données personnelles**, **Écrire ou supprimer**, **Envoyer vers l'extérieur** et **Exécuter du code**.
- **Ses « toujours ».** Chacun est un accord que vous avez donné en répondant **Toujours pour cet agent** à une demande (voir [Approuver ou refuser une action d'agent](approve-or-reject-an-action.md)). Il couvre une famille d'action, pour cet agent seulement.
- **Ses règles fines.** Une règle fine autorise ou refuse une action précise. Il s'en écrit une quand vous répondez **Toujours autoriser cette commande** à une demande d'exécution de code, et d'autres s'écrivent à la main, mode développeur allumé.

Une autorisation a l'une de deux portées, et aucune autre : **un agent**, ou **tous les agents**. Un « toujours » vaut toujours pour un agent. Une règle fine vaut pour un agent ou pour tous les agents.

## Relire les autorisations d'un agent

1. Dans la barre latérale, cliquez sur **Autorisations**.

2. En haut, le panneau **Politique de l'organisation** dit si votre organisation a déposé une politique sur ce poste, et ce qu'elle impose. Sans politique, il indique *Aucune politique : vous êtes libre.*

3. La liste de gauche a deux parties :
   - **Pour tous les agents**, avec la ligne **Règles pour tous les agents**.
   - **Agents**, une ligne par agent, l'agent Apollia en premier. Sous chaque nom, une ligne compte ses familles, ses « toujours » et ses règles fines.

   Le champ **Filtrer par agent, famille ou accès…**, au-dessus de la liste, la réduit.

4. Cliquez sur un agent. Le panneau de droite montre :
   - **Accès accordés, par famille** : chaque famille où l'agent détient un accès, ce que cet accès lui permet, et un badge qui dit ce que la famille demande avant que l'agent agisse : **Sans demande** pour la lecture, **Votre accord est demandé**, **Toujours accordé** quand un « toujours » la couvre, **Accord à chaque fois** pour l'exécution de code, ou **Imposé par la politique** quand la politique de votre organisation interdit la famille.
   - **« Toujours » donnés** : chaque accord permanent, avec la date à laquelle il a été donné.
   - **Règles fines** : chaque règle en clair, avec un badge **Autorise** ou **Refuse**, sa famille, qui elle vise et la date de son ajout.

   **Ouvrir la fiche** mène à la fiche de l'agent, où ses accès s'ajustent.

<!-- claim:prefix-rules-evaluated-per-invocation -->
> **Ce que fait réellement une règle fine :** une règle fine est vérifiée à chaque action des agents qu'elle vise, sur la demande elle-même. Une règle qui **refuse** l'emporte toujours : l'action qu'elle reconnaît est refusée, même quand un « toujours » l'aurait laissée passer. Une règle qui **autorise** une commande laisse cette commande s'exécuter sans demande, tant que le mode développeur est allumé.
<!-- claim:executor-guard-blocks-command-chaining -->
> **Le code** est plus strict : une règle qui autorise ne s'applique qu'à une **commande simple unique** qui commence par le texte de la règle, sans chaînage (`;`, `&&`, `||`), pipe, redirection (`>`, `<`) ni substitution (`` ` ``, `$(...)`). Une règle sans commande ne laisse jamais du code s'exécuter sans demande.

## Retirer un accord ou une règle

1. Sélectionnez l'agent, ou **Règles pour tous les agents**.
2. Sur la ligne du « toujours » ou de la règle fine, cliquez sur **Retirer**.
3. Le retrait prend effet aussitôt et il est inscrit au Journal. La ligne reste en place, marquée **Retiré**, avec un bouton **Annuler** qui la rétablit, jusqu'à ce que vous quittiez l'écran.

Une fois un « toujours » retiré, la prochaine action de cette famille par cet agent redemande votre accord.

## Tout révoquer d'un coup

1. Cliquez sur **Tout révoquer**, au-dessus de la liste.
2. Sous **Ce qui est retiré**, choisissez **Les règles fines seulement**, **Les « toujours » seulement** ou **Les deux**.
3. Sous **Portée**, choisissez **Tous les agents** ou **Un agent**, et désignez l'agent.
4. Lisez la ligne qui dit combien de règles fines et combien de « toujours » sont concernés, puis cliquez sur **Tout révoquer**.

C'est immédiat et irréversible. Les accès accordés par famille ne changent pas : ils s'ajustent sur la fiche de chaque agent.

## Écrire une règle fine (mode développeur)

Mode développeur allumé (**Réglages**, section **Développeur**), l'écran porte aussi un bouton **Ajouter une règle fine**, et chaque règle fine reçoit **Modifier** et **Dupliquer**. La fenêtre demande :

- **Effet** : **Autoriser** ou **Refuser**.
- **Famille d'action** et **Outil** : l'outil sur lequel porte la règle.
- **Préfixe d'argument** : le début de la demande que la règle reconnaît, par exemple `git status`. Pour un outil qui exécute du code, autoriser exige ici une commande simple.
- **Portée** : **Tous les agents** ou **Un agent**.

Chaque règle écrite ou modifiée est inscrite au Journal. Tout le monde voit les règles et peut les retirer, mode développeur éteint aussi.

## Les outils intégrés

Les outils fournis avec Apollia (recherche web, lecture de page, Python et les autres) ne s'activent pas et ne se règlent pas ici. Une ligne en bas de l'écran le dit, et **Ouvrir Connexions** mène à **Connexions**, section **Outils intégrés**. Voir [Inspecter un outil](inspect-a-tool.md).

## Relire les décisions prises

Cet écran montre ce qui est en vigueur, pas ce qui s'est passé. Chaque retrait et chaque règle écrite est inscrit au **Journal**, que vous ouvrez depuis la barre latérale ; son filtre **Nature** sépare les **Actions** des **Changements d'accès**. Voir [Consulter la piste d'audit](../observabilite/read-the-audit-trail.md).

## Si ça ne marche pas

- **Un agent ne montre aucun « toujours » et aucune règle fine** : aucun n'a été donné. Ses accès par famille s'appliquent quand même, et chaque action sensible demande votre accord.
- **Le bouton Retirer manque sur une famille** : un accès accordé par famille ne se retire pas ici. Cliquez sur **Ouvrir la fiche** et ajustez les accès de l'agent.
- **Le bouton Tout révoquer de la fenêtre est grisé** : le choix fait ne concerne aucune règle fine et aucun « toujours ».
- **Un agent qui n'est plus installé est listé** : il détient encore un « toujours » ou une règle fine, que vous pouvez retirer.
- **Ajouter une règle fine ne s'affiche pas** : le mode développeur est éteint.

> **Référence technique :** [Référence Apollia](/reference)
