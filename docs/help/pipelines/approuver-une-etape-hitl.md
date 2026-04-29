# Approuver une étape HITL d'un pipeline

> **Cette fonctionnalité n'est pas disponible dans la version actuelle.** Le moteur de pipelines déclaratifs a été retiré du runtime. Pour les approbations HITL d'agents autonomes, voir *Approuver ou refuser une action d'agent*.

<!--

> Pour les operators qui doivent valider une étape critique d'un pipeline en cours d'exécution avant que la suite se déclenche.

## Prérequis

- Un pipeline lancé et en cours d'exécution.
- Une étape du pipeline est définie comme nécessitant une validation humaine (HITL).
- Vous comprenez ce que l'étape est censée produire.

## Étapes

1. Dans la sidebar, cliquez sur **Pipelines**, puis sur l'onglet **Runs**.

2. Repérez le run en cours et cliquez dessus pour ouvrir le graphe d'exécution. L'étape en attente d'approbation apparaît en **orange** avec un lien **View approvals** en texte orange.
   `[SCREENSHOT: graphe DAG, une étape en orange avec lien "View approvals", carte d'approbation accessible depuis l'Inbox]`

   > **Note :** l'étape en attente ne porte pas de badge texte "En attente de validation" — elle est identifiable par sa couleur orange et le lien "View approvals" qui s'affiche dans son bloc.

3. Cliquez sur le lien **View approvals** ou ouvrez l'**Inbox** dans la sidebar pour trouver la carte d'approbation correspondante.

4. Lisez le **titre de l'action** affiché en haut de la carte (par exemple : *L'agent veut écrire dans ~/Rapports/digest-2026-W17.md*).

5. Vérifiez le **type d'action** indiqué : écriture de fichier, exécution de commande, ou appel d'un outil externe.

6. Consultez l'**aperçu du contenu** sous le titre. Le contenu est affiché dans un bloc avec défilement vertical (hauteur maximale fixe) — l'intégralité du contenu est accessible par défilement, pas uniquement les premières lignes.
   `[SCREENSHOT: ApprovalCard avec titre, type d'action, et bloc de preview avec scrollbar latérale]`

7. Choisissez votre décision :
   - **Approuver** — l'étape s'exécute, le pipeline reprend immédiatement à l'étape suivante.
   - **Refuser** — l'étape est bloquée, l'agent est notifié, et le pipeline s'adapte (étape de fallback ou arrêt).

8. Si vous souhaitez éviter de revalider des actions similaires, utilisez les options de périmètre disponibles sur la carte (voir *Approuver ou refuser une action d'agent*).

9. Cliquez sur **Approuver** ou **Refuser**. Le graphe se met à jour : l'étape passe au vert (ou au rouge si refusée) et l'étape suivante démarre.

## Vérification

L'étape qui était en orange passe au vert dans le graphe. L'étape suivante démarre automatiquement et passe au bleu. La carte d'approbation disparaît de l'Inbox.

## Si ça ne marche pas

- **La carte d'approbation n'apparaît pas alors que l'étape est en attente** : ouvrez l'**Inbox** dans la sidebar. Les approbations en arrière-plan y sont centralisées.
- **Pipeline en erreur après refus** : c'est le comportement attendu si le pipeline n'a pas de fallback défini. Consultez les logs de l'étape refusée pour comprendre.
- **Le bouton Approuver est grisé** : votre fournisseur d'IA est déconnecté. Reconnectez-le depuis le bandeau supérieur, puis revenez à la carte.

-->
