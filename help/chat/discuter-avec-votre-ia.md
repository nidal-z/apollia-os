# Discuter avec votre IA

> Pour tout operator qui veut commencer à dialoguer avec son IA : ouvrir une conversation, envoyer des messages et obtenir des réponses adaptées au contexte.

## Prérequis

- Un fournisseur d'IA est connecté (pastille verte dans le bandeau supérieur).
- (Optionnel) Un projet est lié pour injecter automatiquement votre contexte de travail.
- (Optionnel) Un agent Assistant est démarré si vous voulez discuter avec un agent spécialisé.

## Étapes

1. Dans la sidebar, cliquez sur **Chat**. La liste de vos conversations s'affiche à gauche, la zone de saisie au centre.
   `[SCREENSHOT: page Chat, sidebar conversations à gauche, zone vide au centre avec champ de saisie en bas]`

2. Cliquez sur **+ Nouvelle conversation** en haut de la liste. Une conversation vierge s'ouvre.

3. (Optionnel) En haut de la conversation, sélectionnez le **fournisseur d'IA** et le **mode** (Libre, ou un Assistant spécifique parmi vos agents démarrés).

4. Tapez votre instruction en langage naturel dans le champ de saisie en bas. Soyez précis : *"Résume ce fichier en 5 points"* est plus efficace que *"Aide-moi"*.

5. Appuyez sur **Entrée** ou cliquez sur **Envoyer**. La réponse s'affiche en streaming, mot après mot.
   `[SCREENSHOT: conversation avec un message utilisateur et une réponse IA en cours de streaming, formatage markdown rendu]`

6. Posez vos questions de suivi dans le même fil. L'IA conserve tout l'historique de la conversation.

7. Si vous discutez avec un Assistant, un volet **Raisonnement** à droite affiche les étapes que l'agent suit pour répondre.
   `[SCREENSHOT: conversation avec Assistant, volet droit "Raisonnement" déplié montrant 3 étapes numérotées]`

8. Si l'IA veut effectuer une action sensible (écrire un fichier, lancer une commande), une carte d'approbation apparaît : voir [Approuver ou refuser une action](../controle/approuver-ou-refuser-une-action.md).

9. Pour organiser vos conversations, cliquez sur le menu en haut de la conversation : **Renommer**, **Lier à un projet**, ou **Archiver**.

## Vérification

Votre conversation apparaît dans la liste de gauche avec un titre et la date du dernier message. Vous pouvez la rouvrir à tout moment, l'historique est conservé.

## Si ça ne marche pas

- **Pas de réponse :** vérifiez que la pastille du fournisseur est verte dans le bandeau supérieur.
- **Réponse en erreur ou tronquée :** changez de modèle dans le sélecteur en haut de la conversation, ou consultez [Le fournisseur d'IA ne répond pas](../troubleshooting/le-fournisseur-d-ia-ne-repond-pas.md).
- **L'IA ne connaît pas vos fichiers :** liez la conversation à un projet et activez les context providers.

> **Concept :** [book ch12 — Chat interactif](https://github.com/nidal-z/apollia-os/blob/main/book/src/ch12-00-chat-interactif.md) — comprendre comment le contexte est injecté et comment les modes Libre et Agent diffèrent.
