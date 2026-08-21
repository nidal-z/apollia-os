---
title: Créer un projet
sidebar_position: 1
---

# Créer un projet

> Pour les operators qui veulent regrouper un dossier de travail, ses fichiers et ses chats sous une même enveloppe réutilisable.

## Prérequis

- Un fournisseur d'IA connecté (la connexion est verte dans le bandeau supérieur).
- Vous savez quel travail vous allez piloter depuis ce projet (site, étude, dossier client…).
- Aucun dossier à préparer : Apollia déduit le dossier de travail du nom du projet et le crée s'il n'existe pas encore.

## Étapes

1. Dans la sidebar, cliquez sur **Projets**.

2. Cliquez sur **+ Nouveau projet** en haut à droite.
   ![page Projets, bouton + Nouveau projet surligné en haut à droite](/img/operator-help/projets-creer-un-projet-1.png)

3. L'étape 1 sur 2, **Partir d'un template ?**, propose deux cartes. Choisissez-en une, puis cliquez sur **Continuer**.
   - **Projet vierge** : aucune configuration prédéfinie.
   - **Projet développeur** : pré-active les context providers orientés code (Git, arborescence, APOLLIA.md).

4. À l'étape 2 sur 2, donnez un **nom** clair à votre projet (par exemple : *Site marketing 2026*). Ce nom apparaîtra dans la sidebar, dans les chats liés et dans les notifications. Une **description** et une **couleur** sont également proposées, toutes deux facultatives.
   ![modal Nouveau projet, ouverte sur l'étape où vous nommez le projet](/img/operator-help/projets-creer-un-projet-2.png)

5. Cliquez sur **Créer le projet**. Le projet apparaît immédiatement dans la liste. Apollia déduit son dossier de travail du nom, sous `Apollia/` dans votre dossier personnel ou dans vos documents, et crée ce dossier.

6. Cliquez sur le projet dans la colonne de gauche pour ouvrir son **panneau de détail** à droite. Un en-tête porte le nom du projet, le nombre d'agents liés et le bouton **Nouveau chat** ; en dessous, une barre de six onglets : **Conversations**, **Tâches**, **Agents**, **Mémoire**, **Contexte**, **Paramètres**.
   ![Panneau de detail du projet, avec son en-tete et ses six onglets, ouvert sur l onglet Conversations](/img/operator-help/projets-creer-un-projet-3.png)

7. Continuez avec **Activer les context providers** pour charger automatiquement les bonnes informations dans vos futurs chats.

## Rattacher un agent au projet

L'onglet **Agents** porte le lien entre un projet et les assistants que vous avez installés. Son compteur est celui affiché dans l'en-tête.

1. Ouvrez le projet, puis l'onglet **Agents**.

2. Choisissez un assistant dans la liste **Choisir un agent à rattacher...**. Seuls les assistants installés y figurent, et ceux déjà rattachés ne sont pas proposés deux fois.

3. Cliquez sur **Ajouter un agent**. L'assistant rejoint la liste, avec sa description et une pastille verte quand il tourne.

4. Pour en détacher un, cliquez sur la **✕** en bout de ligne, puis confirmez sur le bouton **Retirer** qui remplace la description. Rien n'est désinstallé, seul le lien disparaît.

Ce que le rattachement fait, et ce qu'il ne fait pas : il regroupe l'assistant sous le projet et restreint l'onglet **Tâches** à ses tâches. Il ne change rien à son exécution. Les instructions du projet, ses documents et ses context providers sont injectés dans les **conversations ouvertes depuis ce projet**, jamais dans la tâche d'un agent.

## Attacher un document

L'onglet **Mémoire** contient deux sections empilées : les documents attachés au projet, et les namespaces mémoire scopés à ce projet.

1. Ouvrez le projet, puis l'onglet **Mémoire**.

2. Cliquez sur **Attacher un fichier**, à droite du titre **Documents en mémoire**. Le sélecteur de fichiers du système s'ouvre.

3. Choisissez le fichier. Apollia enregistre son nom, son chemin et sa taille. **Le fichier n'est pas copié** : il reste où il est, et Apollia le relit à ce chemin chaque fois qu'il construit le contexte du projet.

4. Pour en détacher un, cliquez sur la **✕** en bout de ligne, puis confirmez. Le message le dit clairement : le fichier reste sur le disque, seule la référence du projet disparaît.

À quoi sert un document : quand vous ouvrez un chat depuis le projet, le contenu de chaque document attaché est lu sur le disque et ajouté au contexte de cette conversation, sous le nom du document. Un fichier déplacé ou supprimé est ignoré silencieusement, et un document long est coupé au-delà de 10 000 octets avec une marque *[truncated]*. Un fichier qu'Apollia ne sait pas lire comme du texte n'apporte rien à la conversation.

## Vérification

Le projet est listé dans la sidebar sous **Projets** et son onglet **Paramètres** affiche le dossier de travail qu'Apollia a créé pour lui.

## Si ça ne marche pas

- **Rien ne se passe quand vous cliquez sur Créer le projet** : le nom est obligatoire, un message le signale en haut à droite quand le champ est vide.
- **Le dossier de travail n'est pas celui que vous vouliez** : ouvrez le projet, allez dans son onglet **Paramètres** et choisissez-en un autre avec **Choisir un dossier…**.
- **Le projet apparaît mais reste vide** : c'est normal, les fournisseurs de contexte s'activent à l'étape suivante.
- **L'onglet Tâches dit qu'il faut au moins un agent** : aucun assistant n'est encore rattaché. Le bouton de cet écran vide mène directement à l'onglet **Agents**.
- **La liste des agents ne propose rien à rattacher** : tous les assistants installés le sont déjà, la liste le dit au lieu de rester vide. Installez-en un autre depuis **Mes assistants**.
- **Un agent rattaché affiche « Agent non installé sur cette machine »** : le lien a survécu à la désinstallation de l'assistant. Détachez-le, ou réinstallez l'assistant sous le même nom.
- **Un document n'apporte rien à la conversation** : vérifiez que le fichier est toujours au chemin qu'il avait au moment du rattachement, et que c'est bien un fichier texte. Apollia ignore ce qu'il ne sait pas lire, sans erreur.

> **Concept :** [Explication Apollia](/explanation) - comprendre pourquoi un projet sert d'enveloppe contextuelle pour vos chats.
