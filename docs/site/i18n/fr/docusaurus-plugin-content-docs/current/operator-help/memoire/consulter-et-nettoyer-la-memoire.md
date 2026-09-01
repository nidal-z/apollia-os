---
title: Consulter et nettoyer la mémoire
slug: /operator-help/memory/review-and-clean-up-memory
sidebar_position: 2
---

# Consulter et nettoyer la mémoire

> Pour les operators qui veulent voir ce que leurs IA ont retenu, et supprimer ce qui ne devrait plus être là.

## Prérequis

- Au moins un agent qui a déjà conversé ou exécuté une tâche.

## Où trouver quoi

Deux endroits distincts, deux usages distincts :

- **Paramètres → Profil** - votre profil utilisateur (prénom, rôle, secteur, supervision des agents, souveraineté…). C'est ce que tous vos agents savent de vous. Voir [Mon profil](gerer-mon-profil.md).
- **Mémoire** (depuis la sidebar) - l'explorateur des **namespaces mémoire** par agent et par projet. C'est ce que chaque agent retient pour lui-même (épisodes de conversation, faits sémantiques, procédures apprises).

Cette page couvre la deuxième : naviguer dans les namespaces mémoire et y supprimer des entrées.

## Étapes

1. Dans la sidebar, cliquez sur **Mémoire**. La page affiche un layout deux colonnes : sidebar des namespaces à gauche, panneau central avec la liste des entrées du namespace sélectionné.

   ![Page Mémoire : la liste des namespaces à gauche, les filtres par type et la recherche au centre, et les entrées de la mémoire du profil utilisateur](/img/operator-help/memoire-consulter-et-nettoyer-la-memoire-1.png)

2. **Sidebar gauche** - la liste des **namespaces** (un namespace = un espace mémoire isolé). Chaque namespace est classifié automatiquement :
   - **Profil** : votre profil utilisateur partagé (`__user__`). En lecture seule depuis cette page - l'édition se fait dans **Paramètres → Profil**. Une bannière le rappelle quand vous sélectionnez `__user__`.
   - **Agents** : namespaces d'agents installés (un par agent - ex: `veille-ia`, `email-triage`).
   - **Projets** : namespaces scopés à un projet (format `{project_id}:{ns}`).
   - **Autres** : namespaces legacy ou d'agents désinstallés.

   Un **segmented control** en haut permet de filtrer la liste par catégorie. Un champ **Filtrer…** permet de retrouver un namespace par nom.

3. **Panneau central** - la liste des entrées du namespace sélectionné, avec :
   - Un **segmented control** par type d'entrée : **Toutes / Épisodique / Sémantique / Procédurale**.
   - Une **barre de recherche** plein texte (FTS5) qui interroge le contenu.
   - Un **breadcrumb** sous le filtre qui rappelle le namespace courant.
   - Chaque ligne montre l'icône du type, la clé, un aperçu de la valeur, et la date relative.

4. **Cliquez sur une entrée** pour ouvrir le **panneau de détail** à droite. Il affiche la valeur complète (avec pretty-print JSON automatique si applicable), toutes les métadonnées (type, namespace, ID, dates, score BM25 en mode recherche), et expose deux actions : **Copier** la valeur et **Supprimer** l'entrée.

   ![Panneau de détail d'une entrée de mémoire, avec sa valeur, ses métadonnées et les actions Copier et Supprimer](/img/operator-help/memoire-consulter-et-nettoyer-la-memoire-2.png)

5. Pour **rechercher**, tapez quelques mots-clés dans la **barre de recherche** en haut du panneau central. Les entrées correspondantes s'affichent triées par pertinence (score BM25), et le breadcrumb indique « *N résultats* ».

6. Pour **supprimer une entrée** précise, deux options équivalentes :
   - Survolez la ligne, cliquez sur le menu **⋯** en bout de ligne, choisissez **Supprimer**, puis confirmez sur le bouton **Confirmer** qui apparaît.
   - Ou ouvrez le panneau de détail (click sur la ligne) et utilisez le bouton **Supprimer** en bas.

   L'entrée disparaît immédiatement et ne reviendra pas.

## Exporter, importer et purger un namespace

Trois boutons sont posés en haut à droite du panneau central, à côté du champ de recherche : **Exporter**, **Importer** et **Purger**. Ils agissent sur le **namespace sélectionné** et restent inactifs tant qu'aucun n'est sélectionné.

### Exporter

Cliquez sur **Exporter**. Une fenêtre d'enregistrement s'ouvre avec un nom déjà proposé (`<namespace>-memory-<date>.json`). Validez, et Apollia écrit un fichier JSON contenant les entrées épisodiques, sémantiques et procédurales du namespace. La confirmation indique combien d'entrées de chaque type ont été écrites, et où.

Rien n'est envoyé nulle part : le fichier arrive exactement là où vous l'avez demandé, sur votre machine.

### Importer

Cliquez sur **Importer**. La fenêtre demande deux choses :

- **Fichier source** : un fichier JSON produit par un export Apollia, choisi via **Choisir un fichier**.
- **Stratégie** : **Fusionner** ajoute les entrées manquantes et laisse les existantes intactes ; **Remplacer** vide d'abord le namespace, puis charge le fichier.

**Remplacer** affiche un avertissement rouge dans la fenêtre et demande une seconde confirmation (**Vider et importer**) avant toute suppression. Fusionner part directement du bouton **Importer**. La confirmation dit combien d'entrées ont été chargées et sous quelle stratégie, et la liste en dessous se recharge.

### Purger par ancienneté

Cliquez sur **Purger**. C'est la suppression en masse, elle est irréversible, donc elle se fait en deux temps.

1. Choisissez le **type de mémoire** (Tous les types, Épisodique, Sémantique, Procédurale) et l'ancienneté en jours sous **Plus ancien que (jours)**. `0` supprime toutes les entrées du type choisi.

2. Sous les champs, un aperçu indique combien d'entrées listées partiraient. Il est compté sur les entrées que cette page lit, et l'écran le dit : le chiffre exact est celui annoncé une fois la purge exécutée.

3. Cliquez sur **Continuer**, relisez le récapitulatif (type, namespace, ancienneté), puis confirmez avec **Purger**. Un message annonce le nombre d'entrées réellement supprimées.

## Vérification

L'entrée supprimée n'apparaît plus dans la liste, même après recharge de la page. Une recherche par mot-clé sur cette entrée ne retourne plus de résultat. Le compteur du namespace dans la sidebar et le compteur du type dans le segmented control sont décrémentés.

## Si ça ne marche pas

- **La page est vide** : aucun agent n'a encore généré de mémoire. Lancez une conversation et revenez.
- **La suppression échoue** : l'agent est en train d'écrire dans la mémoire, attendez quelques secondes et réessayez.
- **Le namespace attendu n'apparaît pas** : vérifiez que l'agent est bien installé (l'agent doit être listé dans **Agents** pour apparaître sous la catégorie *Agents*, sinon le namespace bascule en *Autres*).
- **Vous voulez vider tout un namespace** : utilisez **Purger** avec **Tous les types** et `0` jour. Pour tout effacer d'un coup, voir [Réinitialiser Apollia (factory reset)](../troubleshooting/reinitialiser-apollia-factory-reset.md), ou utilisez la CLI : `apollia-os memory clear --agent <NAME> --confirm`.
- **L'aperçu de purge annonce moins d'entrées que la purge n'en supprime** : l'aperçu est compté sur la liste que cette page lit, qui s'arrête aux 500 épisodes de conversation les plus récents. Sur un gros namespace, le chiffre réel est plus élevé, et c'est celui annoncé à la fin.
- **Les boutons Exporter, Importer et Purger sont grisés** : aucun namespace n'est sélectionné. Choisissez-en un dans la sidebar de gauche.

> **Note** : Pour gérer les **outils** disponibles à vos agents (recherche web, lecture de fichiers, etc.), ouvrez **Paramètres → Outils**. La page propose le détail de chaque outil, son activation/désactivation, sa configuration éventuelle et son contrat, voir [Inspecter un outil](../controle/inspecter-un-outil.md).

> **Référence technique :** [Référence Apollia](/reference) - types de mémoire, durées de rétention par défaut, limites.
