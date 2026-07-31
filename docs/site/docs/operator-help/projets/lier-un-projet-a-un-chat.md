# Lier un projet à un chat

> Pour les operators qui veulent qu'une conversation IA charge automatiquement les fichiers, l'historique git et les documents d'un projet.

## Prérequis

- Un projet créé avec au moins un context provider activé.
- Un fournisseur d'IA connecté.
- (Optionnel) Le dossier projet est un repo git, pour activer le fournisseur Git.

## Étapes

1. **Depuis la page Projets** : cliquez sur **Projets** dans la sidebar, ouvrez le projet, puis cliquez sur **+ Nouveau chat lié** en haut à droite.
   ![page de détail projet, bouton + Nouveau chat lié surligné](/img/operator-help/projets-lier-un-projet-a-un-chat-1.png)

2. Le chat s'ouvre automatiquement. L'icône du projet apparaît dans son en-tête : il est attaché.

3. **Depuis un chat existant** : ouvrez le chat, cliquez sur le menu en haut (trois points), puis sur **Lier à un projet**.
   ![en-tête de chat, menu déroulant avec option Lier à un projet](/img/operator-help/projets-lier-un-projet-a-un-chat-2.png)

4. Sélectionnez le projet cible dans la liste déroulante. Le contexte s'attache instantanément.

5. Vérifiez les **blocs de contexte** affichés en bas du chat. Vous pouvez les replier ou les déplier pour voir ce qui est réellement transmis à l'IA.

6. Posez une question spécifique au projet pour valider - par exemple : *"Quels fichiers ont changé cette semaine ?"*. La réponse doit citer des fichiers et des commits réels.

7. Vous pouvez créer plusieurs chats liés au même projet. Chacun garde son propre historique mais partage le même contexte.
   ![page projet avec liste de chats liés, chacun avec son titre et sa date](/img/operator-help/projets-lier-un-projet-a-un-chat-3.png)

8. Pour délier un chat, ouvrez son menu en haut et cliquez sur **Délier du projet**. Le chat est conservé, seul le contexte projet disparaît.

## Vérification

L'icône projet est visible dans l'en-tête du chat et le bandeau de contexte affiche les blocs Git, Arborescence ou Documents que vous avez activés.

## Si ça ne marche pas

- **Le bouton + Nouveau chat lié est grisé** : aucun fournisseur d'IA n'est connecté, ouvrez **Paramètres → Backends LLM** pour en configurer un.
- **Les blocs de contexte sont vides** : retournez sur la page projet et activez au moins un context provider.
- **L'IA ne semble rien savoir du projet** : le contexte est probablement plié, dépliez-le ou rechargez le chat.

> **Concept :** [Explication Apollia](../../explanation/index.md) - comprendre comment le contexte d'un projet est utilisé par l'IA.
