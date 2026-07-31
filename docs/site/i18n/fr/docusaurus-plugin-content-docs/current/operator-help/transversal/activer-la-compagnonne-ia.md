# Activer l'Aide Apollia

> Pour les operators qui veulent garder un assistant flottant à portée de clic, capable de répondre à une question rapide sans quitter l'écran courant.

## Prérequis

- Au moins un fournisseur d'IA est configuré et prêt. L'état est visible **en haut à gauche** : un **point coloré à gauche du mot *Apollia*** dans le bandeau supérieur indique l'état combiné runtime + LLM.
  - 🟢 vert - runtime sain + au moins un LLM prêt → l'Aide Apollia peut démarrer.
  - 🟡 ambre - aucun LLM connecté → le bouton est grisé.
  - 🔴 rouge - runtime déconnecté → rien ne fonctionnera ; quittez et relancez l'application.

  Pour configurer un LLM, allez sur **Paramètres → Modèles LLM**.

## Étapes

1. Dans la sidebar, repérez le bouton **Aide Apollia** (logo Apollia, dans le bas de la sidebar).

   Le bouton est **grisé et non cliquable** tant qu'aucun fournisseur d'IA n'est prêt - au survol, un tooltip explicite : *« Configurez un modèle LLM pour activer l'aide contextuelle »*.

   ![Le tableau de bord, avec la barre latérale et l'accès à l'Aide Apollia](/img/operator-help/fr/transversal-activer-la-compagnonne-ia-1.png)

2. Cliquez sur le bouton. Un **panneau flottant** s'ouvre, ancré à droite de l'écran par défaut. Une session de discussion dédiée démarre - un court spinner s'affiche pendant la création (1 à 2 secondes).

3. Posez une question rapide. L'Aide Apollia répond sans interrompre votre travail sur la page principale.

   ![Panneau de l'Aide Apollia ouvert, avec son message d'accueil et la zone de saisie](/img/operator-help/fr/transversal-activer-la-compagnonne-ia-2.png)

4. **Déplacer le panneau** : saisissez la **poignée en haut du panneau** (icône grip-handle) et glissez-le où vous voulez. Il s'aligne automatiquement aux bords de l'écran pour rester accessible.

5. **Redimensionner** : tirez le **coin inférieur gauche** pour ajuster la largeur et la hauteur.

   Votre position et votre taille préférées sont **mémorisées** d'une session à l'autre.

6. L'Aide Apollia **connaît la page que vous êtes en train de regarder**. Si vous êtes sur *Mes assistants* et que vous demandez *« pourquoi cet agent a échoué ? »*, elle saura à quoi vous faites référence.

7. **Réduire en bulle** : cliquez sur l'icône Moins (−) en haut du panneau. Il se condense en une mini-bulle cliquable que vous pouvez redéployer plus tard. L'historique de la conversation est conservé.

8. **Fermer** : cliquez sur l'icône X en haut du panneau (ou utilisez le raccourci **Cmd+/** / **Ctrl+/** depuis n'importe où dans l'app). Le panneau disparaît mais l'Aide reste *activée* - un nouveau Cmd+/ le rouvre instantanément sur la même session.

9. **Désactiver complètement** : cliquez à nouveau sur le bouton **Aide Apollia** dans la sidebar. Cette fois, c'est la désactivation : le panneau se ferme et l'historique est fermé. La préférence est persistée - la prochaine ouverture redémarrera une session vierge.

## Raccourci clavier

- **Cmd+/** (macOS) / **Ctrl+/** (Windows et Linux) - bascule l'affichage du panneau **sans toucher à l'activation**. Pratique pour cacher rapidement l'Aide pendant une démo et la rappeler ensuite.

## Vérification

- Cliquer le bouton **Aide Apollia** ouvre le panneau en moins de 2 secondes.
- Le panneau peut être déplacé et redimensionné sans tomber hors écran.
- Le raccourci **Cmd+/** bascule l'affichage instantanément.
- L'Aide répond à *« sur quelle page suis-je ? »* en nommant correctement la page courante.

## Si ça ne marche pas

- **L'Aide ne répond pas / aucune réponse n'arrive** : vérifiez le **point d'état Apollia** dans le bandeau. S'il est ambre, configurez un fournisseur LLM dans **Paramètres → Modèles LLM**.
- **Le bouton dans la sidebar est grisé** : aucun LLM n'est prêt. Tooltip explicite au survol. Voir point précédent.
- **Le panneau s'ouvre puis affiche une erreur** : la session n'a pas pu démarrer. Cliquez sur **Réessayer** dans le panneau, ou fermez et rouvrez via la sidebar.
- **Le panneau est invisible alors que je l'ai déjà ouvert** : il est peut-être minimisé en bulle dans un coin de l'écran. Cherchez la bulle Aide Apollia ; sinon **Cmd+/** force la réouverture.
- **Le panneau s'ouvre dans un coin inaccessible** : maintenez le bouton **Aide Apollia** de la sidebar enfoncé (ou désactivez puis réactivez) pour réinitialiser la position par défaut (côté droit).

> **Concept :** [Explication Apollia](/explanation)
