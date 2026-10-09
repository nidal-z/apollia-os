---
title: Naviguer au clavier (command palette)
slug: /operator-help/transversal/navigate-with-the-keyboard
sidebar_position: 3
---

# Naviguer au clavier (command palette)

> Pour les operators qui veulent ouvrir n'importe quelle page, lancer une action ou retrouver un agent en quelques touches, sans passer par la souris.

## Prérequis

- Aucun. La command palette est disponible partout dans l'application.

## Étapes

1. Depuis n'importe quel écran, appuyez sur **Cmd+K** (macOS) ou **Ctrl+K** (Windows et Linux). La palette s'ouvre au centre de l'écran.

   ![command palette ouverte au centre de l'écran, champ de recherche en haut, suggestions groupées en dessous](/img/operator-help/transversal-naviguer-au-clavier-command-palette-1.png)

   > **Note :** il n'y a qu'un seul raccourci d'ouverture. Cmd+K (ou Ctrl+K) couvre tous les cas - page, action, raccourci. Tapez quelques lettres pour réduire la liste.

2. Tapez quelques lettres de ce que vous cherchez. La liste se filtre en temps réel. Exemple en tapant **agent** : *Agents* (page) et *Créer un agent*.

   Les entrées sont **groupées par catégorie** (Récemment utilisés, Pages, Actions, Sessions, Paramètres, Aide) - les sections changent selon la page courante.

3. Utilisez les flèches **Haut** et **Bas** pour parcourir les résultats. La ligne sélectionnée est mise en évidence.

4. Appuyez sur **Entrée** pour exécuter l'action ou ouvrir la page correspondante. La palette se ferme automatiquement.

5. Appuyez sur **Échap** à tout moment pour la refermer sans rien faire. Un second **Cmd+K** la rouvre vide.

## Cheatsheet rapide

Pour voir d'un coup d'œil les raccourcis disponibles dans le contexte courant, appuyez sur **?** (ou **Shift+/**) sans aucun modificateur. Un overlay condensé apparaît avec les combinaisons utiles à l'écran où vous êtes. Re-pressez **Échap** pour le fermer.

## Liste complète des raccourcis

Pour parcourir tous les raccourcis groupés par catégorie (Global, Navigation, Chat, Réglages, Companion), ouvrez **Réglages**, section **Chat**, et descendez jusqu'au bloc **Tous les raccourcis**. Il propose une **recherche** pour retrouver une combinaison par nom (*« barre latérale »*, *« nouveau chat »*, *« aide »*…). La même liste s'ouvre depuis la palette avec **Ouvrir la page Raccourcis**.

> **Lecture seule.** Cette liste n'autorise pas la personnalisation des combinaisons. Seule exception : la touche qui envoie un message, qui a son propre interrupteur dans le bloc **Raccourcis clavier** de la même section.

## Quelques raccourcis utiles

- **Cmd+K / Ctrl+K** - ouvrir la command palette.
- **?** - afficher la cheatsheet contextuelle.
- **Cmd+B / Ctrl+B** - replier / déplier la sidebar.
- **Cmd+[ / Ctrl+[** - page précédente (navigation historique).
- **Cmd+] / Ctrl+]** - page suivante.
- **Cmd+/ / Ctrl+/** - ouvrir ou replier l'onglet **Guide** sur l'écran courant.
- **Entrée** - envoyer le message courant (dans un chat) ; **Maj+Entrée** passe à la ligne. Un interrupteur dans **Réglages**, section **Chat**, fait envoyer **Cmd+Entrée / Ctrl+Entrée** à la place.
- **Échap** - fermer le dialog / chat-input courant.

## Vérification

La palette s'ouvre instantanément avec **Cmd+K** / **Ctrl+K**, et l'action choisie s'exécute correctement après pression sur **Entrée**.

## Si ça ne marche pas

- **Cmd+K n'ouvre rien** : un autre logiciel (Slack, Notion, navigateur…) intercepte peut-être la combinaison en premier. Cliquez d'abord dans la fenêtre d'Apollia pour qu'elle reçoive le focus, puis ré-essayez. Si le problème persiste, l'autre logiciel doit être quitté ou son raccourci changé.
- **L'action recherchée n'apparaît pas** : certaines entrées sont **contextuelles**. Par exemple, *Sessions récentes* / *Modèles de chat* / *Slash commands* n'apparaissent que si vous êtes sur la page **Chat**. Naviguez d'abord vers la page correspondante puis rouvrez la palette.
- **Je veux personnaliser un raccourci** : la liste **Tous les raccourcis** de **Réglages**, section **Chat**, est en lecture seule. Seule la touche qui envoie un message se change, dans la même section.
