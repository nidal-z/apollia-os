# Naviguer au clavier (command palette)

> Pour les operators qui veulent ouvrir n'importe quelle page, lancer une action ou retrouver un agent en quelques touches, sans passer par la souris.

## Prérequis
- Aucun. La command palette est disponible partout dans l'application.

## Étapes

1. Depuis n'importe quel écran, appuyez sur **Cmd+K** (Mac) ou **Ctrl+K** (Windows et Linux). La command palette s'ouvre au centre de l'écran.
   `[SCREENSHOT: command palette ouverte, champ de recherche en haut, liste de suggestions en dessous]`

2. Tapez quelques lettres de l'action ou de la page recherchée. La liste se filtre en temps réel : *agent* fait apparaître **Installer un agent**, **Démarrer un agent**, **Voir les logs**.

3. Utilisez les flèches **Haut** et **Bas** pour parcourir les résultats. La ligne sélectionnée est mise en évidence.

4. Appuyez sur **Entrée** pour exécuter l'action ou ouvrir la page correspondante. La palette se ferme automatiquement.

5. Appuyez sur **Échap** à tout moment pour refermer la palette sans rien faire.

6. Pour personnaliser le raccourci d'ouverture ou ajouter d'autres raccourcis, allez dans **Paramètres → Raccourcis**. Cliquez sur la ligne à modifier, puis appuyez sur la nouvelle combinaison de touches.
   `[SCREENSHOT: page Raccourcis avec liste de raccourcis et bouton de capture pour chacun]`

## Vérification
La palette s'ouvre instantanément avec le raccourci, et l'action choisie s'exécute correctement après pression sur **Entrée**.

## Si ça ne marche pas
- **Le raccourci ne fait rien** : un autre logiciel intercepte peut-être **Cmd+K** ou **Ctrl+K**. Choisissez une autre combinaison dans **Paramètres → Raccourcis**.
- **L'action recherchée n'apparaît pas** : la palette ne contient que les actions disponibles dans le contexte courant. Certaines actions n'apparaissent que si vous êtes sur la bonne page (par exemple **Démarrer un agent** depuis la page Agents).
- **La nouvelle combinaison personnalisée n'est pas enregistrée** : vérifiez qu'elle n'entre pas en conflit avec un raccourci existant. La page **Raccourcis** signale les doublons en orange.
