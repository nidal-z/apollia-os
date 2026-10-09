---
title: Télécharger des modèles locaux
slug: /operator-help/installation/download-local-models
sidebar_position: 5
---

# Télécharger des modèles locaux

> Pour toute personne qui veut faire tourner Apollia sans connexion internet : télécharger un modèle d'IA, un modèle de recherche par le sens ou un modèle de dictée directement sur votre machine.

## Prérequis

- Au moins 5 Go d'espace disque libre (jusqu'à 30 Go selon le modèle).
- Connexion internet active pendant le téléchargement uniquement.
- Apollia est lancé.

## Étapes

1. Dans la barre latérale, cliquez sur **IA**. L'écran liste les modèles déjà présents sur ce poste, par catégorie : **Modèles de langage**, **Embeddings**, **Dictée**.

2. Cliquez sur **Découvrir des modèles**. Une fenêtre s'ouvre sur la **Sélection Apollia**, les modèles choisis et essayés par l'équipe Apollia. Saisissez un nom dans le champ de recherche pour chercher plutôt chez les éditeurs reconnus sur Hugging Face. Les modèles sont regroupés par éditeur, et le filtre de format démarre sur **Tous les formats**.

   > **Note :** la même fenêtre contient les modèles de dictée et les modèles de recherche par le sens : choisissez l'usage du modèle en haut de la fenêtre.

3. Cliquez sur le modèle qui vous intéresse. Sa fiche montre ce qu'il est, ce qu'il sait faire, et ses **Options de téléchargement** pour ce poste : chaque variante indique si elle **Tient sur ce poste**, **Tient de justesse** ou est **Trop grande**, et celle qui convient à votre matériel porte la mention **Recommandée**.

4. Choisissez une variante et cliquez sur le bouton de téléchargement, qui indique sa taille. Le téléchargement démarre.

5. Suivez-le dans le panneau des téléchargements, sous l'icône des téléchargements de la barre du haut : il affiche la progression et le débit, et **Annuler** l'arrête. Vous pouvez fermer la fenêtre de découverte et continuer à utiliser le reste d'Apollia. Un téléchargement ne s'interrompt pas en raison d'une durée trop longue : seul l'établissement de la connexion initiale est soumis à un délai (30 secondes).

6. À la fin, le fichier est vérifié, puis le modèle apparaît sur l'écran **IA**, dans **Sur ce poste**.

7. (Optionnel) Ouvrez le menu en bout de ligne d'un modèle de langage et choisissez **Utiliser par défaut** : les conversations et les agents répondent alors avec lui quand aucun autre modèle n'est choisi, et la ligne porte le badge **Par défaut**. Un modèle de dictée se choisit dans **Réglages → Dictée**. Juste après la sélection d'un modèle local pendant l'onboarding, l'étape de conversation affiche brièvement un statut **Démarrage du moteur…** le temps que le modèle s'enregistre ; la conversation démarre d'elle-même dès qu'il est prêt.

8. Le nombre de vos modèles et l'espace disque qu'ils occupent sont affichés en bas de la liste. Pour libérer de la place, ouvrez le menu d'un modèle et choisissez **Supprimer**.

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
  avez déjà, ou utilisez un serveur, et réessayez plus tard.
- **Aucun modèle sélectionné ne tient sur cette machine :** utilisez un
  serveur, ou importez un modèle plus petit depuis le disque.

Le même classement est disponible en ligne de commande, daemon lancé :
`apollia-os model recommend`.

## Vérification

Pour un modèle de langage, ouvrez un nouveau chat, choisissez votre modèle dans le sélecteur d'agents et de modèles, et envoyez un message : la réponse arrive sans connexion internet. Depuis la fenêtre de découverte, **Utiliser dans une nouvelle conversation** fait de même sur une variante déjà téléchargée. Pour un modèle de dictée, suivez la page [Activer la dictée vocale](../chat/enable-voice-dictation.md).

## Si ça ne marche pas

- **Téléchargement bloqué à 0 % :** vérifiez votre connexion internet, annulez le téléchargement depuis le panneau des téléchargements et relancez-le.
- **Espace disque insuffisant :** supprimez un modèle existant ou libérez de la place avant de relancer.
- **Le téléchargement est refusé :** certains modèles sont en accès restreint. Enregistrez votre **Jeton Hugging Face** dans **Réglages → Général**, sous **Modèles**, puis relancez le téléchargement.
- **Le téléchargement a échoué :** le panneau des téléchargements le range sous **En échec** avec la raison, par exemple une connexion perdue ou un fichier qui ne correspond pas à celui publié.

> **Appel d'outils avec un modèle local :** il fonctionne sans réglage de votre part, mais pas parce que les schémas bruts conviendraient au moteur. Apollia réécrit chaque schéma d'outil avant de l'envoyer, car des constructions qu'un schéma peut légitimement contenir cassent la grammaire d'outils du moteur. Attendez-vous à ce qu'un petit modèle appelle les outils avec moins de précision qu'un modèle de pointe ; cette part tient au modèle, pas à la tuyauterie.

> **Référence technique :** [Référence Apollia](/reference).
