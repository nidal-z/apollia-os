# Approuver une étape HITL d'un pipeline

> Pour les operators qui doivent valider une étape critique d'un pipeline en cours d'exécution avant que la suite se déclenche.

## Prérequis

- Un pipeline lancé et en cours d'exécution.
- Une étape du pipeline est définie comme nécessitant une validation humaine (HITL).
- Vous comprenez ce que l'étape est censée produire.

## Étapes

1. Dans la sidebar, cliquez sur **Pipelines**, puis sur l'onglet **Runs**.

2. Repérez le run en cours et cliquez dessus pour ouvrir le graphe d'exécution. L'étape en attente d'approbation est entourée en orange et porte la mention **En attente de validation**.
   `[SCREENSHOT: graphe DAG, une étape encadrée en orange avec badge "En attente de validation", carte d'approbation au-dessus du graphe]`

3. Lisez le **titre de l'action** affiché en haut de la carte (par exemple : *L'agent veut écrire dans ~/Rapports/digest-2026-W17.md*).

4. Vérifiez le **type d'action** indiqué : écriture de fichier, exécution de commande, ou appel d'un outil externe.

5. Consultez l'**aperçu du contenu** sous le titre. Pour un fichier, Apollia affiche les premières lignes ; pour une commande, la commande complète ; pour un outil, les paramètres d'appel.
   `[SCREENSHOT: ApprovalCard avec titre, type "Écriture fichier", chemin, et bloc de preview affichant les 10 premières lignes du contenu]`

6. Choisissez votre décision :
   - **Approuver** — l'étape s'exécute, le pipeline reprend immédiatement à l'étape suivante.
   - **Refuser** — l'étape est bloquée, l'agent est notifié, et le pipeline s'adapte (étape de fallback ou arrêt).
   - **Toujours autoriser** — case facultative qui crée une règle de permission durable pour les actions équivalentes.

7. Si vous cochez **Toujours autoriser**, précisez le périmètre (cet agent uniquement ou tous, ce dossier ou ses sous-dossiers).

8. Cliquez sur **Approuver** ou **Refuser**. Le graphe se met à jour : l'étape passe au vert (ou au rouge si refusée) et l'étape suivante démarre.

## Vérification

L'étape qui était en orange passe au vert dans le graphe. L'étape suivante démarre automatiquement et passe au bleu. La carte d'approbation disparaît et bascule dans l'historique de l'**Inbox**.

## Si ça ne marche pas

- **La carte d'approbation n'apparaît pas alors que l'étape est en attente** : ouvrez l'**Inbox** dans la sidebar. Les approbations en arrière-plan y sont centralisées.
- **Pipeline en erreur après refus** : c'est le comportement attendu si le pipeline n'a pas de fallback défini. Consultez les logs de l'étape refusée pour comprendre.
- **Le bouton Approuver est grisé** : votre fournisseur d'IA est déconnecté. Reconnectez-le depuis le bandeau supérieur, puis revenez à la carte.

> **Concept :** [book ch10 — HITL (Human-In-The-Loop)](https://github.com/nidal-z/apollia-os/blob/main/book/src/ch10-00-hitl.md)
