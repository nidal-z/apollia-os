# Naviguer au clavier (command palette)

> Pour les operators qui veulent ouvrir n'importe quelle page, lancer une action ou retrouver un agent en quelques touches, sans passer par la souris.

## Prérequis

- Aucun. La command palette est disponible partout dans l'application.

## Étapes

1. Depuis n'importe quel écran, appuyez sur **Cmd+K** (macOS) ou **Ctrl+K** (Windows et Linux). La palette s'ouvre au centre de l'écran.

   `[SCREENSHOT: command palette ouverte au centre de l'écran, champ de recherche en haut, suggestions groupées en dessous]`

   > **Note :** il n'y a qu'un seul raccourci d'ouverture. Cmd+K (ou Ctrl+K) couvre tous les cas — page, action, raccourci. Tapez quelques lettres pour réduire la liste.

2. Tapez quelques lettres de ce que vous cherchez. La liste se filtre en temps réel. Exemple en tapant **agent** : *Agents* (page), *Créer un agent*, *Démarrer tous les agents*, *Arrêter tous les agents*.

   Les entrées sont **groupées par catégorie** (Pages, Actions, Sessions récentes, Modèles de chat, Slash commands…) — les sections changent selon la page courante.

3. Utilisez les flèches **Haut** et **Bas** pour parcourir les résultats. La ligne sélectionnée est mise en évidence.

4. Appuyez sur **Entrée** pour exécuter l'action ou ouvrir la page correspondante. La palette se ferme automatiquement.

5. Appuyez sur **Échap** à tout moment pour la refermer sans rien faire. Un second **Cmd+K** la rouvre vide.

## Cheatsheet rapide

Pour voir d'un coup d'œil les raccourcis disponibles dans le contexte courant, appuyez sur **?** (ou **Shift+/**) sans aucun modificateur. Un overlay condensé apparaît avec les combinaisons utiles à l'écran où vous êtes. Re-pressez **Échap** pour le fermer.

## Liste complète des raccourcis

Pour parcourir tous les raccourcis (globaux, navigation, chat, paramètres, companion, approbations) groupés par catégorie, allez sur **Paramètres → Raccourcis**. La page propose une **recherche** pour retrouver une combinaison par nom (*« sidebar »*, *« nouveau chat »*, *« aide »*…).

`[SCREENSHOT: page Paramètres → Raccourcis, barre de recherche en haut, raccourcis groupés par catégorie (Global / Navigation / Chat / …), chaque ligne affichant la combinaison clavier sous forme de touches stylisées]`

> **Lecture seule.** Cette page n'autorise pas la personnalisation des combinaisons pour le moment. Les raccourcis sont fixés et alignés sur les conventions natives de macOS / Windows.

## Quelques raccourcis utiles

- **Cmd+K / Ctrl+K** — ouvrir la command palette.
- **?** — afficher la cheatsheet contextuelle.
- **Cmd+B / Ctrl+B** — replier / déplier la sidebar.
- **Cmd+[ / Ctrl+[** — page précédente (navigation historique).
- **Cmd+] / Ctrl+]** — page suivante.
- **Cmd+/ / Ctrl+/** — basculer l'affichage de l'**Aide Apollia** (le Companion).
- **Cmd+Enter / Ctrl+Enter** — envoyer le message courant (dans un chat).
- **Échap** — fermer le dialog / chat-input courant.

## Vérification

La palette s'ouvre instantanément avec **Cmd+K** / **Ctrl+K**, et l'action choisie s'exécute correctement après pression sur **Entrée**.

## Si ça ne marche pas

- **Cmd+K n'ouvre rien** : un autre logiciel (Slack, Notion, navigateur…) intercepte peut-être la combinaison en premier. Cliquez d'abord dans la fenêtre d'Apollia pour qu'elle reçoive le focus, puis ré-essayez. Si le problème persiste, l'autre logiciel doit être quitté ou son raccourci changé.
- **L'action recherchée n'apparaît pas** : certaines entrées sont **contextuelles**. Par exemple, *Sessions récentes* / *Modèles de chat* / *Slash commands* n'apparaissent que si vous êtes sur la page **Chat**. Naviguez d'abord vers la page correspondante puis rouvrez la palette.
- **Je veux personnaliser un raccourci** : non disponible pour le moment — la page **Paramètres → Raccourcis** est en lecture seule. Les conventions natives (Cmd / Ctrl) sont respectées partout.
