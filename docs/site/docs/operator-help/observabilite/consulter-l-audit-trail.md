# Consulter l'audit trail

> Pour les operators qui veulent retrouver précisément qui a fait quoi, quand - typiquement pour un contrôle interne ou une enquête après incident.

## Prérequis

- Au moins une action sensible (écriture de fichier, commande, appel d'outil) a été exécutée.
- Vous savez approximativement la période ou l'agent à investiguer.

## Étapes

1. Dans la sidebar, cliquez sur **Observabilité**, puis sur l'onglet **Piste d'audit**.

2. En haut de l'onglet, un **encart violet** rappelle l'utilité de la piste d'audit : contrôle interne, enquête après incident, conformité, vérification du périmètre d'action d'un agent. C'est la trace immuable de chaque outil invoqué par un agent.

3. Juste en dessous, **quatre indicateurs clés** (KPI) se mettent à jour selon les filtres : **Entrées affichées**, **Outils distincts**, **Échecs** (en rouge si > 0), **Durée moyenne**.
   ![onglet Piste d'audit - bannière de purpose en haut, 4 KPI, filtres, puis tableau](/img/operator-help/observabilite-consulter-l-audit-trail-1.png)

4. Le tableau présente toutes les invocations d'outils tracées, du plus récent au plus ancien. **Cinq colonnes** :
   - **Horodatage** (date + heure locale)
   - **Outil** (nom technique en monospace : `bash`, `file_write`, `mcp:notion.search`, etc.)
   - **Agent** (nom lisible ; à défaut l'identifiant brut si l'agent n'est plus enregistré)
   - **Durée** (`850ms` ou `2.1s`, ou `-` si non mesuré)
   - **Statut** - badge **Succès** (vert, ✓) ou **Échec** (rouge, ✕). Le statut est déduit du code de sortie *et* de la présence de stderr ; un outil MCP sans code de sortie est considéré OK s'il s'est terminé sans erreur.

5. Affinez la recherche avec les deux sélecteurs au-dessus du tableau :
   - **Outil** - isole les invocations d'un outil précis (la liste se construit à partir des entrées chargées).
   - **Agent** - isole le travail d'un seul agent.

6. Cliquez sur une ligne pour déplier son détail. Selon ce qui a été capturé, trois sections peuvent apparaître :
   - **Arguments** - JSON envoyé à l'outil, formaté.
   - **stdout** - sortie standard.
   - **stderr** - sortie d'erreur, affichée en rouge.

   Si l'invocation n'a rien produit de capturable (outils MCP en lecture seule, outils sans I/O standard…), un message *« Aucun détail disponible »* s'affiche.
   ![ligne dépliée affichant les sections Arguments / stdout / stderr](/img/operator-help/observabilite-consulter-l-audit-trail-2.png)

7. En bas du tableau, le bouton **Charger plus** étend la liste de 50 entrées supplémentaires.

   > **L'export et la vérification se font en ligne de commande**, pas depuis
   > l'interface. Voir la section suivante.

## Exporter et vérifier en ligne de commande

L'interface montre le journal ; la ligne de commande permet de le sortir et de
prouver qu'il n'a pas été modifié.

```sh
apollia-os audit list --limit 200        # consulter
apollia-os audit stats                   # compter
apollia-os audit export --output audit.json --limit 100000
apollia-os audit verify                  # vérifier toute la chaîne
apollia-os audit verify <RUN_ID>         # vérifier une exécution
apollia-os audit anchor                  # imprimer l'ancre de tête
```

**`verify`** recalcule la chaîne de hachage et contrôle les signatures. Sans
argument, il parcourt le journal entier ; avec un identifiant d'exécution, il se
limite à celle-ci. C'est ce qui distingue un journal d'une simple liste : une
entrée modifiée après coup casse la chaîne et se voit.

**`anchor`** imprime l'ancre de tête de la chaîne globale. La conserver hors de
la machine est la seule défense contre une troncature de la fin du journal par
quelqu'un qui aurait obtenu la clé de signature. Cette clé est un fichier local,
lisible par le compte qui exécute Apollia : l'ancre exportée est donc la
protection réelle, pas une précaution avancée.

**`export`** écrit le journal en JSON. Il s'arrête à `--limit`, 10000 par défaut,
et prévient sur la sortie d'erreur quand il atteint ce plafond.

Détail de ces commandes dans
[Audit, verify and roll back a run](../../how-to/audit-verify-rollback.md).

## Vérification

Vous retrouvez dans le tableau les actions que vous savez avoir validées récemment, avec leur statut correct (Succès en vert, Échec en rouge). Les KPI en haut reflètent la sélection courante : si vous filtrez par agent, le compteur d'**Entrées affichées** baisse en conséquence.

## Si ça ne marche pas

- **Une action attendue ne figure pas** : vérifiez les filtres **Outil** et **Agent** - un filtre actif peut masquer la ligne. Cliquez sur **Charger plus** si la fenêtre par défaut (50 entrées) ne remonte pas assez loin.
- **Le détail d'une ligne ne montre rien** : l'invocation provient probablement d'un outil sans capture stdout/stderr (outil MCP, appel API interne). La ligne reste tracée mais ses entrées-sorties ne le sont pas.
- **Le tableau est vide** : aucune action outillée n'a encore été exécutée. Lancez un agent sur une tâche utilisant des outils (bash, file_write, etc.).

> **Référence technique :** [Référence Apollia](../../reference/index.md)
