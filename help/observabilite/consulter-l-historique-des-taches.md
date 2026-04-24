# Consulter l'historique des tâches

> Pour les operators qui veulent retrouver une exécution passée, comprendre comment elle s'est déroulée et identifier les ralentissements ou les échecs récurrents.

## Prérequis
- Au moins une tâche, un trigger ou un pipeline s'est exécuté récemment.

## Étapes

1. Dans la sidebar, cliquez sur **Observabilité**, puis sur l'onglet **Timeline**.

2. La timeline liste chaque événement chronologiquement : tâche démarrée, outil appelé, approbation reçue, tâche terminée.
   `[SCREENSHOT: onglet Timeline, liste d'événements avec icônes, durées et statuts]`

3. Utilisez les filtres en haut pour resserrer la recherche :
   - Par **agent** pour ne voir qu'un seul exécutant.
   - Par **type** : tâche, outil, approbation, déclenchement.
   - Par **statut** : terminée, en cours, échouée.

4. Cliquez sur une ligne pour ouvrir le détail. Vous voyez la durée totale, les étapes internes, les outils utilisés et le résultat final.
   `[SCREENSHOT: panneau de détail d'une tâche, étapes en arborescence, durée par étape]`

5. Pour suivre une exécution en cours en temps réel, repérez la ligne avec le statut **En cours** (icône animée). La page se met à jour automatiquement.

6. Pour creuser un échec, cliquez sur la ligne en rouge, puis sur **Voir les logs**. La page **Agents → Logs** s'ouvre déjà filtrée sur la bonne exécution.

## Vérification
Vous retrouvez l'exécution recherchée et son détail correspond à ce que vous attendiez.

## Si ça ne marche pas
- **La timeline est vide** : aucune exécution récente n'a eu lieu. Démarrez un agent ou un trigger pour générer une ligne.
- **Une exécution attendue n'apparaît pas** : vérifiez les filtres actifs. Un filtre par agent ou par statut masque souvent la ligne cherchée.
- **Le détail d'une étape ne s'ouvre pas** : la donnée a peut-être été purgée par la rétention. Vérifiez la durée de conservation dans **Settings → Données**.

> **Concept :** [book ch10 — Observer un agent](https://github.com/nidal-z/apollia-os/blob/main/book/src/ch10-00-observer-un-agent.md)
