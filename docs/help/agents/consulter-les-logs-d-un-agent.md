# Consulter les logs d'un agent

> Pour comprendre ce qu'un agent a fait, ou pourquoi il a échoué : ouvrir son panneau Logs et parcourir l'historique de ses tâches.

## Prérequis

- L'agent est installé et a été démarré au moins une fois.
- Idéalement, l'agent a déjà exécuté une mission (au moins quelques tâches disponibles).

## Étapes

1. Dans la sidebar, cliquez sur **Mes assistants**.

2. Localisez la carte de l'agent dont vous voulez consulter l'activité.

3. Cliquez sur **Logs** sur sa carte. Un panneau s'ouvre à droite, intitulé **Logs de l'agent**, avec un compteur de tâches en haut.
   ![panneau Logs ouvert avec compteur, barre de recherche, filtres de statut et tri](../_screenshots/agents-consulter-les-logs-d-un-agent-1.png)

   > **Note :** ce panneau affiche l'historique des tâches exécutées par l'agent. Il ne s'agit pas d'un journal textuel avec niveaux Info/Warning/Error.

4. Chaque ligne de la liste contient déjà l'essentiel - pas besoin de cliquer pour ouvrir un détail :
   - **Statut** (à gauche) - voir la liste ci-dessous.
   - **Durée** d'exécution (ex. `850ms`, `2.4s`).
   - **Horodatage relatif** (ex. `5min ago`) - survolez-le pour voir la date et l'heure exactes.
   - **Entrée reçue** - la demande qui a déclenché la tâche.
   - **Résultat** ou **Erreur** - la sortie produite, ou le message d'erreur si la tâche a échoué.

5. Repérez les tâches par leur **statut** :
   - **Terminée** - tâche exécutée avec succès.
   - **Échouée** - tâche en erreur, à examiner.
   - **En cours** - tâche encore en cours d'exécution.
   - **Vérification** - statut transitoire visible aux paliers `supervised`, `bounded_autonomous` et `long_autonomous` : la boucle de vérification post-run est en cours : l'agent contrôle son travail avant de conclure. Ce statut précède toujours **Terminée** ou **Échouée** et peut durer plusieurs secondes selon la complexité de la tâche.
   - **Approbation** - l'agent attend une décision humaine (à traiter depuis l'Inbox).
   - **Soumise** - tâche enregistrée, pas encore prise en charge.
   - **Annulée** - tâche interrompue avant la fin.

6. **Filtrer la liste** quand il y a beaucoup de tâches :
   - Tapez dans la **barre de recherche** pour ne garder que les tâches dont l'entrée ou le résultat contient ce mot.
   - Cliquez sur une **puce de statut** (Toutes / Terminée / Échouée / En cours / Vérification / Approbation / Soumise / Annulée) pour ne voir que ce statut.
   - Utilisez le **menu de tri** en haut à droite des filtres pour ordonner par : Plus récentes (défaut), Plus anciennes, Plus longues, Plus courtes.
   - Le compteur en haut indique le nombre de tâches affichées vs total (ex. `4 / 27 tâches`).
   - Si aucune tâche ne correspond, un bouton **Réinitialiser les filtres** apparaît.

7. **Rafraîchir** la liste sans fermer le panneau : cliquez sur l'icône `↻` en haut à droite du panneau.

8. Fermez le panneau pour revenir à la liste des agents.

## Vérification

Vous voyez la liste des tâches de l'agent avec leur statut, leur durée et un aperçu de la demande et du résultat. Une tâche échouée récente est repérable d'un coup d'œil grâce à son statut rouge **Échouée** et son message d'erreur affiché juste en dessous.

## Si ça ne marche pas

- **Aucune tâche affichée :** l'agent n'a jamais été démarré ou n'a reçu aucune mission. Lancez-le et envoyez-lui une instruction.
- **Panneau vide après une exécution :** vérifiez que l'agent est bien démarré (statut ACTIF sur sa carte), puis cliquez sur `↻` pour rafraîchir.
- **Erreur incompréhensible :** copiez le message et consultez [Un agent est bloqué](../troubleshooting/un-agent-est-bloque.md).

> **Référence technique :** [Ops-Exploitation-et-Debug](https://github.com/Apollia-OS/apollia-os/wiki/Ops-Exploitation-et-Debug) - interprétation des statuts de tâche, dépannage agent bloqué ou en timeout.
