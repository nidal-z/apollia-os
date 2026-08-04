# Télécharger des modèles locaux

> Pour tout operator qui veut faire tourner Apollia 100 % hors ligne : télécharger un modèle d'IA et un modèle de transcription vocale directement sur votre machine.

## Prérequis

- Au moins 5 Go d'espace disque libre (jusqu'à 30 Go selon le modèle).
- Connexion internet active pendant le téléchargement uniquement.
- Apollia est lancé et le bandeau supérieur ne signale pas d'erreur.

## Étapes

1. Dans la sidebar, cliquez sur **Paramètres**, puis sur la section **Hub de modèles**.
   ![page Hub de modèles, liste des modèles disponibles avec colonnes Nom, Taille, Type, État](/img/operator-help/fr/installation-telecharger-des-modeles-locaux-1.png)

2. Filtrez la liste par type **GGUF** pour les modèles d'IA conversationnelle.

   > **Note :** les modèles Whisper (dictée vocale) sont gérés depuis la section **Reconnaissance vocale** dans Paramètres, pas depuis le Hub de modèles.

3. Cliquez sur la ligne du modèle qui vous intéresse. Un panneau affiche la taille exacte, la durée estimée de téléchargement et la configuration matérielle recommandée.

4. Cliquez sur **Télécharger**. Une barre de progression apparaît à côté du modèle.
   ![ligne modèle "Llama 3.1 8B" avec barre de progression à 42 % et bouton Annuler](/img/operator-help/fr/installation-telecharger-des-modeles-locaux-2.png)

5. Laissez la fenêtre ouverte (le téléchargement peut prendre 5 à 30 minutes selon votre débit et la taille du modèle). Vous pouvez continuer à utiliser le reste d'Apollia. Le téléchargement ne s'interrompt pas en raison d'une durée trop longue - seul l'établissement de la connexion initiale est soumis à un délai (30 secondes).

6. À la fin, l'état du modèle passe à **Disponible localement** avec une pastille verte.

7. (Optionnel) Cliquez sur **Définir par défaut** pour utiliser ce modèle automatiquement dans les nouveaux chats (GGUF) ou pour la dictée (Whisper). Juste après la sélection d'un modèle local pendant l'onboarding, l'étape de conversation affiche brièvement un statut **Démarrage du moteur** le temps que le modèle s'enregistre ; la conversation démarre d'elle-même dès qu'il est prêt.
   ![Hub de modèles : la section Modèles installés, avec le modèle actif marqué d'un badge Utilisé](/img/operator-help/fr/installation-telecharger-des-modeles-locaux-1bis.png)

8. L'espace disque utilisé par tous vos modèles est affiché en bas de la page. Pour libérer de la place, cliquez sur **Supprimer** sur n'importe quel modèle déjà téléchargé.

## Vérification

Pour un modèle GGUF, ouvrez un nouveau chat, sélectionnez votre modèle local dans le sélecteur de backend, et envoyez un message : la réponse arrive sans connexion internet. Pour un modèle Whisper, suivez la page [Activer la dictée vocale](../chat/activer-la-dictee-vocale.md).

## Si ça ne marche pas

- **Téléchargement bloqué à 0 % :** vérifiez votre connexion internet et redémarrez le téléchargement.
- **Espace disque insuffisant :** supprimez un modèle existant ou libérez de la place avant de relancer.
- **Modèle absent du sélecteur après téléchargement :** redémarrez Apollia pour qu'il détecte le nouveau modèle.

> **Modèles locaux fiables :** les modèles locaux GGUF appellent vos outils de manière fiable, sans réglage de votre part.

> **Référence technique :** [Référence Apollia](/reference) - formats GGUF supportés, paramètres de quantization, recommandations matériel.
