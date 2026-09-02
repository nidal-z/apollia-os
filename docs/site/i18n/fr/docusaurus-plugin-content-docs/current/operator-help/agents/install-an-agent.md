---
title: Installer un agent
slug: /operator-help/agents/install-an-agent
sidebar_position: 1
---

# Installer un agent

> Pour tout operator qui veut ajouter un agent à Apollia : à partir d'un fichier ou d'un dossier que vous avez reçu (par e-mail, via une prestation, depuis un dépôt Git…), l'enregistrer dans l'application en quelques clics.

## Prérequis

- Un fournisseur d'IA est connecté (pastille verte dans le bandeau supérieur).
- Vous avez reçu **l'un des deux livrables suivants**, déjà présent sur votre disque :
  - **Un fichier Python seul** (par exemple `mon-agent.py`) - un agent simple qui fait une seule chose.
  - **Un dossier complet** (par exemple `mon-package/`) - un ensemble qui peut contenir plusieurs agents et leur planification.

> Apollia n'a pas (encore) de catalogue en ligne ni d'installation depuis le web : tout part d'un fichier local. La page **Connexions** sert uniquement aux serveurs MCP, pas aux agents.

## Fichier seul ou dossier : comment savoir ?

Si la personne qui vous a livré l'agent vous a remis **un seul fichier `.py`**, c'est un agent simple - utilisez le parcours **Nouvel assistant**.

Si elle vous a remis **un dossier**, ouvrez-le : s'il contient un fichier nommé `agent.toml` à sa racine, c'est un **package**. Un package permet de regrouper plusieurs agents qui travaillent ensemble (un agent principal et ses assistants) et de programmer leur déclenchement automatique (chaque matin à 7h, à chaque nouveau fichier déposé, etc.). Utilisez alors le parcours **Installer un package**.

En cas de doute, demandez à la personne qui vous a livré l'agent.

## Étapes - Installer un fichier Python seul

1. Dans la sidebar, ouvrez **Mes assistants**. La page liste vos assistants existants, et le bouton **Nouvel assistant** est en haut à droite.

   ![Page Mes assistants : liste à gauche, détail de l'agent sélectionné à droite, bouton "Nouvel assistant" en haut à droite](/img/operator-help/agents-installer-un-agent-1.png)

2. En haut à droite, cliquez sur **Nouvel assistant**. Un sélecteur de fichier s'ouvre, filtré sur les fichiers `.py`.

3. Choisissez le fichier qu'on vous a livré et validez. Apollia le copie dans son dossier d'installation et enregistre l'agent.

4. Le nouvel agent apparaît dans la colonne de gauche, sous **Mes assistants**, avec une pastille grise (statut **arrêté**). Vous pouvez maintenant le démarrer.

## Étapes - Installer un package (dossier)

1. Dans la sidebar, ouvrez **Mes assistants**.

2. En haut à droite, cliquez sur **Installer un package**. Une fenêtre **Installer un package d'agents** s'ouvre.

3. Cliquez sur **Choisir un dossier** et sélectionnez le dossier qu'on vous a livré. Apollia lit son descripteur - si quelque chose cloche (dossier sans `agent.toml`, manifeste invalide), un message d'erreur vous l'indique précisément.

4. **Aperçu du package.** Apollia affiche un résumé : nom, version, auteur, la liste des agents du package, leurs déclencheurs (s'il y en a) et le nombre de dépendances. Prenez le temps de vérifier que ça correspond bien à ce que vous attendez.

   Certains packages déclarent des vérifications : aux paliers `supervised` et supérieurs, le runtime contrôle automatiquement que l'agent a produit le résultat attendu. C'est prévu par l'auteur du package, vous n'avez rien à configurer.

   ![Dialogue d'installation, étape preview : sections Agents et Triggers, badge vert Valide](/img/operator-help/agents-installer-un-agent-2.png)

   Si le package déclare un déclencheur **webhook**, une ligne supplémentaire le signale avec un badge orange « config » et le bouton du bas devient **Configurer →**.

   ![Aperçu avec un trigger webhook nécessitant une configuration, bouton Configurer →](/img/operator-help/agents-installer-un-agent-2bis.png)

5. Cliquez sur **Installer**. Si le package contient des déclencheurs **webhook** à paramétrer, le bouton affiche **Configurer →** à la place, voir l'étape suivante.

6. **(Optionnel) Dépendances Python.** Si le package déclare des dépendances pip, un écran de confirmation les liste avant que quoi que ce soit ne soit téléchargé. Rien n'est installé tant que vous n'avez pas confirmé.

   ![Dialogue d'installation, étape de confirmation des dépendances : encart ambre, liste des paquets pip, note sur le venv](/img/operator-help/agents-installer-un-agent-2ter.png)

   Les paquets viennent de [pypi.org](https://pypi.org) et atterrissent dans un environnement virtuel dédié à cet agent, sous `~/.apollia/venvs/`. Votre Python système n'est pas touché, et désinstaller le package les emporte avec lui. Lisez la liste : c'est le seul moment où vous voyez exactement quel code tiers l'agent va exécuter.

7. **(Optionnel) Configuration des webhooks.** Si on vous le demande, chaque webhook nécessite un **secret** (au moins 32 caractères) qui sécurise les appels entrants. Trois cas :
   - Si la personne qui a préparé le package vous a fourni un secret, copiez-le dans le champ.
   - Sinon, générez-en un long et imprévisible (n'importe quel mot de passe robuste fait l'affaire) et conservez-le précieusement, vous en aurez besoin pour configurer le service qui appellera le webhook.
   - L'URL affichée au-dessus du champ est l'adresse à laquelle ce webhook répondra : copiez-la avec le bouton dédié.

   ![Dialogue d'installation, étape configure : carte d'un trigger webhook avec endpoint URL et champ secret HMAC-SHA256](/img/operator-help/agents-installer-un-agent-3.png)

8. Cliquez sur **Installer**. Apollia copie le package, enregistre les agents et active leurs déclencheurs. Un écran final confirme l'installation avec le nombre d'agents et de déclencheurs créés.

   ![Écran de confirmation Package installé ! avec compteur agents et triggers, bouton Fermer](/img/operator-help/agents-installer-un-agent-4.png)

9. Fermez le dialogue. Le package apparaît dans la colonne de gauche, sous **Mes packages**. Les agents qu'il contient sont aussi listés sous **Mes assistants** (sauf ceux qui sont uniquement appelés en interne par d'autres agents).

## Vérification

- Un fichier seul → l'agent apparaît sous **Mes assistants** avec une pastille grise.
- Un package → la carte du package apparaît sous **Mes packages** avec un compteur du type `0/2 agents · 0/1 triggers`. Cliquez dessus pour voir le détail.
- Le bouton **Démarrer** (icône lecture) à droite de la ligne est actif.

Pour la suite, voir [Démarrer un agent](start-an-agent.md).

## Remplacer le fichier d'un agent

Un agent déjà installé accepte une nouvelle version de son fichier Python sans passer par une désinstallation.

1. Ouvrez **Mes assistants** et sélectionnez l'agent dans la colonne de gauche.

2. En haut à droite du panneau de détail, cliquez sur **Mettre à jour**. Un sélecteur de fichiers s'ouvre, filtré sur les fichiers `.py`, le même que pour l'installation.

3. Choisissez le nouveau fichier. Apollia le valide **avant** d'écrire quoi que ce soit : un module que le runtime refuse laisse l'agent installé exactement en l'état.

4. Si l'agent tourne, l'en-tête passe en avertissement : remplacer son fichier l'arrête puis le relance sur la nouvelle version. Confirmez avec **Remplacer et redémarrer**, ou annulez. Un agent arrêté saute cette étape, il n'y a rien à interrompre.

Ce qui est conservé : le dossier d'installation, le démarrage automatique et la date d'installation. Ce qui change : le fichier lui-même, les fichiers `.py` et les sous-dossiers Python situés à côté de lui, et la version, lue dans le nouveau module.

Le message de confirmation dit quelle version répond :

- « *… mis à jour en vX. La nouvelle version sera chargée au prochain démarrage.* » pour un agent qui était arrêté.
- « *… mis à jour en vX et redémarré sur la nouvelle version.* » quand le redémarrage est passé.

Si le redémarrage a échoué, un bandeau rouge remplace la confirmation et nomme le cas : soit le fichier est installé mais l'agent n'a pas pu être arrêté, et c'est la version **précédente** qui répond toujours, soit il a été arrêté et n'est pas remonté, et plus rien ne répond. La cause brute est derrière **Détails techniques**, dans le bandeau.

## Retirer un agent

1. Ouvrez **Mes assistants** et sélectionnez l'agent.

2. Cliquez sur **Désinstaller** en haut à droite. L'en-tête se transforme en confirmation : « *Supprimer définitivement « nom » ?* ».

3. Cochez **Supprimer aussi la mémoire et les données de l'agent** si sa mémoire doit partir avec lui. Décoché, la mémoire reste sur le disque et se retrouve dans la catégorie *Autres* de la page **Mémoire**, puisque l'agent qui la nommait n'existe plus.

4. Cliquez sur **Supprimer**. La ligne disparaît de la liste, l'entrée en base et le dossier d'installation partent avec elle.

Les deux confirmations occupent le même coin de l'en-tête : armer la désinstallation abandonne un fichier de remplacement que vous veniez de choisir. Répondez à une seule à la fois.

## Si ça ne marche pas

- **« Le dossier doit contenir un fichier `agent.toml` »** : vous avez sans doute sélectionné un dossier parent. Ouvrez le dossier livré et cherchez à quel niveau se trouve `agent.toml` - c'est ce niveau-là qu'il faut sélectionner.
- **Badge rouge « Invalide » dans l'aperçu** : le descripteur du package contient une erreur. Le message rouge sous le badge précise laquelle. Renvoyez-le à la personne qui a préparé le package, c'est à elle de le corriger.
- **« Le secret doit faire au moins 32 caractères »** : votre secret est trop court. Tapez (ou collez) une chaîne plus longue.
- **L'agent installé n'apparaît pas** : l'enregistrement a échoué silencieusement. Ouvrez les logs depuis la carte pour lire l'erreur précise.
- **Le nouveau fichier est refusé lors de la mise à jour** : le bandeau montre ce que le chargeur a reproché, sous **Détails techniques**. Rien n'a été écrit, l'agent tourne toujours sur son fichier précédent. Renvoyez le message à la personne qui a préparé l'agent.
- **Après une mise à jour, l'agent se comporte comme avant** : relisez la confirmation. Sur un agent arrêté, le nouveau fichier n'est chargé qu'au démarrage suivant, et un redémarrage en échec le dit explicitement.
- **Avertissements « trigger » sur l'écran final** : l'agent est installé, mais certains de ses déclencheurs n'ont pas pu être activés. Notez le détail affiché et signalez-le à la personne qui a préparé le package.

> **Pour les profils techniques :** [Référence Apollia](/reference) (format `agent.toml`, outils natifs activables, structure d'un package).
