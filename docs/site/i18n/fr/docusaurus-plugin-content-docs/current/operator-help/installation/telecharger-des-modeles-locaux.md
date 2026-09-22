---
title: Télécharger des modèles locaux
slug: /operator-help/installation/download-local-models
sidebar_position: 5
---

# Télécharger des modèles locaux

> Pour tout operator qui veut faire tourner Apollia 100 % hors ligne : télécharger un modèle d'IA et un modèle de transcription vocale directement sur votre machine.

## Prérequis

- Au moins 5 Go d'espace disque libre (jusqu'à 30 Go selon le modèle).
- Connexion internet active pendant le téléchargement uniquement.
- Apollia est lancé et le bandeau supérieur ne signale pas d'erreur.

## Étapes

1. Dans la sidebar, cliquez sur **Paramètres**, puis sur la section **Hub de modèles**.
   ![page Hub de modèles, liste des modèles disponibles avec colonnes Nom, Taille, Type, État](/img/operator-help/installation-telecharger-des-modeles-locaux-1.png)

2. Filtrez la liste par type **GGUF** pour les modèles d'IA conversationnelle.

   > **Note :** les modèles Whisper (dictée vocale) sont gérés depuis la section **Reconnaissance vocale** dans Paramètres, pas depuis le Hub de modèles.

3. Cliquez sur la ligne du modèle qui vous intéresse. Un panneau affiche la taille exacte, la durée estimée de téléchargement et la configuration matérielle recommandée.

4. Cliquez sur **Télécharger**. Une barre de progression apparaît à côté du modèle.
   ![ligne modèle "Llama 3.1 8B" avec barre de progression à 42 % et bouton Annuler](/img/operator-help/installation-telecharger-des-modeles-locaux-2.png)

5. Laissez la fenêtre ouverte (le téléchargement peut prendre 5 à 30 minutes selon votre débit et la taille du modèle). Vous pouvez continuer à utiliser le reste d'Apollia. Le téléchargement ne s'interrompt pas en raison d'une durée trop longue - seul l'établissement de la connexion initiale est soumis à un délai (30 secondes).

6. À la fin, l'état du modèle passe à **Disponible localement** avec une pastille verte.

7. (Optionnel) Cliquez sur **Définir par défaut** pour utiliser ce modèle automatiquement dans les nouveaux chats (GGUF) ou pour la dictée (Whisper). Juste après la sélection d'un modèle local pendant l'onboarding, l'étape de conversation affiche brièvement un statut **Démarrage du moteur** le temps que le modèle s'enregistre ; la conversation démarre d'elle-même dès qu'il est prêt.
   ![Hub de modèles : la section Modèles installés, avec le modèle actif marqué d'un badge Utilisé](/img/operator-help/installation-telecharger-des-modeles-locaux-1bis.png)

8. L'espace disque utilisé par tous vos modèles est affiché en bas de la page. Pour libérer de la place, cliquez sur **Supprimer** sur n'importe quel modèle déjà téléchargé.

## Modèles proposés pendant l'onboarding

<!-- claim:onboarding-recommends-for-hardware -->

Pendant l'onboarding, la liste **Modèles recommandés** n'est pas une liste
figée. Apollia mesure votre machine (sa mémoire, et si sa carte graphique a sa
propre mémoire ou la partage, comme sur un Mac), demande à HuggingFace quels
fichiers existent pour les générations de modèles qu'il connaît, et lit les
premiers mégaoctets de chaque candidat pour vérifier que le moteur embarqué peut
le charger et lui faire appeler des outils. La liste est ensuite classée pour
votre machine : le même modèle peut arriver en tête sur un Mac et plus bas sur
un PC doté d'une petite carte graphique.

Au-dessus de la liste, **Fenêtre de contexte** règle la part d'une
conversation que le modèle garde en mémoire (32k tokens par défaut). Une fenêtre
plus grande demande plus de mémoire, donc la liste est recalculée quand vous la
changez, et le modèle choisi est configuré avec cette fenêtre. Vous pouvez la
modifier plus tard dans les **Réglages**, sur le backend du modèle, à la ligne
**Fenêtre de contexte** ; le moteur, la compaction automatique et la jauge
**Ctx** la suivent tous. Un modèle entraîné sur une fenêtre plus courte tourne
à la sienne : au-delà, il lit des positions sur lesquelles il n'a jamais été
entraîné, et ses réponses se dégradent. La ligne le signale alors.

Chaque ligne dit pourquoi elle est là : la mémoire qu'il faut, répartie entre le
modèle lui-même, sa mémoire de conversation et le moteur, la façon dont il est
compressé (par exemple `Q4_K_M`), et la mémoire depuis laquelle il tourne. Sur
un PC doté d'une carte graphique, c'est soit la mémoire vidéo de la carte, soit,
pour un modèle trop grand pour elle, un partage entre mémoire vidéo et RAM
système. La ligne sous la liste nomme les deux mémoires séparément.

- **Tourne entièrement sur ton GPU :** le cas le plus rapide, mesuré par
  rapport à la seule mémoire vidéo de la carte.
- **Tient de justesse** ou **avec peu de marge :** le modèle tient, avec peu de
  place pour les autres applications.
- **Trop grand pour ton GPU seul :** une partie du modèle est en RAM système,
  bien plus lente. Un modèle plus petit peut répondre plus vite.
- **N couches sur M sur votre GPU, le reste sur le processeur :** le même
  partage, compté en couches.
- Un modèle qui générerait moins d'environ quatre tokens par seconde sur votre
  machine n'est pas proposé, sauf si rien de plus rapide ne tient.
- **Sans outils :** le modèle peut discuter mais ne peut pas exécuter d'agents.
- **HuggingFace injoignable :** la liste est récupérée en direct, donc sans
  connexion il n'y a rien à proposer. Importez un fichier de modèle que vous
  avez déjà, ou utilisez un fournisseur cloud, et réessayez plus tard.
- **Aucun modèle sélectionné ne tient sur cette machine :** utilisez un
  fournisseur cloud, ou importez un modèle plus petit depuis le disque.

Le même classement est disponible en ligne de commande, daemon lancé :
`apollia-os model recommend`.

## Vérification

Pour un modèle GGUF, ouvrez un nouveau chat, sélectionnez votre modèle local dans le sélecteur de backend, et envoyez un message : la réponse arrive sans connexion internet. Pour un modèle Whisper, suivez la page [Activer la dictée vocale](../chat/enable-voice-dictation.md).

## Si ça ne marche pas

- **Téléchargement bloqué à 0 % :** vérifiez votre connexion internet et redémarrez le téléchargement.
- **Espace disque insuffisant :** supprimez un modèle existant ou libérez de la place avant de relancer.
- **Modèle absent du sélecteur après téléchargement :** redémarrez Apollia pour qu'il détecte le nouveau modèle.

> **Modèles locaux fiables :** les modèles locaux GGUF appellent vos outils de manière fiable, sans réglage de votre part.

> **Référence technique :** [Référence Apollia](/reference) - formats GGUF supportés, paramètres de quantization, recommandations matériel.
