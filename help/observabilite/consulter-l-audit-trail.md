# Consulter l'audit trail

> Pour les operators qui veulent retrouver précisément qui a fait quoi, quand, et avec quelle approbation — typiquement pour un contrôle interne ou une enquête après incident.

## Prérequis
- Au moins une action sensible (écriture de fichier, commande, appel d'outil) a été exécutée.
- Vous savez approximativement la période ou l'agent à investiguer.

## Étapes

1. Dans la sidebar, cliquez sur **Observabilité**, puis sur l'onglet **Audit trail**.

2. Le tableau présente toutes les actions tracées, du plus récent au plus ancien. Cinq colonnes : **Date**, **Agent**, **Outil**, **Action**, **Approbation**.
   `[SCREENSHOT: onglet Audit trail, tableau avec colonnes et menus de filtre en haut]`

3. Affinez la recherche avec les filtres en haut du tableau :
   - Par **agent** pour isoler le travail d'un seul agent.
   - Par **outil** pour cibler un type d'action (lecture fichier, exécution commande, appel MCP).
   - Par **approbation** : approuvée, refusée, ou automatique (autorisée par une règle de permission).

4. Cliquez sur une ligne pour voir le détail complet : arguments envoyés à l'outil, résultat produit, durée, personne ayant approuvé, raison d'un éventuel refus.
   `[SCREENSHOT: panneau de détail d'une action audit, avec arguments, sortie et statut d'approbation]`

5. En bas du tableau, un compteur récapitule la période affichée : nombre total d'actions, approuvées, refusées, automatiques.

6. Pour exporter l'historique, cliquez sur **Exporter** en haut à droite. Choisissez **CSV** pour un tableur ou **JSON** pour un système d'archivage.

## Vérification
Vous retrouvez dans le tableau les actions que vous savez avoir validées récemment, avec leur statut correct.

## Si ça ne marche pas
- **Une action attendue ne figure pas** : vérifiez le filtre par agent ou par date — un filtre actif peut masquer la ligne.
- **Le détail d'une ligne est vide** : il s'agit probablement d'une action ancienne dont les arguments ont été purgés. Vérifiez la rétention configurée dans **Settings → Données**.
- **L'export échoue** : un export très volumineux peut dépasser quelques minutes. Restreignez la période avec un filtre de date avant de réessayer.

> **Référence technique :** [Securite-Audit-Trail](https://github.com/nidal-z/apollia-os/wiki/Securite-Audit-Trail)
