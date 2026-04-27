# Suivre l'historique d'un trigger

> Pour les operators qui veulent vérifier qu'un trigger a bien tourné cette nuit, ou comprendre pourquoi une exécution a échoué.

## Prérequis

- Un trigger déjà créé.
- Au moins une exécution passée (manuelle ou programmée).

## Étapes

1. Dans la sidebar, cliquez sur **Automatisations**.

2. Cliquez sur la ligne du trigger qui vous intéresse pour ouvrir son détail.
   `[SCREENSHOT: liste des triggers avec une ligne sélectionnée et le panneau de détail ouvert à droite]`

3. Ouvrez l'onglet **Historique**. Vous voyez la table des exécutions passées avec leurs colonnes Date/Heure, Statut, Durée, Actions.

4. Repérez les statuts possibles (affichés en anglais) :
   - **FIRED** — l'exécution s'est déclenchée et s'est terminée avec succès.
   - **SKIPPED** — l'exécution a été ignorée car une autre était déjà en cours.
   - **ERROR** — l'exécution a échoué, à investiguer.

5. Cliquez sur une ligne d'exécution pour afficher ses **détails** : le payload transmis, la sortie produite, et les éventuelles erreurs.
   `[SCREENSHOT: détail d'une exécution avec sections Payload, Sortie, Erreurs]`

6. Cliquez sur **Logs complets** pour ouvrir l'intégralité des journaux de l'agent pour cette exécution. Utile pour les échecs.

7. Filtrez la liste par statut (par exemple **ERROR uniquement**) pour ne voir que les exécutions à dépanner.

8. Consultez le compteur **Prochaine exécution** en haut du panneau pour savoir quand le trigger se relancera.
   `[SCREENSHOT: en-tête du trigger avec compteur Prochaine exécution dans 2h 14min]`

9. Pour copier les logs (afin de les partager ou de les analyser), cliquez sur l'icône **Copier** en haut du panneau de logs.

## Vérification

L'historique affiche au moins une ligne avec le statut attendu et la durée de l'exécution. Le compteur de prochaine exécution est cohérent avec la fréquence du trigger.

## Si ça ne marche pas

- **L'historique est vide** : aucun déclenchement n'a encore eu lieu, lancez **Déclencher maintenant** depuis la liste.
- **Le statut reste sur FIRED indéfiniment** : le déclenchement a eu lieu mais l'agent est peut-être bloqué, voir [Un agent est bloqué](../troubleshooting/un-agent-est-bloque.md).
- **Toutes les exécutions sont en SKIPPED** : la précédente n'a jamais terminé, arrêtez l'agent puis relancez le trigger.

> **Référence technique :** [Ops-Exploitation-et-Debug](https://github.com/nidal-z/apollia-os/wiki/Ops-Exploitation-et-Debug) — interprétation des statuts et dépannage des échecs.
