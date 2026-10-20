---
title: La vue d'un lot
slug: /operator-help/screens/batch
sidebar_position: 18
---

# La vue d'un lot

> Un lot : où il en est, ce que vous avez demandé, ce qu'il a produit, élément par élément.

## À quoi ça sert

La vue d'un lot s'ouvre dès qu'un lot est lancé, et de nouveau depuis l'historique des lots. Elle dit où en est le lot, rappelle ce que vous avez demandé à l'agent, liste les fichiers que le lot a produits, et montre ses résultats : en tableau quand vous avez demandé des colonnes de résultat, et élément par élément dans tous les cas.

## La barre d'en-tête

La barre du haut suit le lot pendant qu'il tourne :

- Avancement : combien d'éléments sont traités sur le total, avec le nombre d'éléments réussis, en échec, en attente de validation et restant à traiter.
- Rythme : le temps pris par élément et l'heure vers laquelle le lot devrait finir.
- **Pause**, **Reprendre** et **Arrêter** : un lot en pause ne démarre aucun nouvel élément tant que vous ne le reprenez pas ; l'élément en cours se termine. Un lot arrêté ne reprend pas, et ses résultats restent exportables.
- **Relancer les échecs** : relance les éléments en échec.
- **Exporter…** : écrit les résultats dans un classeur Excel (.xlsx), un fichier CSV (.csv) ou un document JSON (.json). Chaque champ de la réponse a sa propre colonne. Pour un lot sur un dossier, l'export contient le nom de chaque fichier, pas son texte.

## Ce que vous avez demandé, et les fichiers produits

La carte **Ce que vous avez demandé** rappelle le lancement : la consigne envoyée avec chaque élément, les colonnes du résultat avec ce que chacune contient, et la synthèse demandée à la fin, s'il y en a une.

À côté, **Fichiers produits** liste ce que le lot a écrit sur ce poste :

- le classeur avec les colonnes de l'agent, pour un lot sur un fichier Excel ou CSV ;
- la synthèse du lot, quand une synthèse a été demandée ;
- les exports faits depuis cet écran.

Chaque fichier propose **Ouvrir** et **Afficher dans le dossier**.

Quand une synthèse a été demandée, la carte **Synthèse du lot** l'affiche une fois le dernier élément traité.

## Résultats et Éléments

Dès qu'un lot avec des colonnes de résultat a des éléments terminés, deux onglets apparaissent : **Résultats** et **Éléments**. Pendant que le lot tourne, la vue s'ouvre sur **Éléments** ; une fois le lot fini, sur **Résultats**.

**Résultats** est un tableau : une ligne par élément, une colonne par champ.

- Cliquez sur l'en-tête d'une colonne pour la trier. Les montants sont lus comme des nombres.
- Un champ qui prend peu de valeurs devient un filtre, avec le nombre de chaque valeur, par exemple « Conforme : oui 36, non 4 ». **Tout afficher** retire les filtres.
- Une cellule laissée vide, ou une réponse qui n'a pas donné ses champs, est signalée à relire.
- Cliquez sur une ligne pour ouvrir l'élément.

**Éléments** montre la liste des éléments à côté du détail de celui que vous ouvrez : ses étapes, sa réponse champ par champ, son entrée et les sources qu'il a consultées. Au-dessus de la liste, des onglets trient les éléments par état, comme **Tous**, **Réussis** ou **En échec**, et un champ de recherche les filtre. Pendant que le lot tourne, **Suivre l'élément en cours** garde le détail sur l'élément en train d'être traité.

Un élément qui attend un accès non accordé pour le lot attend aussi dans **À valider** : voir [Approuver ou refuser une action d'agent](/operator-help/control/approve-or-reject-an-action).

## Continuer dans le Chat

Une fois le lot fini, **Continuer dans le Chat** ouvre une conversation avec l'agent du lot. Le tableau des résultats y est joint en fichier CSV, et le début d'une demande est écrit dans la zone de saisie. Rien n'est envoyé : terminez la demande, puis envoyez-la.

## Refaire le lot sur une autre source

Une fois le lot fini, **Refaire ce lot…** ouvre la fenêtre de lancement avec le même agent, la même consigne, les mêmes colonnes de résultat et la même synthèse. Il ne reste qu'à choisir la source.

## Ce que vous pouvez faire

- Suivre le lot, le mettre en pause, le reprendre ou l'arrêter.
- Lire les résultats en tableau, les trier et les filtrer, et ouvrir un élément pour lire son détail.
- Ouvrir les fichiers que le lot a produits, ou exporter ses résultats.
- Relancer ses échecs.
- Continuer dans le Chat avec le tableau des résultats, ou refaire le même lot sur une autre source.

## Pour aller plus loin

- [Confier une tâche à un agent](/operator-help/agents/start-an-agent)
- [L'historique des lots](/operator-help/screens/batches)
- [Approuver ou refuser une action d'agent](/operator-help/control/approve-or-reject-an-action)
