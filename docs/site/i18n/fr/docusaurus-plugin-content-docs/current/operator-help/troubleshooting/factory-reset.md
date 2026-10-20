---
title: Réinitialiser Apollia (factory reset)
slug: /operator-help/troubleshooting/factory-reset
sidebar_position: 6
---

# Réinitialiser Apollia (factory reset)

> Pour toute personne qui souhaite remettre Apollia dans son état d'usine : effacer agents, mémoire, sources, intégrations et préférences. Cette action est irréversible - lisez chaque étape avant d'agir.

## Avant de commencer - à lire impérativement

Une réinitialisation supprime **toutes** vos données locales : agents installés, mémoire des conversations, sources, connexions, transcriptions, historique de chat, règles de permissions, clés enregistrées, modèles téléchargés et préférences.

**Aucune de ces données n'est récupérable après confirmation.** Les fichiers stockés ailleurs sur votre disque (vos documents, vos rapports) ne sont pas touchés.

Avant de continuer, demandez-vous :

- Souhaitez-vous vraiment tout perdre, ou voulez-vous simplement résoudre un problème précis ? Consultez d'abord [Un agent est bloqué](an-agent-is-stuck.md) ou [L'IA ne répond pas](the-ai-provider-does-not-answer.md).
- Avez-vous fait une **sauvegarde** de ce qui compte ? Voir l'étape 1 ci-dessous.

## Étape 1 - Sauvegarder ce qui compte (recommandé)

1. **Mémoire :** utilisez la CLI `apollia-os memory export --namespace <namespace> --output <fichier>` pour exporter la mémoire de chaque agent. Réimport ensuite avec `apollia-os memory import --namespace <namespace> --input <fichier>`.
2. **Transcriptions :** ouvrez **Transcriptions** *(visible dans la barre latérale avec le mode développeur allumé)* et notez les transcriptions importantes.
3. **Liste de vos agents et connexions :** prenez une capture d'écran ou notez les noms : vous devrez les réinstaller manuellement après la réinitialisation.

## Étape 2 - Lancer la réinitialisation

1. Dans la barre latérale, cliquez sur **Réglages**. La section **Général** s'ouvre : descendez jusqu'au bloc **Zone de danger**.
2. Repérez la carte **Réinitialisation d'usine**, sous **Dernier recours**. Lisez attentivement ce qu'elle annonce comme supprimé.
3. Cliquez sur le bouton rouge **Réinitialisation d'usine**.

## Étape 3 - Confirmer explicitement

Une fenêtre de confirmation s'ouvre avec une **pause de sécurité de 3 secondes** pendant lesquelles le bouton de confirmation reste désactivé.

1. Lisez à nouveau la liste des données concernées.
2. Dans le champ de confirmation, **tapez exactement** `FACTORY RESET` (en majuscules, espace inclus). Le coller au presse-papiers est **bloqué** : vous devez taper la phrase au clavier.
3. Le bouton **Réinitialisation d'usine** de la fenêtre devient actif uniquement quand la phrase est correcte **et** la pause de 3 secondes écoulée.
4. Cliquez sur **Réinitialisation d'usine**.

## Étape 4 - Après la réinitialisation

1. Apollia redémarre automatiquement. Si le redémarrage automatique échoue (environnement de développement sans bundle packagé), un message vous invite à redémarrer l'application manuellement.
2. Au redémarrage, le **parcours de configuration en sept étapes** s'ouvre automatiquement : **Langue → IA → Recherche, voix → Qui je suis → Premier agent → Connexion → Première tâche**. C'est le même parcours qu'au tout premier lancement.
3. À l'étape **IA**, vous devez reconfigurer le LLM (téléchargement d'un modèle local ou connexion à un serveur) - la réinitialisation a effacé vos modèles et vos serveurs. Voir aussi [Connecter un modèle distant](../installation/connecter-un-modele-distant.md) si vous préférez ne pas passer par le parcours intégré.
4. Une fois le parcours terminé, réinstallez vos agents, remettez en place vos connexions et ajoutez de nouveau vos sources, selon votre besoin.
5. Si vous avez exporté votre mémoire via la CLI à l'étape 1, réimportez-la avec `apollia-os memory import --namespace <namespace> --input <fichier>`.

## Si quelque chose se passe mal

- **Apollia ne redémarre pas après la réinitialisation :** lancez l'application manuellement depuis votre menu d'applications.
- **Le parcours de configuration ne s'affiche pas :** la réinitialisation a peut-être échoué partiellement. Vérifiez que le répertoire de données est absent ou vide - sinon, supprimez-le manuellement et relancez. Son chemin exact est affiché dans **Réglages → Général**, dans le bloc **Application**, à la ligne **Répertoire de données** : voir [Retrouver sa version et ses données](../transversal/find-your-version-and-data.md). Si le problème persiste, contactez le support en précisant l'instant exact de la réinitialisation.
- **Le bouton Continuer reste grisé à l'étape IA :** c'est normal tant qu'aucun modèle n'est configuré. Lancez le téléchargement d'un modèle depuis la liste recommandée, ce qui l'ouvre aussitôt, ou cliquez sur la carte **Utiliser un serveur** pour en ajouter un.
- **Vous regrettez la suppression :** restaurez vos sauvegardes de l'étape 1. Sans sauvegarde, les données sont définitivement perdues.

> **Concept :** [Retrouver sa version et ses données](../transversal/find-your-version-and-data.md).
