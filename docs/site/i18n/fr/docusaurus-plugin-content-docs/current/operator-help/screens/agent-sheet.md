---
title: La fiche d'un agent
slug: /operator-help/screens/agent-sheet
sidebar_position: 3
---

# La fiche d'un agent

> Tout ce qu'un agent est et fait, sur une page de l'écran Agents.

## À quoi ça sert

La fiche dit ce que fait l'agent (sa mission et des exemples de demandes), quel modèle répond pour lui, ce qu'il a le droit de faire par famille d'action, quelles sources il consulte, et ce qu'il a fait. Ses onglets séparent la fiche elle-même, l'activité et la mémoire.

Deux sources changent ce que l'agent garde. Avec **Mémoire de l'agent** cochée, l'agent garde des notes qui lui sont propres et les retrouve d'une conversation à l'autre. Avec **Profil** coché, l'agent lit votre profil et peut proposer un fait à y ajouter, que vous acceptez ou refusez à chaque fois.

## Les sources où l'agent cherche, et les fichiers qu'il ouvre

Un fichier ou un dossier parmi les sources de l'agent est fouillé, pas ouvert : l'agent peut chercher dans un fichier, pas l'ouvrir en entier, et chercher dans un dossier, pas en ouvrir les fichiers.

- Sur la ligne d'une source fichier, **Lui permettre de l'ouvrir** accorde à l'agent la lecture du dossier connecté qui contient le fichier. Quand aucun dossier connecté ne le contient, le bouton nomme le dossier du fichier et le connecte sur place.
- Sur la ligne d'une source dossier, **Lui permettre d'en ouvrir les fichiers** accorde à l'agent la lecture de ce dossier.

Pour ajouter un dossier, cliquez sur **Ajouter une source**, puis sur **Un dossier**. La fenêtre liste vos connexions dossier et propose **Choisir un dossier sur ce poste…** : le dossier choisi est connecté directement, sans passer par **Connexions**.

## Les pages scannées d'une source

Une source lit le texte de ses documents, et aussi les pages scannées et les images qu'elle contient : Apollia en reconnaît le texte, sur ce poste. La ligne de chaque source dit où en est sa lecture, et deux mentions peuvent suivre le compte de ses fichiers :

- **N pages en attente de reconnaissance** : des pages scannées ou des images, qu'Apollia ne peut pas encore lire parce que la langue de lecture n'est pas sur ce poste. La langue se télécharge d'elle-même la première fois qu'une source contient une page scannée ; sans connexion au réseau, elle reste absente. À côté de la mention, **Importer la langue** ouvre l'écran IA, où **Importer une langue de lecture** prend le fichier de langue sur un disque ou une clé. Les pages en attente sont lues dans la minute qui suit, sans redémarrer Apollia, et la mention disparaît.
- **N pages non lues** : des pages sans texte que la reconnaissance ne sait pas lire non plus, par exemple une page faite de plusieurs images, ou une page sur laquelle le moteur a échoué. Importer une langue n'y change rien.

Aucune des deux mentions n'apparaît quand toutes les pages ont été lues.

## Ce que vous pouvez faire

- Ajuster l'agent : sa mission, son modèle, ses accès et ses sources.
- Ouvrir une exécution passée pour lire son entrée, sa sortie et ses étapes.
- Relire ce que l'agent a retenu, y chercher et l'effacer.

## Pour aller plus loin

- [Choisir le niveau d'autonomie d'un agent](/operator-help/agents/choose-an-autonomy-level)
- [Confier une tâche à un agent](/operator-help/agents/start-an-agent)
- [Consulter les logs d'un agent](/operator-help/agents/view-an-agent-logs)
