# Programmer un trigger

> Pour les operators qui veulent qu'une tâche IA s'exécute toute seule, à heure fixe ou sur événement, sans intervention manuelle.

## Prérequis

- Au moins un agent installé et démarrable depuis la page Mes assistants.
- Un fournisseur d'IA configuré (depuis **Paramètres → Modèles**).
- Vous savez à quelle fréquence vous voulez que la tâche se répète.

## Étapes - créer une automatisation en langage naturel (parcours par défaut)

1. Dans la sidebar, cliquez sur **Mes déclencheurs**. La page s'intitule **Automatisations**.

2. Cliquez sur le bouton **Créer une automatisation** en haut à droite. Un assistant en **4 étapes** s'ouvre (Décrire → Planifier → Assistant → Aperçu).
   ![page Automatisations, bouton "Créer une automatisation" surligné en haut à droite, stepper 4 étapes visible...](../_screenshots/automatisations-programmer-un-trigger-1.png)

3. **Étape Décrire** - Décrivez **le quand** : à quel moment ou à quelle fréquence le déclencheur doit se déclencher. Exemples : *« Tous les matins à 8 h »*, *« Chaque lundi à 9 h pour préparer la semaine »*, *« Toutes les 30 minutes »*. Vous n'avez pas besoin de nommer un assistant à ce stade : un déclencheur est indépendant de l'assistant qui l'exécutera, vous choisirez cet assistant à l'étape **Assistant**. Cliquez sur **Suivant** ; Apollia analyse la phrase (l'étiquette du bouton passe à *« Analyse… »*).

4. **Étape Planifier** - Apollia affiche sa lecture de votre phrase dans un encadré (par exemple *« Tous les jours à 08:00 »*) avec la **prochaine exécution prévue**. Si quelque chose vous semble inexact, ajustez en langage naturel dans le champ du bas (*« plutôt à 9 h »*) et appuyez sur Entrée - la planification se met à jour. Si Apollia a besoin d'une précision sur le calendrier (heure manquante, jour ambigu…), un bandeau orange affiche les points à clarifier ; complétez-les via le champ d'ajustement. L'absence d'assistant dans la description n'est pas bloquante à cette étape.
   ![étape Planifier - encadré schedule humain ("Tous les jours à 08:00"), ligne "Prochaine exécution : …", cham...](../_screenshots/automatisations-programmer-un-trigger-2.png)

5. **Étape Assistant** - Sélectionnez l'assistant qui exécutera ce déclencheur. Un déclencheur lance toujours **un assistant à la fois**. Si votre phrase nommait un agent existant, il est pré-sélectionné et un sous-texte indique *« Reconnu automatiquement : … »*. Sinon, un encart orange rappelle qu'aucun assistant n'a été reconnu, et vous le choisissez dans la liste déroulante. Seuls les assistants installés apparaissent.

6. **Étape Aperçu** - Apollia récapitule le déclencheur : la planification en clair, l'assistant cible, et la consigne transmise au déclenchement si elle a été détectée. Cliquez sur **Activer cette automatisation**.

7. Une notification confirme la création. L'automatisation apparaît dans la table avec un voyant **Active** vert et la colonne **Prochain déclenchement** indique la date prévue.

## Lancer manuellement et suivre

8. Pour vérifier que tout fonctionne sans attendre la prochaine échéance, passez la souris sur la ligne de l'automatisation et cliquez sur l'**icône lecture ▶︎** à droite. Une exécution démarre immédiatement et un toast confirme le lancement.
   `[SCREENSHOT: ligne d'automatisation au hover, icône Play à droite visible, tooltip "Lancer maintenant"]`

9. Pour consulter l'historique d'exécution, cliquez sur l'icône **⋯** sur la ligne (visible au hover) → **Voir l'historique**. Voir la page [Suivre l'historique d'un trigger](suivre-l-historique-d-un-trigger.md).

## Mode avancé (optionnel)

Pour les opérateurs qui préfèrent saisir directement les paramètres techniques (expression cron exacte, chemin de fichier précis, secret webhook…), le wizard propose un lien **Mode avancé** en bas à gauche. Il ouvre une fenêtre de création détaillée où vous choisissez :

- **L'assistant cible** (en haut).
- **Le type de déclenchement** parmi cinq cartes :
  - **Sur un calendrier** - quotidien, hebdomadaire, ou à heure précise.
  - **À intervalle régulier** - toutes les 30 minutes, toutes les heures, etc.
  - **Une seule fois** - à une date et heure données.
  - **Quand un fichier ou un dossier change** - surveille un fichier précis ou un dossier (option **récursif** pour inclure les sous-dossiers).
  - **Via une URL externe** - déclenché par appel HTTP entrant (webhook).
- **Les paramètres du type choisi** - voir détails ci-dessous.
- Un interrupteur **Activée** (vrai par défaut).
- Une section **Paramètres avancés** repliée par défaut : nom technique (ID) personnalisable, comportement *« si un déclenchement est déjà en cours »* (mettre en file ou ignorer), et **consigne** envoyée à l'assistant.

### Paramètres par type - mode avancé

- **Sur un calendrier** : sélectionnez un preset (*Toutes les 15 min, 30 min, Toutes les heures, Quotidien, Hebdomadaire*) ou **Personnalisé** pour saisir une expression cron brute (`min heure jour mois jour-semaine`). Les presets **Quotidien** et **Hebdomadaire** affichent un sélecteur d'heure ; **Hebdomadaire** ajoute des puces pour choisir les jours.
- **À intervalle régulier** : sélecteur d'unité + valeur (toutes les N secondes / minutes / heures).
- **Une seule fois** : un sélecteur de date + un sélecteur d'heure côte à côte.
- **Quand un fichier ou un dossier change** : un champ **Chemin surveillé** (acceptant un chemin de fichier précis **ou** un chemin de dossier), trois puces à activer/désactiver pour le type d'événement - **Création**, **Modification**, **Suppression** - et un interrupteur **Inclure les sous-dossiers**. Cet interrupteur active la surveillance récursive : tous les sous-dossiers (à n'importe quelle profondeur) sont alors également surveillés. Désactivé par défaut, il n'a d'effet que pour un chemin de dossier ; il est ignoré pour un fichier précis.
- **Via une URL externe** : un encart d'explication (l'URL de réception sera affichée après création) + un champ **Secret** avec un bouton **Générer** pour produire une clé aléatoire.

## Vérification

L'automatisation figure dans la table **Automatisations** avec :
- Un chip **Active** vert (point lumineux animé).
- Une colonne **Prochain déclenchement** indiquant la date/heure prévue.
- Une colonne **Dernière exéc** qui se remplit après le premier déclenchement.

## Si ça ne marche pas

- **L'assistant cible n'apparaît pas dans la liste** : il n'est pas installé. Allez sur **Mes assistants** et installez-le, puis rouvrez le wizard.
- **Apollia n'a pas compris ma phrase** : un message *« Nous n'avons pas pu comprendre automatiquement »* apparaît. Reformulez plus simplement en énonçant clairement la fréquence et l'heure (ex. *« Tous les jours à 9 h »*).
- **L'étape Planifier indique des points à clarifier** : l'encart orange en haut liste les ambiguïtés sur le calendrier (par exemple « heure manquante »). Tapez la précision dans le champ d'ajustement et validez avec Entrée. Si la seule chose qu'Apollia n'a pas trouvée est l'assistant, vous pouvez quand même passer à l'étape suivante : la sélection se fait à l'étape Assistant.
- **Le bouton "Activer" est désactivé** : il manque une donnée - vérifiez que l'étape Planifier n'a plus d'ambiguïté de calendrier et qu'un assistant est sélectionné.
- **Le lancement immédiat (icône ▶︎) est grisé** : l'automatisation est en pause. Réactivez-la depuis le mode avancé (interrupteur **Activée**) ou recréez-en une.

> **Référence technique :** [Briques-Triggers](https://github.com/Apollia-OS/apollia-os/wiki/Briques-Triggers) - table complète des types, paramètres acceptés et expressions cron supportées.
