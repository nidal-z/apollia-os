---
title: Activer l'Aide Apollia
slug: /operator-help/transversal/enable-apollia-help
sidebar_position: 4
---

# Activer l'Aide Apollia

> Pour les operators qui veulent de l'aide sur l'écran qu'ils regardent, à un clic, sans le quitter.

## Prérequis

- Au moins un fournisseur d'IA est configuré et prêt. L'état est visible **en haut à gauche** : un **point coloré à gauche du mot *Apollia*** dans le bandeau supérieur indique l'état combiné runtime + LLM.
  - 🟢 vert - runtime sain + au moins un LLM prêt → le guide peut répondre.
  - 🟡 ambre - aucun LLM connecté → le guide ne peut pas encore répondre.
  - 🔴 rouge - runtime déconnecté → rien ne fonctionnera ; quittez et relancez l'application.

  Pour configurer un LLM, allez sur **Paramètres → Modèles LLM**.

## Étapes

1. Cliquez sur l'**icône d'aide** (un point d'interrogation dans un cercle) de la barre du haut. Elle est sur chaque écran, toujours au même endroit. **Cmd+/** (macOS) ou **Ctrl+/** (Windows et Linux) fait la même chose depuis n'importe où, même pendant la saisie, tout comme l'action **Ouvrir ou fermer le guide** de la palette de commandes (**Cmd+K**).

   ![L'écran Accueil, avec l'icône d'aide de la barre du haut](/img/operator-help/transversal-activer-la-compagnonne-ia-1.png)

2. Le **panneau de détail** s'ouvre à droite de l'écran, sur son onglet **Guide**. Une session de discussion dédiée démarre, et un court spinner s'affiche pendant sa création (1 à 2 secondes).

3. Posez une question rapide. Le guide répond dans la colonne, et l'écran reste utilisable à sa gauche.

   ![L'onglet Guide ouvert dans le panneau de détail, avec la zone de saisie](/img/operator-help/transversal-activer-la-compagnonne-ia-2.png)

4. Le guide **sait sur quel écran vous êtes**, et quelle fiche y est ouverte. Sur l'écran Agents, un agent ouvert, demander *« que fait cet agent tout seul ? »* suffit : il sait de quel agent vous parlez.

5. **Replier** : cliquez sur l'icône de repli en haut à droite de la colonne, de nouveau sur l'icône d'aide, ou appuyez de nouveau sur **Cmd+/**. La session reste ouverte : l'ouverture suivante revient sur la même conversation.

L'icône d'aide fonctionne aussi pendant le premier lancement : la barre du haut et le panneau de détail restent au-dessus de la fenêtre d'accueil, et le guide sait alors que vous êtes dans le premier lancement.

## Raccourci clavier

- **Cmd+/** (macOS) / **Ctrl+/** (Windows et Linux) - ouvre et replie l'onglet Guide.
- **Cmd+Shift+C** - ouvre l'onglet Guide *et* place le curseur dans son champ de saisie, depuis n'importe où. À utiliser quand vous comptez taper tout de suite.

## Vérification

- Cliquer l'icône d'aide ouvre l'onglet Guide en moins de 2 secondes.
- Le raccourci **Cmd+/** l'ouvre et le replie instantanément.
- Le guide répond à *« sur quel écran suis-je ? »* en nommant correctement l'écran courant.

## Si ça ne marche pas

- **Le guide ne répond pas / aucune réponse n'arrive** : vérifiez le **point d'état Apollia** dans le bandeau. S'il est ambre, configurez un fournisseur LLM dans **Paramètres → Modèles LLM**.
- **L'onglet Guide s'ouvre puis affiche une erreur** : la session n'a pas pu démarrer. Cliquez sur **Réessayer** dans l'onglet, ou repliez-le et rouvrez-le.

> **Concept :** [Explication Apollia](/explanation)
