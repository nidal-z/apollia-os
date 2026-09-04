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

1. Dans la barre latérale, cliquez sur **Réglages**, puis sur **À propos** (en bas de la navigation, dans le groupe **Aide**).

2. L'en-tête affiche la **version**, le **canal de diffusion** et la **plateforme**. Le canal est lu depuis la chaîne de version : une version portant un suffixe après le tiret, comme `0.1.0-preview`, est une préversion ; une version nue est une version stable. Ce numéro est celui du produit, et ce n'est pas celui gravé sur les fichiers d'installation : ceux-ci portent `0.1.0-1`, parce que le format d'installeur Windows refuse un identifiant de préversion non numérique. Les deux désignent le même build, et le numéro à citer est celui de cet écran.

3. La section **Version et build** liste les valeurs qui identifient cette installation : version, plateforme, interpréteur Python, moteur d'inférence et moteur de transcription. Un clic sur une valeur la copie.

<!-- claim:about-reports-resolved-data-dir -->

4. La section **Où vivent vos données** affiche le **répertoire de données** : le dossier unique qui contient vos conversations, la mémoire des agents, les modèles, la configuration et le journal d'audit. Cliquez dessus pour copier le chemin complet.

   > **Note :** le chemin affiché est celui que cette installation a réellement résolu, pas un exemple générique. Il se lit normalement `.apollia` dans votre répertoire personnel, et il suit le répertoire personnel avec lequel Apollia a été lancé. Fiez-vous à la valeur à l'écran plutôt qu'à un chemin écrit dans un guide.

5. La section **Ce qui tourne sur cette machine** énonce, point par point, ce qui reste en local : l'inférence, la transcription vocale, le stockage et le journal d'audit.

## Signaler un problème avec les bonnes informations

1. Toujours sur la page **À propos**, dans la section **Version et build**, cliquez sur **Copier le rapport de diagnostic**. Cela copie un bloc de texte brut avec la version, la plateforme, l'interpréteur Python, le répertoire de données, les deux moteurs et la licence.

2. Dans la section **Ressources**, cliquez sur **Signaler un problème**. Votre navigateur ouvre un nouveau ticket sur le dépôt public.

3. Collez le rapport de diagnostic dans le ticket, puis décrivez ce que vous faisiez et ce que vous attendiez à la place.

> **Rien n'est envoyé automatiquement.** Le rapport de diagnostic part dans votre presse-papiers et nulle part ailleurs. Relisez-le avant de le coller, et retirez tout chemin que vous préférez ne pas publier.

## Sauvegarder ou tout effacer

Le répertoire de données de l'étape 4 est un dossier ordinaire. Le copier ailleurs sauvegarde Apollia ; le supprimer remet l'application dans son état de premier lancement. Pour le parcours guidé, avec ses précautions, voir [Réinitialiser Apollia (factory reset)](../troubleshooting/factory-reset.md).

## Pour aller plus loin

Le manuel complet, avec les guides de chaque écran, est publié sur **docs.apollia.fr**. On y accède depuis **Réglages → Aide → Centre d'aide**, ou depuis **Réglages → À propos → Documentation**.

## Vérification

La page **À propos** affiche une version, et le répertoire de données qu'elle indique existe sur votre disque à ce chemin exact.

## Si ça ne marche pas

- **« Les informations système n'ont pas pu être chargées » :** l'interface a atteint la page avant que le runtime ne soit prêt. Quittez la page et revenez, ou relancez l'application.
- **Le répertoire de données indique que le répertoire personnel n'a pas pu être résolu :** Apollia a été lancé dans un environnement sans répertoire personnel (`HOME` sur macOS et Linux, `USERPROFILE` sur Windows). Cela arrive avec certains lanceurs de services. Relancez-le depuis votre session normale.
- **La version indique « Version inconnue » :** même cause que le premier cas. Le reste de la page reste utilisable.
