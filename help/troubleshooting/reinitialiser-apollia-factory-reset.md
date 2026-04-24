# Réinitialiser Apollia (factory reset)

> Pour tout operator qui souhaite remettre Apollia dans son état d'usine : effacer agents, mémoire, projets, intégrations et préférences. Cette action est irréversible — lisez chaque étape avant d'agir.

## Avant de commencer — à lire impérativement

Une réinitialisation supprime **toutes** vos données locales : agents installés, mémoire des conversations, projets, intégrations MCP, transcriptions, historique de chat, règles de permissions, clés API enregistrées et préférences.

**Aucune de ces données n'est récupérable après confirmation.** Les fichiers stockés ailleurs sur votre disque (vos documents, vos rapports) ne sont pas touchés.

Avant de continuer, demandez-vous :

- Souhaitez-vous vraiment tout perdre, ou voulez-vous simplement résoudre un problème précis ? Consultez d'abord [Un agent est bloqué](un-agent-est-bloque.md) ou [Le fournisseur d'IA ne répond pas](le-fournisseur-d-ia-ne-repond-pas.md).
- Avez-vous fait une **sauvegarde** de ce qui compte ? Voir l'étape 1 ci-dessous.

## Étape 1 — Sauvegarder ce qui compte (recommandé)

1. **Mémoire :** ouvrez **Mémoire**, cliquez sur **Exporter tout** et conservez le fichier produit dans un endroit sûr.
2. **Transcriptions :** ouvrez **Transcriptions** et exportez l'historique au format de votre choix (JSON, Markdown, texte).
3. **Audit trail :** ouvrez **Observabilité → Audit trail** et exportez l'historique pour conserver une trace des actions passées.
4. **Liste de vos agents et intégrations :** prenez une capture d'écran ou notez les noms : vous devrez les réinstaller manuellement après le reset.

## Étape 2 — Lancer la réinitialisation

1. Dans la sidebar, cliquez sur **Settings**, puis sur l'onglet **Zone de danger**.
   `[SCREENSHOT: page Settings Zone de danger, encart rouge "Réinitialisation" avec bouton clairement isolé]`
2. Repérez le bloc **Réinitialiser Apollia (factory reset)**. Lisez attentivement la liste des données qui vont être supprimées.
3. Cliquez sur le bouton rouge **Réinitialiser**.

## Étape 3 — Confirmer explicitement

Une fenêtre de confirmation s'ouvre avec une pause de sécurité de quelques secondes pendant lesquelles le bouton est désactivé.

1. Lisez à nouveau la liste des données concernées.
2. Dans le champ de confirmation, **tapez exactement** le mot demandé (généralement `RESET` ou `RÉINITIALISER`). Aucun copier-coller n'est accepté.
3. Le bouton **Confirmer la réinitialisation** devient actif uniquement quand le mot est correct et la pause écoulée.
4. Cliquez sur **Confirmer la réinitialisation**.

## Étape 4 — Après la réinitialisation

1. Apollia redémarre automatiquement et affiche l'écran d'accueil initial (onboarding).
2. Reconfigurez votre fournisseur d'IA : voir [Connecter un fournisseur d'IA](../installation/connecter-un-fournisseur-d-ia.md).
3. Réinstallez vos agents, vos intégrations MCP et vos projets selon votre besoin.
4. Si vous avez sauvegardé votre mémoire, importez-la depuis **Mémoire → Importer**.

## Si quelque chose se passe mal

- **Apollia ne redémarre pas après la réinitialisation :** lancez l'application manuellement depuis votre menu d'applications.
- **L'écran d'onboarding ne s'affiche pas :** la réinitialisation a peut-être échoué partiellement. Contactez le support en précisant l'instant exact du reset.
- **Vous regrettez la suppression :** restaurez vos sauvegardes de l'étape 1. Sans sauvegarde, les données sont définitivement perdues.

> **Concept :** [Securite-Donnees-Locales](https://github.com/nidal-z/apollia-os/wiki/Securite-Donnees-Locales) — comprendre où Apollia stocke vos données et ce qui est effacé exactement lors d'une réinitialisation.
