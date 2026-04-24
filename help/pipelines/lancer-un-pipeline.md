# Lancer un pipeline

> Pour les operators qui veulent exécuter un workflow multi-étapes déjà défini et suivre sa progression de bout en bout.

## Prérequis

- Au moins un pipeline défini (créé depuis l'interface ou importé).
- Tous les agents impliqués dans les étapes sont démarrés (statut vert).
- Un fournisseur d'IA connecté (la connexion est verte dans le bandeau supérieur).

## Étapes

1. Dans la sidebar, cliquez sur **Pipelines**.

2. Restez sur l'onglet **Définitions**. Vous voyez la liste de tous vos pipelines avec leur nom, leur description et le nombre d'étapes.
   `[SCREENSHOT: page Pipelines, onglet Définitions actif, liste de pipelines avec colonnes Nom, Étapes, Dernière exécution]`

3. Repérez le pipeline à exécuter et cliquez sur **Lancer** sur sa ligne.

4. Si le pipeline attend des paramètres d'entrée, un modal s'ouvre. Renseignez les champs demandés (par exemple : nom du client, contenu du document, URL cible). Les champs obligatoires sont marqués d'un astérisque.
   `[SCREENSHOT: modal "Lancer le pipeline", trois champs d'entrée avec labels, bouton Lancer en bas à droite]`

5. Cliquez sur **Lancer**. Apollia bascule automatiquement sur l'onglet **Runs** et affiche votre exécution en cours.

6. Cliquez sur le run en haut de la liste pour ouvrir le **graphe d'exécution**. Chaque étape est une boîte ; les flèches montrent les dépendances.
   `[SCREENSHOT: vue DAG d'un pipeline, 5 boîtes avec couleurs différentes (gris, bleu, vert), flèches orientées entre elles]`

7. Suivez l'avancement en temps réel grâce aux couleurs des étapes :
   - **Gris** — en attente.
   - **Bleu** — en cours d'exécution.
   - **Vert** — terminée avec succès.
   - **Rouge** — en échec.

8. Cliquez sur n'importe quelle étape pour voir l'agent appelé, l'entrée reçue et la sortie produite.

9. Si une étape exige une approbation humaine, une carte d'approbation apparaît au-dessus du graphe. Voyez la page *Approuver une étape HITL* pour la suite.

10. Quand toutes les étapes sont vertes, cliquez sur **Voir le résultat final** pour consulter la sortie consolidée, ou sur **Archiver le run** pour le ranger dans l'historique.

## Vérification

Le run apparaît dans l'onglet **Runs** avec le statut **Terminé** et une durée totale. Le résultat final est accessible en un clic depuis la dernière étape verte.

## Si ça ne marche pas

- **Étape rouge dès le démarrage** : l'agent associé n'est pas démarré. Allez dans **Agents**, démarrez-le, puis relancez le pipeline.
- **Modal de paramètres avec un message rouge** : un champ obligatoire est vide ou mal formaté (par exemple, une URL sans `https://`).
- **Pipeline bloqué en bleu sans avancer** : une étape attend une approbation. Vérifiez l'**Inbox** pour voir s'il y a une carte d'approbation en attente.

> **Concept :** [book ch13 — Pipelines](https://github.com/nidal-z/apollia-os/blob/main/book/src/ch13-00-pipelines.md)
