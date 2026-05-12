# Lire le résumé du jour

> Pour les operators qui veulent voir d'un coup d'œil ce que leurs agents ont fait et ce qui demande leur attention en ce moment.

## Prérequis

- Au moins un agent ou trigger a tourné récemment.

## Étapes

1. Ouvrez l'application. Vous arrivez par défaut sur le **Tableau de bord**.

2. La page affiche trois cartes côte à côte, plus une bande d'activité récente en dessous.

   - **Décisions en attente** *(la plus large, à gauche)* — actions HITL en attente de votre décision. Compteur en en-tête. Liste compacte des premiers items (extraits d'inbox) ; un lien *« Voir tout → »* mène à la **Boîte de réception**.
   - **Livrables prêts** — tâches récemment complétées. Cliquez sur une ligne pour ouvrir l'onglet **Tâches**.
   - **Au travail** — agents actuellement actifs. Cliquez sur un agent pour ouvrir son détail.
   `[SCREENSHOT: tableau de bord en mode opérateur — 3 cartes en grille, "Décisions en attente" à gauche occupe deux colonnes, "Livrables prêts" et "Au travail" à droite]`

3. Sous les trois cartes, la section **Activité récente** liste les dernières tâches (toutes statuts confondus) sous forme de mini-cartes. Cliquez pour ouvrir la page **Tâches**.

4. Pour la vue détaillée par événement (tâche, outil, LLM, approbation, etc.), allez dans **Observabilité → Chronologie** depuis la sidebar.

## Vérification

Les compteurs des trois cartes correspondent à votre activité actuelle. Si vous lancez une nouvelle tâche, *« Au travail »* s'incrémente sans rafraîchissement manuel — les données se mettent à jour automatiquement via les flux temps réel.

## Si ça ne marche pas

- **Tout est vide alors qu'un agent vient de tourner** : la mise à jour temps réel a peut-être perdu sa connexion ; quittez et rouvrez l'application.
- **Une erreur récente n'est pas visible** : ouvrez **Mes assistants**, cliquez sur l'agent concerné, ouvrez ses **Logs** pour voir le message complet (cf. [Consulter les logs d'un agent](../agents/consulter-les-logs-d-un-agent.md)).

> **Référence technique :** [Ops-Exploitation-et-Debug](https://github.com/nidal-z/apollia-os/wiki/Ops-Exploitation-et-Debug)
