# Créer un projet

> Pour les operators qui veulent regrouper un dossier de travail, ses fichiers et ses chats sous une même enveloppe réutilisable.

## Prérequis

- Un dossier existant sur votre machine (un repo, un workspace, un dossier de travail).
- Un fournisseur d'IA connecté (la connexion est verte dans le bandeau supérieur).
- Vous savez quel travail vous allez piloter depuis ce projet (site, étude, dossier client…).

## Étapes

1. Dans la sidebar, cliquez sur **Projets**.

2. Cliquez sur **+ Nouveau projet** en haut à droite.
   `[SCREENSHOT: page Projets, bouton + Nouveau projet surligné en haut à droite]`

3. Donnez un **nom** clair à votre projet (par exemple : *Site marketing 2026*). Ce nom apparaîtra dans la sidebar, dans les chats liés et dans les notifications.

4. Cliquez sur **Parcourir** et sélectionnez le **dossier racine** du projet sur votre machine.

5. (Optionnel) Choisissez un **modèle de projet** dans la liste déroulante. Le modèle pré-active les fournisseurs de contexte adaptés (un projet de code aura par défaut Git et Arborescence).
   `[SCREENSHOT: modal Nouveau projet avec champs Nom, Dossier racine, Modèle]`

6. Cliquez sur **Créer**. Le projet apparaît immédiatement dans la liste.

7. Cliquez sur la carte du projet pour ouvrir sa **page de détail**. Vous y voyez le chemin, la date de création, le nombre de documents attachés et les onglets pour configurer la suite.
   `[SCREENSHOT: page de détail projet avec onglets Vue d'ensemble, Context providers, Chats liés]`

8. Continuez avec **Activer les context providers** pour charger automatiquement les bonnes informations dans vos futurs chats.

## Vérification

Le projet est listé dans la sidebar sous **Projets** et sa page de détail affiche bien le dossier racine que vous avez choisi.

## Si ça ne marche pas

- **Le dossier n'apparaît pas dans le sélecteur** : vérifiez les permissions du dossier (Apollia doit pouvoir le lire).
- **Le bouton Créer reste grisé** : un nom et un dossier sont obligatoires, le modèle est facultatif.
- **Le projet apparaît mais reste vide** : c'est normal, les fournisseurs de contexte s'activent à l'étape suivante.

> **Concept :** [book ch12 — Chat interactif](https://github.com/nidal-z/apollia-os/blob/main/book/src/ch12-00-chat-interactif.md) — comprendre pourquoi un projet sert d'enveloppe contextuelle pour vos chats.
