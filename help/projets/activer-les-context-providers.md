# Activer les context providers

> Pour les operators qui veulent que l'IA arrive briefée sur leur projet sans avoir à coller du contexte à chaque message.

## Prérequis

- Un projet déjà créé.
- (Pour Git) Le dossier racine est un repo git.
- (Pour Sortie de commande) Vous savez quelle commande shell donne le contexte utile.

## Étapes

1. Dans la sidebar, cliquez sur **Projets**, puis sur la carte du projet à configurer.

2. Ouvrez l'onglet **Context providers**. Quatre fournisseurs sont proposés.
   `[SCREENSHOT: onglet Context providers avec 4 cartes Git, Arborescence, Sortie de commande, Documents]`

3. **Git** — basculez l'interrupteur sur ON. L'aperçu affiche les derniers commits (date, message, auteur). Utile dès que le projet est versionné.

4. **Arborescence** — basculez sur ON. L'aperçu affiche la structure du dossier (fichiers et sous-dossiers, jusqu'à une certaine profondeur). Utile pour que l'IA situe les fichiers.

5. **Sortie de commande** — basculez sur ON, saisissez une commande shell (par exemple `git log --oneline -20`). L'aperçu montre le résultat exact qui sera injecté.
   `[SCREENSHOT: provider Sortie de commande, champ commande rempli, aperçu en dessous]`

6. **Documents** — basculez sur ON, cliquez sur **Uploader**, sélectionnez les fichiers à attacher (PDF, Markdown, TXT). Ces documents resteront disponibles dans tous les chats liés au projet.

7. Cliquez sur **Aperçu** sur n'importe quel fournisseur pour voir exactement ce qui sera transmis à l'IA. Désactivez les fournisseurs qui surchargent inutilement.
   `[SCREENSHOT: aperçu détaillé d'un context provider avec compteur de jetons]`

8. Vos modifications sont sauvegardées automatiquement. Le bandeau **Contexte injecté** indique le total estimé de jetons consommés à chaque message.

## Vérification

Ouvrez un chat lié au projet et posez une question précise (par exemple : *"Quels fichiers ont changé cette semaine ?"*). La réponse doit citer des fichiers et des commits réels.

## Si ça ne marche pas

- **L'aperçu Git est vide** : votre dossier n'est pas un repo git ou n'a aucun commit. Initialisez-le ou désactivez le fournisseur.
- **La sortie de commande affiche une erreur** : la commande échoue dans votre shell. Testez-la d'abord en terminal.
- **Le compteur de jetons est rouge** : le contexte est trop lourd, désactivez Arborescence ou réduisez le nombre de documents attachés.

> **Concept :** [book ch12 — Chat interactif](https://github.com/nidal-z/apollia-os/blob/main/book/src/ch12-00-chat-interactif.md) — savoir quel fournisseur activer selon le type de projet.
