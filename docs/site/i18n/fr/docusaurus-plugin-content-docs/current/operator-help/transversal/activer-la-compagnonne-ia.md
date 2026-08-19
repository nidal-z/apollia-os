# Activer l'Aide Apollia

> Pour les operators qui veulent garder un assistant flottant à portée de clic, capable de répondre à une question rapide sans quitter l'écran courant.

## Prérequis

- Au moins un fournisseur d'IA est configuré et prêt. L'état est visible **en haut à gauche** : un **point coloré à gauche du mot *Apollia*** dans le bandeau supérieur indique l'état combiné runtime + LLM.
  - 🟢 vert - runtime sain + au moins un LLM prêt → l'Aide Apollia peut démarrer.
  - 🟡 ambre - aucun LLM connecté → le bouton est grisé.
  - 🔴 rouge - runtime déconnecté → rien ne fonctionnera ; quittez et relancez l'application.

  Pour configurer un LLM, allez sur **Paramètres → Modèles LLM**.

## Étapes

1. Appuyez sur **Cmd+/** (macOS) ou **Ctrl+/** (Windows et Linux) depuis n'importe où dans l'application. Il n'y a pas de bouton : le raccourci et la palette de commandes sont les deux entrées. Dans la palette, ouverte par **Cmd+K**, l'action s'appelle **Basculer l'Aide Apollia**.

   Rien ne s'ouvre tant qu'aucun fournisseur d'IA n'est prêt.

   ![L'écran Accueil, au moment où le panneau Aide Apollia va s'ouvrir](/img/operator-help/transversal-activer-la-compagnonne-ia-1.png)

2. Un **panneau flottant** s'ouvre, ancré à droite de l'écran par défaut. Une session de discussion dédiée démarre - un court spinner s'affiche pendant la création (1 à 2 secondes).

3. Posez une question rapide. L'Aide Apollia répond sans interrompre votre travail sur la page principale.

   ![Panneau de l'Aide Apollia ouvert, avec son message d'accueil et la zone de saisie](/img/operator-help/transversal-activer-la-compagnonne-ia-2.png)

4. **Déplacer le panneau** : saisissez la **poignée en haut du panneau** (icône grip-handle) et glissez-le où vous voulez. Il s'aligne automatiquement aux bords de l'écran pour rester accessible.

5. **Redimensionner** : tirez le **coin inférieur droit** pour ajuster la largeur et la hauteur. Ce coin ayant le focus, les flèches redimensionnent par pas de 20 pixels.

   Votre position et votre taille préférées sont **mémorisées** d'une session à l'autre.

6. L'Aide Apollia **connaît la page que vous êtes en train de regarder**. Si vous êtes sur *Mes assistants* et que vous demandez *« pourquoi cet agent a échoué ? »*, elle saura à quoi vous faites référence.

7. **Réduire en bulle** : cliquez sur l'icône Moins (−) en haut du panneau. Il se condense en une mini-bulle cliquable que vous pouvez redéployer plus tard. L'historique de la conversation est conservé.

8. **Fermer** : cliquez sur l'icône X en haut du panneau, ou appuyez de nouveau sur **Cmd+/** depuis n'importe où dans l'app. Le panneau disparaît mais la session reste ouverte : le Cmd+/ suivant revient sur la même conversation.

## Raccourci clavier

- **Cmd+/** (macOS) / **Ctrl+/** (Windows et Linux) - ouvre et ferme le panneau. Pratique pour cacher rapidement l'Aide pendant une démo et la rappeler ensuite.
- **Cmd+Shift+C** - ouvre le panneau *et* place le curseur dans son champ de saisie, depuis n'importe où. À utiliser quand vous comptez taper tout de suite.
- **Cmd+Alt** plus une flèche - déplace le panneau vers un autre bord. Ne fonctionne que si le panneau lui-même a le focus, ce qui explique qu'il ne se passe rien depuis la page principale.

## Vérification

- Cliquer le bouton **Aide Apollia** ouvre le panneau en moins de 2 secondes.
- Le panneau peut être déplacé et redimensionné sans tomber hors écran.
- Le raccourci **Cmd+/** bascule l'affichage instantanément.
- L'Aide répond à *« sur quelle page suis-je ? »* en nommant correctement la page courante.

## Si ça ne marche pas

- **L'Aide ne répond pas / aucune réponse n'arrive** : vérifiez le **point d'état Apollia** dans le bandeau. S'il est ambre, configurez un fournisseur LLM dans **Paramètres → Modèles LLM**.
- **Le panneau s'ouvre puis affiche une erreur** : la session n'a pas pu démarrer. Cliquez sur **Réessayer** dans le panneau, ou fermez-le et refaites Cmd+/.
- **Le panneau est invisible alors que je l'ai déjà ouvert** : il est peut-être minimisé en bulle dans un coin de l'écran. Cherchez la bulle Aide Apollia ; sinon **Cmd+/** force la réouverture.
- **Le panneau s'ouvre dans un coin inaccessible** : donnez-lui le focus puis utilisez **Cmd+Alt** avec une flèche pour le ramener sur un bord visible.

> **Concept :** [Explication Apollia](/explanation)
