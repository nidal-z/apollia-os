# Suivre l'historique d'un trigger

> Pour les operators qui veulent vérifier qu'une automatisation a bien tourné cette nuit, ou comprendre pourquoi un déclenchement a été ignoré ou a échoué.

## Prérequis

- Une automatisation déjà créée.
- Au moins un déclenchement passé (manuel via l'icône lecture, ou programmé).

## Étapes

1. Dans la sidebar, cliquez sur **Mes déclencheurs**.

2. Repérez la ligne de l'automatisation qui vous intéresse dans la table. Passez la souris dessus pour faire apparaître les actions à droite.

3. Cliquez sur l'icône **⋯** (trois points) à droite de la ligne → **Voir l'historique**. Un panneau coulissant s'ouvre depuis la droite, intitulé **Historique des déclenchements**, avec un compteur en haut indiquant le nombre total d'événements.
   ![ligne d'automatisation au hover, menu trois points ouvert avec "Voir l'historique" surligné, panneau d'hist...](../_screenshots/automatisations-suivre-l-historique-d-un-trigger-1.png)

4. Chaque ligne de la liste contient déjà l'essentiel — pas besoin de cliquer pour ouvrir un détail :
   - **Statut** (à gauche) — badge coloré DÉCLENCHÉ / IGNORÉ / ERREUR.
   - **Horodatage relatif** à droite (ex. `5min ago`) — survolez-le pour voir la date et l'heure exactes.
   - **Agent** ciblé par le déclenchement.
   - **Identifiant court** de la tâche créée (8 premiers caractères, ou `—` si aucune tâche n'a été produite).
   - **Raison** affichée en rouge sur une ligne dédiée pour les statuts ERREUR.

   ![panneau Historique des déclenchements ouvert — chips de filtre statut en haut, liste de cartes empilées : u...](../_screenshots/automatisations-suivre-l-historique-d-un-trigger-2.png)

5. Repérez les **statuts possibles** :
   - **DÉCLENCHÉ** — le déclenchement a bien eu lieu et une tâche a été créée pour l'assistant. Ce statut ne dit rien du résultat de la tâche elle-même : pour savoir si l'agent a réussi son travail, voir [Consulter les logs d'un agent](../agents/consulter-les-logs-d-un-agent.md).
   - **IGNORÉ** — le déclenchement a été ignoré parce qu'une exécution précédente était encore en cours. Comportement contrôlé par le réglage *« si un déclenchement est déjà en cours »* du mode avancé (file d'attente ou abandon).
   - **ERREUR** — le déclenchement lui-même a échoué (avant même de créer la tâche). La **raison** s'affiche en rouge sur une ligne dédiée juste en dessous.

6. **Filtrer la liste** quand il y a beaucoup de déclenchements :
   - Cliquez sur une **puce de statut** (Tous / Déclenché / Ignoré / Erreur) pour ne voir que ce statut.
   - Utilisez le **menu de tri** en haut à droite des filtres pour ordonner par : Plus récents (défaut) ou Plus anciens.
   - Le compteur en haut indique le nombre de déclenchements affichés vs total (ex. `4 / 27 déclenchements`).
   - Si aucun déclenchement ne correspond, un bouton **Réinitialiser les filtres** apparaît.

7. **Rafraîchir** la liste sans fermer le panneau : cliquez sur l'icône `↻` en haut à droite du panneau. Pour fermer le panneau, cliquez en dehors ou utilisez la touche `Échap`.

## Voir le résultat de la tâche associée

Le panneau Historique n'affiche que les événements de déclenchement, pas le détail de ce que l'assistant a produit. Pour ça :

1. Notez l'identifiant court de tâche affiché sur la ligne DÉCLENCHÉ.
2. Fermez le panneau, rendez-vous sur **Mes assistants**.
3. Ouvrez les logs de l'assistant correspondant ([Consulter les logs d'un agent](../agents/consulter-les-logs-d-un-agent.md)) et repérez la tâche par son préfixe d'identifiant.

## Vérifier la prochaine exécution

Le panneau Historique n'affiche pas la prochaine échéance. Elle est visible directement sur la ligne du tableau **Automatisations**, dans la colonne **Prochain déclenchement** (par exemple *« dans 2 h »* ou *« demain 08:00 »*).

## Vérification

L'historique affiche au moins une ligne avec le statut attendu. Pour une automatisation qui tourne sans accroc, vous verrez une succession de **DÉCLENCHÉ** verts.

## Si ça ne marche pas

- **L'historique est vide** : aucun déclenchement n'a encore eu lieu. Lancez-en un manuellement avec l'icône **▶︎** sur la ligne du tableau, puis cliquez sur **↻** dans le panneau pour rafraîchir.
- **Tous les déclenchements sont en IGNORÉ** : la précédente exécution n'a jamais terminé. Vérifiez l'état de l'assistant dans **Mes assistants** ; consultez ses logs pour voir si la tâche en cours est encore au statut **En cours** depuis trop longtemps. Si oui, voir [Un agent est bloqué](../troubleshooting/un-agent-est-bloque.md). Une fois la tâche débloquée, les futurs déclenchements repasseront en DÉCLENCHÉ.
- **Tous les déclenchements sont en ERREUR** : lisez la raison affichée en rouge sous chaque ligne. Causes fréquentes : assistant désinstallé, secret de webhook invalide, expression cron invalide.
- **Je ne vois pas d'identifiant de tâche associé à un DÉCLENCHÉ** : rare ; la tâche a peut-être été créée puis immédiatement supprimée. Cherchez par préfixe d'identifiant dans **Mes assistants → Logs**.

> **Référence technique :** [Ops-Exploitation-et-Debug](https://github.com/nidal-z/apollia-os/wiki/Ops-Exploitation-et-Debug) — interprétation détaillée des statuts FIRED/SKIPPED/ERROR, comportement `on_busy`, dépannage des déclenchements bloqués.
