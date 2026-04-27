# Lire le digest quotidien

> Pour les operators qui veulent voir d'un coup d'œil ce que leurs agents ont fait dans les dernières 24 heures et ce qui demande leur attention.

## Prérequis
- Au moins un agent ou trigger a tourné dans les 24 dernières heures.
- Vous êtes connecté à un fournisseur d'IA (le bandeau de connexion est vert).

## Étapes

1. Ouvrez l'application. Vous arrivez par défaut sur le **Dashboard**.

2. En haut de la page, regardez le bloc **Digest** : trois grands chiffres résument votre journée.
   - **Terminées** : actions menées à bien par vos agents.
   - **En attente** : décisions qui vous attendent (approbations).
   - **Erreurs** : exécutions qui ont échoué et méritent un coup d'œil.
   `[SCREENSHOT: bloc DigestHero en haut du Dashboard, trois nombres mis en valeur]`

3. Cliquez sur l'un des trois chiffres pour ouvrir le détail correspondant. Par exemple, un clic sur **En attente** vous emmène dans l'**Inbox**, sur la liste des actions à valider.

4. Pour la vue complète, dans la sidebar cliquez sur **Observabilité**, puis sur l'onglet **Timeline**. Vous voyez tous les événements de la journée, du plus récent au plus ancien.

5. Filtrez la timeline avec les listes déroulantes en haut : par agent, par type d'événement (tâche, outil, approbation), par statut.
   `[SCREENSHOT: page Observabilité onglet Timeline, barre de filtres en haut, événements listés en dessous]`

6. Pour archiver le digest, cliquez sur **Exporter** en haut à droite. Choisissez **JSON**. Le fichier est enregistré dans votre dossier Téléchargements.

## Vérification
Le bloc Digest affiche des chiffres cohérents avec votre activité. La timeline contient au moins une ligne par exécution récente.

## Si ça ne marche pas
- **Le digest est entièrement à zéro** : aucun agent n'a tourné depuis 24h. Démarrez un agent ou déclenchez un trigger manuellement pour générer de l'activité.
- **Les chiffres ne se mettent pas à jour** : tirez la page vers le bas pour rafraîchir, ou cliquez sur le bouton de recharge en haut à droite du Dashboard.
- **Une erreur affichée n'a pas de détail** : ouvrez la page **Agents**, cliquez sur l'agent concerné, onglet **Logs** pour voir le message complet.

> **Référence technique :** [Ops-Exploitation-et-Debug](https://github.com/nidal-z/apollia-os/wiki/Ops-Exploitation-et-Debug)
