# Réinitialiser Apollia (factory reset)

> Pour tout operator qui souhaite remettre Apollia dans son état d'usine : effacer agents, mémoire, projets, intégrations et préférences. Cette action est irréversible — lisez chaque étape avant d'agir.

## Avant de commencer — à lire impérativement

Une réinitialisation supprime **toutes** vos données locales : agents installés, mémoire des conversations, projets, intégrations MCP, transcriptions, historique de chat, règles de permissions, clés API enregistrées et préférences.

**Aucune de ces données n'est récupérable après confirmation.** Les fichiers stockés ailleurs sur votre disque (vos documents, vos rapports) ne sont pas touchés.

Avant de continuer, demandez-vous :

- Souhaitez-vous vraiment tout perdre, ou voulez-vous simplement résoudre un problème précis ? Consultez d'abord [Un agent est bloqué](un-agent-est-bloque.md) ou [Le fournisseur d'IA ne répond pas](le-fournisseur-d-ia-ne-repond-pas.md).
- Avez-vous fait une **sauvegarde** de ce qui compte ? Voir l'étape 1 ci-dessous.

## Étape 1 — Sauvegarder ce qui compte (recommandé)

1. **Mémoire :** utilisez la CLI `apollia-os memory export --namespace <namespace>` pour exporter la mémoire de chaque agent.
2. **Transcriptions :** ouvrez **Transcriptions** et notez les transcriptions importantes.
3. **Liste de vos agents et connexions :** prenez une capture d'écran ou notez les noms : vous devrez les réinstaller manuellement après le reset.

## Étape 2 — Lancer la réinitialisation

1. Dans la sidebar, cliquez sur **Paramètres**, puis sur la section **Zone de danger**.
   `[SCREENSHOT: page Paramètres Zone de danger, encart rouge "Réinitialisation d'usine" avec bouton clairement isolé]`
2. Repérez le bloc **Réinitialisation d'usine**. Lisez attentivement la liste des données qui vont être supprimées.
3. Cliquez sur le bouton rouge **Réinitialisation d'usine**.

## Étape 3 — Confirmer explicitement

Une fenêtre de confirmation s'ouvre avec une pause de sécurité de quelques secondes pendant lesquelles le bouton est désactivé.

1. Lisez à nouveau la liste des données concernées.
2. Dans le champ de confirmation, **tapez exactement** `FACTORY RESET` (en majuscules). Aucun copier-coller n'est accepté.
3. Le bouton **Confirmer la réinitialisation** devient actif uniquement quand le mot est correct et la pause écoulée.
4. Cliquez sur **Confirmer la réinitialisation**.

## Étape 4 — Après la réinitialisation

1. Apollia redémarre automatiquement et affiche l'écran d'accueil initial. Si le redémarrage automatique échoue (environnement sans bundle packagé), un bandeau orange vous invite à relancer l'application manuellement.
2. Reconfigurez votre fournisseur d'IA : voir [Connecter un fournisseur d'IA](../installation/connecter-un-fournisseur-d-ia.md).
3. Réinstallez vos agents, vos intégrations MCP et vos projets selon votre besoin.
4. Si vous avez exporté votre mémoire via la CLI, réimportez-la avec `apollia-os memory import`.

## Si quelque chose se passe mal

- **Apollia ne redémarre pas après la réinitialisation :** lancez l'application manuellement depuis votre menu d'applications.
- **L'écran d'onboarding ne s'affiche pas :** la réinitialisation a peut-être échoué partiellement. Contactez le support en précisant l'instant exact du reset.
- **Vous regrettez la suppression :** restaurez vos sauvegardes de l'étape 1. Sans sauvegarde, les données sont définitivement perdues.

> **Concept :** [Securite-Donnees-Locales](https://github.com/nidal-z/apollia-os/wiki/Securite-Donnees-Locales) — comprendre où Apollia stocke vos données et ce qui est effacé exactement lors d'une réinitialisation.
