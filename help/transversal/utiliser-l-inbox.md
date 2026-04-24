# Utiliser l'inbox

> Pour les operators qui veulent traiter au même endroit toutes les décisions en attente : approbations d'actions, validations de pipelines, demandes des agents.

## Prérequis
- Au moins un agent ou un pipeline a déclenché une demande d'approbation.

## Étapes

1. Dans la sidebar, cliquez sur **Inbox**. La liste des actions en attente s'affiche, triée par date.
   `[SCREENSHOT: page Inbox, liste de cartes d'approbation avec agent, type d'action et horodatage]`

2. Cliquez sur une ligne pour afficher la carte d'approbation : type d'action, chemin ou commande concernée, aperçu du contenu qui sera produit.

3. Trois choix s'offrent à vous :
   - **Approuver** — l'action s'exécute immédiatement, la ligne passe dans l'historique.
   - **Refuser** — un dialogue vous demande la raison, l'agent est notifié et adapte sa suite.
   - **Archiver** — vous mettez la carte de côté sans décider (utile pour les notifications informatives).
   `[SCREENSHOT: carte d'approbation avec aperçu et trois boutons Approuver, Refuser, Archiver]`

4. Affinez l'affichage avec les filtres en haut :
   - Par **agent** pour traiter le travail d'un seul exécutant.
   - Par **statut** : en attente, approuvée, refusée.
   - Par **date** : aujourd'hui, cette semaine, ce mois.

5. Le compteur en haut de la page récapitule l'état : *3 en attente, 12 approuvées aujourd'hui, 1 refusée*.

6. Pour consulter l'historique des décisions passées, basculez le filtre statut sur **Approuvée** ou **Refusée**. Vous voyez qui a décidé quoi et quand.

## Vérification
Le compteur en haut redescend à zéro lorsque toutes les demandes en attente sont traitées.

## Si ça ne marche pas
- **L'inbox reste vide alors qu'un agent attend** : vérifiez que l'agent est bien démarré (page Agents). Une approbation requiert un agent actif.
- **Approuver ne déclenche rien de visible** : ouvrez la conversation ou la timeline liée à l'agent, l'action s'exécute immédiatement et son résultat y apparaît.
- **Vous voulez éviter de réapprouver toujours la même chose** : cochez **Toujours autoriser** sur la carte avant d'approuver. Voir *Approuver ou refuser une action d'agent*.

> **Concept :** [book ch11 — HITL et contrôle](https://github.com/nidal-z/apollia-os/blob/main/book/src/ch11-00-hitl-et-controle.md)
