---
title: Discuter avec votre IA
slug: /operator-help/chat/chat-with-your-ai
sidebar_position: 1
---

# Discuter avec votre IA

> Pour tout operator qui veut commencer à dialoguer avec son IA : ouvrir une conversation, envoyer des messages et obtenir des réponses adaptées au contexte.

## Prérequis

- Un modèle répond : un modèle installé sur ce poste ou servi par un serveur, réglé dans l'écran **IA**.
- (Optionnel) Un agent est installé si vous voulez discuter avec un agent spécialisé.

## Étapes

1. Dans la barre latérale, cliquez sur **Chat**. La liste de vos conversations s'affiche à gauche, la zone de saisie au centre.
   ![page Chat, sidebar conversations à gauche, zone vide au centre avec champ de saisie en bas](/img/operator-help/chat-discuter-avec-votre-ia-1.png)

2. Cliquez sur **Nouveau chat** en haut de la liste. Une conversation vierge s'ouvre.

3. (Optionnel) Pour choisir qui répond, cliquez sur le sélecteur en haut de la conversation. Une liste s'ouvre, avec vos agents d'abord, puis vos modèles. Un agent choisi ouvre une nouvelle conversation avec lui ; un modèle choisi devient celui de la conversation en cours.

4. Tapez votre instruction en langage naturel dans le champ de saisie en bas. Soyez précis : *"Résume ce fichier en 5 points"* est plus efficace que *"Aide-moi"*.

5. Appuyez sur **Entrée** ou cliquez sur **Envoyer**. La réponse s'affiche en streaming, mot après mot.
   ![conversation avec un message utilisateur et une réponse IA en cours de streaming, formatage markdown rendu](/img/operator-help/chat-discuter-avec-votre-ia-2.png)

![conversation avec un message utilisateur et une réponse IA en cours de streaming, formatage markdown rendu (suite)](/img/operator-help/chat-discuter-avec-votre-ia-2bis.png)

6. Posez vos questions de suivi dans le même fil. L'IA conserve tout l'historique de la conversation.

<!-- claim:chat-timeline-follows-execution-order -->
7. Au-dessus de la réponse, une ligne de résumé indique combien le tour a réfléchi et combien d'outils il a utilisés. En dessous, le tour se lit **dans l'ordre où il s'est produit** : une réflexion, puis l'action qu'elle a déclenchée, puis la réflexion suivante, et ainsi de suite. Chaque ligne est repliée et se déplie sur son détail, le raisonnement tel qu'il a été écrit, un appel d'outil sous forme de compte rendu en langage clair. Mode développeur activé, les données brutes de l'étape s'ajoutent sous ce compte rendu.
   ![bulle de réponse avec la ligne de résumé et la chronologie ordonnée des réflexions et des appels d'outils](/img/operator-help/chat-discuter-avec-votre-ia-3.png)

<!-- claim:failed-tool-call-is-marked-failed -->
8. Un appel d'outil qui échoue est signalé comme tel, par une croix rouge et non par une coche verte, et le reste quand vous rouvrez la conversation plus tard. Un appel que vous avez refusé s'affiche comme refusé, ce qui n'est pas la même chose qu'un appel qui a tourné et échoué.

9. Si l'IA veut effectuer une action sensible (écrire un fichier, lancer une commande), elle demande votre accord dans la conversation : voir [Approuver ou refuser une action](../controle/approve-or-reject-an-action.md).

10. Pour organiser vos conversations, cliquez sur le menu en haut de la conversation : **Renommer** ou **Supprimer**.

<!-- claim:context-gauge-engine-usage -->
11. À droite du champ de saisie, un anneau accompagné d'un pourcentage suit le remplissage de la fenêtre de contexte du modèle. Survolez-le pour lire le détail en jetons. Le pourcentage vient des **comptes de jetons rapportés par le moteur lui-même** à chaque réponse ; quand le moteur n'en rapporte aucun, la jauge affiche `--` plutôt qu'un nombre inventé. Au-delà de 90 %, la jauge passe à l'ambre : envisagez une nouvelle conversation. Par défaut, les échanges les plus anciens sont résumés quand la conversation atteint la limite du modèle.

## Vérification

Votre conversation apparaît dans la liste de gauche avec un titre et la date du dernier message. Vous pouvez la rouvrir à tout moment, l'historique est conservé.

## Si ça ne marche pas

- **Pas de réponse :** vérifiez dans l'écran **IA** qu'un modèle est installé ou que votre serveur répond.
- **Réponse en erreur ou tronquée :** choisissez un autre modèle dans le sélecteur de la conversation, ou consultez [L'IA ne répond pas](../troubleshooting/the-ai-provider-does-not-answer.md).
- **L'IA ne connaît pas vos fichiers :** vérifiez les sources de l'agent avec qui vous parlez, et accordez-lui le dossier qui les contient dans **Connexions**.

> **Concept :** [Explication Apollia](/explanation) - comprendre en quoi une conversation avec un agent diffère d'une conversation avec un modèle seul.
