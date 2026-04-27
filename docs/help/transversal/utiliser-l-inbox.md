# Utiliser l'inbox

> Pour les operators qui veulent traiter au même endroit toutes les décisions en attente : approbations d'actions, validations de pipelines, demandes des agents.

## Prérequis
- Au moins un agent ou un pipeline a déclenché une demande d'approbation.

## Étapes

1. Dans la sidebar, cliquez sur **Inbox**. La liste des actions en attente s'affiche, triée par date.
   `[SCREENSHOT: page Inbox, liste de cartes d'approbation avec agent, type d'action et horodatage]`

2. Cliquez sur une ligne pour afficher la carte d'approbation : type d'action, chemin ou commande concernée, aperçu du contenu qui sera produit.

3. Deux choix s'offrent à vous :
   - **Approuver** — l'action s'exécute immédiatement, la ligne passe dans l'historique.
   - **Refuser** — un dialogue vous demande la raison (10 caractères minimum), l'agent est notifié et adapte sa suite.
   `[SCREENSHOT: carte d'approbation avec aperçu et boutons Approuver et Refuser]`

   > **Note :** il n'y a pas de bouton **Archiver** dans l'Inbox. Les approbations sont traitées via les boutons Approuver et Refuser uniquement.

4. Affinez l'affichage avec les filtres disponibles :
   - Par **agent** pour traiter le travail d'un seul exécutant.
   - Par **statut** : en attente, approuvée, refusée.

5. Le compteur en haut de la page récapitule l'état global des approbations en attente.

6. Pour consulter l'historique des décisions passées, basculez le filtre statut sur **Approuvée** ou **Refusée**. Vous voyez qui a décidé quoi et quand.

## Vérification
Le compteur en haut redescend à zéro lorsque toutes les demandes en attente sont traitées.

## Si ça ne marche pas
- **L'inbox reste vide alors qu'un agent attend** : vérifiez que l'agent est bien démarré (page Mes assistants). Une approbation requiert un agent actif.
- **Approuver ne déclenche rien de visible** : ouvrez la conversation ou la timeline liée à l'agent, l'action s'exécute immédiatement et son résultat y apparaît.
- **Vous voulez éviter de réapprouver toujours la même chose** : utilisez les options de périmètre sur la carte avant d'approuver. Voir *Approuver ou refuser une action d'agent*.

> **Concept :** [book ch10 — HITL et contrôle](https://github.com/nidal-z/apollia-os/blob/main/book/src/ch10-00-hitl.md)
