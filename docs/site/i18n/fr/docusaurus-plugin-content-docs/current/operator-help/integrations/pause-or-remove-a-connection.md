---
title: Mettre en pause ou retirer une connexion
slug: /operator-help/integrations/pause-or-remove-a-connection
sidebar_position: 6.5
---

# Mettre en pause ou retirer une connexion

> Pour tout opérateur qui veut arrêter une connexion un temps sans rien perdre, ou la retirer pour de bon.

## Prérequis

- Au moins une connexion dans **Connexions**.

## Mettre en pause, ou retirer ?

| | Mettre en pause | Retirer |
|---|---|---|
| Les accès des agents | Gardés tels quels. | Retirés à tous les agents. |
| Ses outils | Aucun ne s'exécute avant la reprise. | Disparus. |
| Ses réglages, secrets et connexion au compte | Gardés. | Effacés. |
| Revenir en arrière | **Reprendre**, et elle fonctionne comme avant. | Ajouter la connexion à nouveau, puis la redonner aux agents. |

## Mettre une connexion en pause

1. Dans **Connexions**, ouvrez la fiche de la connexion.
2. Cliquez sur **Mettre en pause**. L'état devient **En pause**, et la fiche dit que ses agents gardent leurs accès mais qu'aucun de ses outils ne s'exécute avant la reprise.

Pendant la pause :

- Un agent qui appelle l'un de ses outils essuie un refus, avec la raison : la connexion est en pause. Le refus est inscrit au **Journal** : *Une action de l'agent a été refusée : sa connexion est en pause*, pour une boîte mail comme pour un compte, un dossier ou un service.
- Un dossier n'est ni lu ni cherché, même quand il se trouve dans un autre dossier que l'agent tient : un agent qui y lit, liste ou écrit un fichier, quel que soit le chemin qu'il écrit, ou qui demande le contexte d'un dossier de code qu'il tient, essuie un refus avec la raison que la connexion est en pause, et la même ligne au **Journal** ; un agent qui liste ou fouille le dossier qui l'entoure n'en trouve rien ; un agent qui cherche dans ses sources apprend que celle-ci est en pause, et les autres sources qu'il tient sont cherchées.
- La fiche de chaque agent qui la tient marque l'accès **En pause**.
- Un service est arrêté et n'est plus jamais démarré, ni au lancement suivant d'Apollia, ni à la demande. Dans le Chat, le sélecteur **@** ne propose plus ses ressources.
- La pause tient après un redémarrage d'Apollia.

Une connexion **Bloquée par l'organisation** ne propose pas de pause : la politique l'arrête déjà.

## Reprendre une connexion

Sur la fiche, cliquez sur **Reprendre**. La liste et la fiche montrent aussitôt son nouvel état. Ses outils s'exécutent à nouveau pour les agents qui les tenaient, sans accès à redonner, et un service est redémarré.

## Retirer une connexion

1. Dans **Connexions**, ouvrez la fiche de la connexion.
2. Cliquez sur **Retirer**. Avant tout effacement, la fiche liste ce qui part avec la connexion, une ligne par chose :
   - ses réglages,
   - les fichiers installés pour elle,
   - sa connexion au compte,
   - chaque secret gardé pour elle,
   - chaque outil qu'un agent tenait par elle, avec le nom de l'agent,
   - pour un dossier, la source et son index,
   - pour un compte Microsoft, chaque dossier ou fichier OneDrive choisi pour lui.
3. Cliquez sur **Retirer la connexion** pour confirmer, ou sur **Annuler**.

La connexion quitte la liste. Si quelque chose n'a pas pu être effacé, un message le nomme, pour que vous le retiriez à la main. Retirer un compte Microsoft oublie aussi les dossiers et fichiers OneDrive que vous aviez choisis pour lui.

Retirer un compte n'efface sa connexion que sur cet ordinateur. Pour retirer aussi l'accès d'Apollia du côté du fournisseur, retirez l'application Apollia des réglages de votre compte : https://myaccount.microsoft.com pour Microsoft, https://myaccount.google.com/permissions pour Google.

## Vérification

- Après **Mettre en pause**, la ligne de la connexion indique **En pause**, et demander à un agent de s'en servir lui vaut un refus qui dit que la connexion est en pause.
- Après **Reprendre**, la ligne indique de nouveau son état, et la demande de l'agent passe.
- Après **Retirer**, la connexion n'est plus dans la liste, et les fiches des agents ne la montrent plus sous **Accès**.

## En cas de problème

- **Une connexion en pause semble encore répondre** : vérifiez que l'agent ne s'est pas servi d'une autre connexion du même genre. Deux comptes font deux connexions, chacune mise en pause pour elle-même.
- **Le retrait dit que quelque chose n'a pas pu être effacé** : le message le nomme. Un secret resté dans le trousseau du système peut être supprimé depuis l'outil de trousseau de votre système.

> **Référence technique :** [Référence Apollia](/reference) , routes des connexions, gouvernance, journal d'audit.
