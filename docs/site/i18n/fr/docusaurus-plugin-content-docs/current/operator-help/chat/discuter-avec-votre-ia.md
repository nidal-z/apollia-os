---
title: Discuter avec votre IA
sidebar_position: 1
---

# Discuter avec votre IA

> Pour tout operator qui veut commencer à dialoguer avec son IA : ouvrir une conversation, envoyer des messages et obtenir des réponses adaptées au contexte.

## Prérequis

- Un fournisseur d'IA est connecté (pastille verte dans le bandeau supérieur).
- (Optionnel) Un projet est lié pour injecter automatiquement votre contexte de travail.
- (Optionnel) Un agent Assistant est démarré si vous voulez discuter avec un agent spécialisé.

## Étapes

1. Dans la sidebar, cliquez sur **Chat**. La liste de vos conversations s'affiche à gauche, la zone de saisie au centre.
   ![page Chat, sidebar conversations à gauche, zone vide au centre avec champ de saisie en bas](/img/operator-help/chat-discuter-avec-votre-ia-1.png)

2. Cliquez sur **Nouveau chat** en haut de la liste. Une conversation vierge s'ouvre.

3. (Optionnel) Pour choisir le **fournisseur d'IA** et le **mode** (Libre ou Agent spécifique), cliquez sur le bouton de configuration en haut de la conversation. Un panneau s'ouvre avec les options disponibles.

4. Tapez votre instruction en langage naturel dans le champ de saisie en bas. Soyez précis : *"Résume ce fichier en 5 points"* est plus efficace que *"Aide-moi"*.

5. Appuyez sur **Entrée** ou cliquez sur **Envoyer**. La réponse s'affiche en streaming, mot après mot.
   ![conversation avec un message utilisateur et une réponse IA en cours de streaming, formatage markdown rendu](/img/operator-help/chat-discuter-avec-votre-ia-2.png)

![conversation avec un message utilisateur et une réponse IA en cours de streaming, formatage markdown rendu (suite)](/img/operator-help/chat-discuter-avec-votre-ia-2bis.png)

6. Posez vos questions de suivi dans le même fil. L'IA conserve tout l'historique de la conversation.

<!-- claim:chat-timeline-follows-execution-order -->
7. Au-dessus de la réponse, une ligne de résumé indique combien le tour a réfléchi et combien d'outils il a utilisés. En dessous, le tour se lit **dans l'ordre où il s'est produit** : une réflexion, puis l'action qu'elle a déclenchée, puis la réflexion suivante, et ainsi de suite. Chaque ligne est repliée et se déplie sur son détail, le raisonnement tel qu'il a été écrit, un appel d'outil sous forme de compte rendu en langage clair en mode Opérateur ou d'entrée et sortie brutes en mode Builder.
   ![bulle de réponse avec la ligne de résumé et la chronologie ordonnée des réflexions et des appels d'outils](/img/operator-help/chat-discuter-avec-votre-ia-3.png)

<!-- claim:failed-tool-call-is-marked-failed -->
8. Un appel d'outil qui échoue est signalé comme tel, par une croix rouge et non par une coche verte, et le reste quand vous rouvrez la conversation plus tard. Un appel que vous avez refusé s'affiche comme refusé, ce qui n'est pas la même chose qu'un appel qui a tourné et échoué.

9. Si l'IA veut effectuer une action sensible (écrire un fichier, lancer une commande), une carte d'approbation apparaît : voir [Approuver ou refuser une action](../controle/approuver-ou-refuser-une-action.md).

10. Pour organiser vos conversations, cliquez sur le menu en haut de la conversation : **Renommer** ou **Supprimer**.

<!-- claim:context-gauge-engine-usage -->
11. Sous le champ de saisie, une petite jauge **Ctx** suit le remplissage de la fenêtre de contexte du modèle. Le pourcentage vient des **comptes de tokens rapportés par le moteur lui-même** à chaque réponse ; quand le backend n'en rapporte aucun (un assistant en mode Agent, par exemple), la jauge affiche `--` plutôt qu'un nombre inventé. Au-delà de 90 %, la jauge passe à l'ambre : envisagez une nouvelle conversation, ou laissez la compaction automatique résumer les tours les plus anciens.

## Vérification

Votre conversation apparaît dans la liste de gauche avec un titre et la date du dernier message. Vous pouvez la rouvrir à tout moment, l'historique est conservé.

## Si ça ne marche pas

- **Pas de réponse :** vérifiez que la pastille du fournisseur est verte dans le bandeau supérieur.
- **Réponse en erreur ou tronquée :** changez de modèle dans le panneau de configuration, ou consultez [Le fournisseur d'IA ne répond pas](../troubleshooting/le-fournisseur-d-ia-ne-repond-pas.md).
- **L'IA ne connaît pas vos fichiers :** liez la conversation à un projet et activez les context providers.

> **Concept :** [Explication Apollia](/explanation) - comprendre comment le contexte est injecté et comment les modes Libre et Agent diffèrent.
