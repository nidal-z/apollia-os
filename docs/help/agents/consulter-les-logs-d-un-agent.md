# Consulter les logs d'un agent

> Pour tout operator qui veut comprendre ce qu'un agent a fait, ou pourquoi il a échoué : ouvrir son panneau d'activité et lire l'historique de ses tâches.

## Prérequis

- L'agent est installé et a été démarré au moins une fois.
- Idéalement, l'agent a déjà exécuté une mission (au moins quelques tâches disponibles).

## Étapes

1. Dans la sidebar, cliquez sur **Mes assistants**.

2. Localisez la carte de l'agent dont vous voulez consulter l'activité.

3. Cliquez sur **Logs** sur sa carte. Un panneau slide-over s'ouvre, listant l'**historique des tâches** par ordre chronologique.
   `[SCREENSHOT: panneau Logs ouvert avec liste de tâches, colonnes statut et horodatage]`

   > **Note :** ce panneau affiche l'historique des tâches exécutées par l'agent (statuts : `working`, `completed`, `failed`, `input_required`, `canceled`). Il ne s'agit pas d'un journal de logs textuels avec niveaux de sévérité Info/Warning/Error.

4. Repérez les tâches par leur **statut** :
   - **Completed** — tâche terminée avec succès.
   - **Failed** — tâche échouée, à examiner.
   - **Working** — tâche en cours d'exécution.
   - **Input required** — l'agent attend une décision humaine (Inbox).
   - **Canceled** — tâche annulée.

5. Cliquez sur une tâche pour afficher son **détail** : entrée reçue, sortie produite, durée d'exécution.

6. Pour les tâches échouées, ouvrez le détail et lisez le message d'erreur. C'est presque toujours là que se trouve la cause du problème.

7. Fermez le panneau pour revenir à la liste des agents.

## Vérification

Vous voyez la liste des tâches de l'agent avec leur horodatage et leur statut. Une tâche échouée récente est immédiatement identifiable.

## Si ça ne marche pas

- **Aucune tâche affichée :** l'agent n'a jamais été démarré ou n'a reçu aucune mission. Lancez-le et envoyez-lui une instruction.
- **Panneau vide après une exécution :** vérifiez que l'agent est bien démarré (statut ACTIF sur sa carte).
- **Erreur incompréhensible :** copiez le message et consultez [Un agent est bloqué](../troubleshooting/un-agent-est-bloque.md).

> **Référence technique :** [Ops-Exploitation-et-Debug](https://github.com/nidal-z/apollia-os/wiki/Ops-Exploitation-et-Debug) — interprétation des statuts de tâche, dépannage agent bloqué ou en timeout.
