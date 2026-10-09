---
title: Retrouver sa version et ses données
slug: /operator-help/transversal/find-your-version-and-data
sidebar_position: 5
---

# Retrouver sa version et ses données

> Pour les operators qui veulent savoir exactement ce qui est installé, où Apollia range leurs données sur le disque, et quoi joindre à un rapport de bug.

## Prérequis

- Aucun. La page est accessible à tout moment, y compris hors ligne.

## Étapes

1. Dans la barre latérale, cliquez sur **Réglages**. La fenêtre s'ouvre sur la section **Général**, qui contient tout ce dont cette page se sert.

2. Le bloc **Mise à jour de l'application** affiche la **Version installée**, et dessous le canal de mise à jour en vigueur : **Canal stable** ou **Canal préversion**. Le canal est celui choisi sur la ligne suivante, **Canal de mise à jour** ; il n'est pas déduit du numéro de version. Un clic sur la version la copie.

<!-- claim:about-reports-resolved-data-dir -->

3. Le bloc **Application** affiche le **Répertoire de données** : le dossier unique qui contient vos conversations, la mémoire des agents, les modèles, la configuration et le journal d'audit. Cliquez dessus pour copier le chemin complet.

   > **Note :** le chemin affiché est celui que cette installation a réellement résolu, pas un exemple générique. Il se lit normalement `.apollia` dans votre répertoire personnel, et il suit le répertoire personnel avec lequel Apollia a été lancé. Fiez-vous à la valeur à l'écran plutôt qu'à un chemin écrit dans un guide.

4. En bas de la section, le bloc **À propos** contient **Ce qui tourne sur cette machine**, qui énonce, point par point, ce qui reste en local : l'inférence, la transcription vocale, le stockage et le journal d'audit. Suivent la licence, les briques open-source sur lesquelles Apollia est bâti et les liens vers la documentation et le code source.

## Signaler un problème avec les bonnes informations

1. Dans **Réglages**, ouvrez **Matériel** (groupe **Système**) et cliquez sur **Copier les infos** dans son en-tête. Cela copie un bloc de texte brut avec la version, le système d'exploitation, le processeur, la mémoire, chaque GPU, et chaque moteur avec sa version. En mode développeur, il porte aussi l'interpréteur Python, le fichier de configuration et le répertoire de données.

2. De retour dans **Général**, dans le bloc **Application**, cliquez sur **Ouvrir dans le navigateur** sur la ligne **Signaler un problème**. Votre navigateur ouvre la page de signalement.

3. Collez ce bloc dans le signalement, puis décrivez ce que vous faisiez et ce que vous attendiez à la place.

> **Rien n'est envoyé automatiquement.** Le bloc part dans votre presse-papiers et nulle part ailleurs. Relisez-le avant de le coller, et retirez tout chemin que vous préférez ne pas publier.

## Sauvegarder ou tout effacer

Le répertoire de données de l'étape 3 est un dossier ordinaire. Le copier ailleurs sauvegarde Apollia ; le supprimer remet l'application dans son état de premier lancement. Pour le parcours guidé, avec ses précautions, voir [Réinitialiser Apollia (factory reset)](../troubleshooting/factory-reset.md).

## Pour aller plus loin

Le manuel complet, avec les guides de chaque écran, est publié sur **docs.apollia.fr**. On y accède depuis **Réglages**, section **Général** : **Ouvrir le centre d'aide** dans le bloc **Aide**, ou **Documentation** sous **Ressources** dans le bloc **À propos**.

## Vérification

La section **Général** affiche une version, et le répertoire de données qu'elle indique existe sur votre disque à ce chemin exact.

## Si ça ne marche pas

- **La version indique « Version inconnue » :** l'interface a atteint la section avant que le moteur ne soit prêt. Fermez les réglages et rouvrez-les, ou relancez l'application.
- **Le répertoire de données indique que le répertoire personnel n'a pas pu être résolu :** Apollia a été lancé dans un environnement sans répertoire personnel (`HOME` sur macOS et Linux, `USERPROFILE` sur Windows). Cela arrive avec certains lanceurs de services. Relancez-le depuis votre session normale.
