# Configurer la rétention de mémoire

> Pour les operators qui veulent contrôler combien de temps leurs IA gardent les informations, et libérer de l'espace automatiquement.

## Prérequis

- Au moins un agent actif ayant déjà généré de la mémoire.
- Vous savez quel type d'information vous voulez garder longtemps, et lequel oublier vite.

## Étapes

1. Dans la sidebar, cliquez sur **Settings**, puis sur l'onglet **Mémoire**.
   `[SCREENSHOT: page Settings, onglet Mémoire avec sliders de rétention]`

2. Repérez les trois curseurs de **durée de rétention** :
   - **Épisodique** — événements datés (défaut : 30 jours).
   - **Sémantique** — faits durables (défaut : 365 jours).
   - **Procédural** — méthodes apprises (défaut : illimité).

3. Réduisez l'**Épisodique** à 7 jours si vous voulez que l'IA oublie vite les anciens événements (utile pour les agents qui traitent beaucoup de tâches courtes).

4. Gardez le **Sémantique** long (365 jours ou plus) : ce sont les apprentissages utiles que l'IA réutilise au quotidien.

5. Laissez le **Procédural** sur **Illimité** : ces procédures changent rarement et structurent le comportement de vos agents.
   `[SCREENSHOT: trois sliders avec valeurs Épisodique 7j, Sémantique 365j, Procédural Illimité]`

6. Activez l'interrupteur **Purge automatique**. Apollia nettoie chaque nuit les entrées qui ont dépassé leur durée de rétention.

7. Pour libérer de l'espace immédiatement, cliquez sur **Purger maintenant**. Toutes les entrées expirées sont supprimées, les autres restent intactes.

8. En haut de la page, consultez les **statistiques** : taille totale de la mémoire et nombre d'entrées par type. Utile pour repérer un agent trop bavard.
   `[SCREENSHOT: bandeau statistiques avec mémoire utilisée 2.3 MB et 156 entrées]`

9. Cliquez sur **Enregistrer**. Les nouvelles règles s'appliquent à partir de la prochaine purge automatique.

## Vérification

Le compteur d'entrées baisse après un clic sur **Purger maintenant** si des entrées étaient déjà expirées. Le bandeau de statistiques se met à jour en temps réel.

## Si ça ne marche pas

- **Le bouton Purger maintenant ne change rien** : aucune entrée n'a encore expiré, c'est normal.
- **La purge automatique ne se déclenche pas** : vérifiez qu'Apollia tourne la nuit (machine non éteinte ou en veille profonde).
- **Vous avez réduit une durée et perdu trop d'information** : vous pouvez ré-augmenter le curseur, mais les entrées déjà supprimées ne reviennent pas.

> **Référence technique :** [Briques-Memory-Engine](https://github.com/nidal-z/apollia-os/wiki/Briques-Memory-Engine) — stratégies de rétention, compromis entre coût et richesse de contexte.
