# Consulter l'audit trail

> Pour les operators qui veulent retrouver précisément qui a fait quoi, quand — typiquement pour un contrôle interne ou une enquête après incident.

## Prérequis
- Au moins une action sensible (écriture de fichier, commande, appel d'outil) a été exécutée.
- Vous savez approximativement la période ou l'agent à investiguer.

## Étapes

1. Dans la sidebar, cliquez sur **Observabilité**, puis sur l'onglet **Piste d'audit**.

2. Le tableau présente toutes les actions tracées, du plus récent au plus ancien. Cinq colonnes : **Horodatage**, **Outil**, **Agent**, **Durée**, **Sortie**.
   `[SCREENSHOT: onglet Audit trail, tableau avec colonnes et menus de filtre en haut]`

3. Affinez la recherche avec les filtres en haut du tableau :
   - Par **outil** pour cibler un type d'action (lecture fichier, exécution commande, appel MCP).
   - Par **agent** pour isoler le travail d'un seul agent.

4. Cliquez sur une ligne pour voir le détail complet : arguments envoyés à l'outil (JSON), sortie stdout/stderr, durée d'exécution et code de sortie.
   `[SCREENSHOT: ligne développée avec arguments JSON, sortie et durée]`

5. En bas du tableau, un bouton **Charger plus** permet d'afficher les entrées suivantes (50 par lot).

   > **⚠️ Non disponible dans cette version :** l'export de l'audit trail n'est pas encore disponible dans l'interface ni via la CLI. Les commandes CLI disponibles sont `apollia-os audit list` et `apollia-os audit stats`.

## Vérification
Vous retrouvez dans le tableau les actions que vous savez avoir validées récemment, avec leur statut correct.

## Si ça ne marche pas
- **Une action attendue ne figure pas** : vérifiez les filtres par outil ou par agent — un filtre actif peut masquer la ligne.
- **Le détail d'une ligne est vide** : il s'agit probablement d'une action ancienne dont les arguments ont été purgés.
- **Le tableau est vide** : aucune action outillée n'a encore été exécutée. Lancez un agent sur une tâche utilisant des outils (bash, file_write, etc.).

> **Référence technique :** [Securite-Audit-Trail](https://github.com/nidal-z/apollia-os/wiki/Securite-Audit-Trail)
