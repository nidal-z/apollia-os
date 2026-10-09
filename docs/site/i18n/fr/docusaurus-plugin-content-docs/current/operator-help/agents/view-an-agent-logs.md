---
title: Consulter les logs d'un agent
slug: /operator-help/agents/view-an-agent-logs
sidebar_position: 4
---

# Consulter les logs d'un agent

> Pour comprendre ce qu'un agent a fait, ou pourquoi il a échoué : ouvrir son panneau Logs et parcourir l'historique de ses tâches.

## Prérequis

- L'agent est installé et a reçu au moins une tâche.
- Idéalement, l'agent a déjà exécuté une mission (au moins quelques tâches disponibles).

## Étapes

1. Dans la barre latérale, cliquez sur **Agents**.

2. Cliquez sur la ligne de l'agent dont vous voulez consulter l'activité. Sa fiche s'ouvre.

3. Ouvrez l'onglet **Activité** de la fiche, puis cliquez sur **Logs**. Un clic droit sur la ligne de l'agent propose la même entrée **Logs**. Un panneau s'ouvre à droite, intitulé **Logs de l'agent**, avec un compteur de tâches en haut.
   ![panneau Logs ouvert avec compteur, barre de recherche, filtres de statut et tri](/img/operator-help/agents-consulter-les-logs-d-un-agent-1.png)

   > **Note :** ce panneau affiche l'historique des tâches exécutées par l'agent. Il ne s'agit pas d'un journal textuel avec niveaux Info/Warning/Error.

4. Chaque ligne de la liste contient déjà l'essentiel - pas besoin de cliquer pour ouvrir un détail :
   - **Statut** (à gauche) - voir la liste ci-dessous.
   - **Durée** d'exécution (ex. `850ms`, `2.4s`, `1m 30s` au-delà de la minute).
   - **Horodatage relatif** (ex. `5min ago`) - survolez-le pour voir la date et l'heure exactes.
   - **Entrée reçue** - la demande qui a déclenché la tâche.
   - **Résultat** ou **Erreur** - la sortie produite, ou le message d'erreur si la tâche a échoué.

5. Repérez les tâches par leur **statut** :
   - **TERMINÉE** - tâche exécutée avec succès.
   - **ÉCHOUÉE** - tâche en erreur, à examiner.
   - **EN COURS** - tâche encore en cours d'exécution.
   - **APPROBATION** - l'agent attend votre décision, à donner dans l'écran **À valider**.
   - **SOUMISE** - tâche enregistrée, pas encore prise en charge.
   - **ANNULÉE** - tâche interrompue avant la fin.

6. **Filtrer la liste** quand il y a beaucoup de tâches :
   - Tapez dans la **barre de recherche** pour ne garder que les tâches dont l'entrée ou le résultat contient ce mot.
   - Cliquez sur une **puce de statut** (Toutes / TERMINÉE / ÉCHOUÉE / EN COURS / APPROBATION / SOUMISE / ANNULÉE) pour ne voir que ce statut.
   - Utilisez le **menu de tri** en haut à droite des filtres pour ordonner par : Plus récentes (défaut), Plus anciennes, Plus longues, Plus courtes.
   - Le compteur en haut indique le nombre de tâches affichées vs total (ex. `4 / 27 tâches`).
   - Si aucune tâche ne correspond, un bouton **Réinitialiser les filtres** apparaît.

7. **Rafraîchir** la liste sans fermer le panneau : cliquez sur l'icône `↻` en haut à droite du panneau. L'icône voisine, **Copier les logs**, copie les tâches affichées dans le presse-papiers.

8. Fermez le panneau pour revenir à la fiche de l'agent.

## Vérification

Vous voyez la liste des tâches de l'agent avec leur statut, leur durée et un aperçu de la demande et du résultat. Une tâche échouée récente est repérable d'un coup d'œil grâce à son statut rouge **ÉCHOUÉE** et son message d'erreur affiché juste en dessous.

## Si ça ne marche pas

- **Aucune tâche affichée :** l'agent n'a encore reçu aucune tâche. Confiez-lui-en une, voir [Confier une tâche à un agent](start-an-agent.md).
- **Panneau vide après une exécution :** cliquez sur `↻` pour rafraîchir.
- **Erreur incompréhensible :** copiez le message et consultez [Un agent est bloqué](../troubleshooting/an-agent-is-stuck.md).

> **Référence technique :** [Référence Apollia](/reference) - interprétation des statuts de tâche, dépannage agent bloqué ou en timeout.
