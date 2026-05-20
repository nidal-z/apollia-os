# Installer un agent

> Pour tout operator qui veut ajouter un agent à Apollia : à partir d'un fichier ou d'un dossier que vous avez reçu (par e-mail, via une prestation, depuis un dépôt Git…), l'enregistrer dans l'application en quelques clics.

## Prérequis

- Un fournisseur d'IA est connecté (pastille verte dans le bandeau supérieur).
- Vous avez reçu **l'un des deux livrables suivants**, déjà présent sur votre disque :
  - **Un fichier Python seul** (par exemple `mon-agent.py`) — un agent simple qui fait une seule chose.
  - **Un dossier complet** (par exemple `mon-package/`) — un ensemble qui peut contenir plusieurs agents et leur planification.

> Apollia n'a pas (encore) de catalogue en ligne ni d'installation depuis le web : tout part d'un fichier local. La page **Connexions** sert uniquement aux serveurs MCP, pas aux agents.

## Fichier seul ou dossier : comment savoir ?

Si la personne qui vous a livré l'agent vous a remis **un seul fichier `.py`**, c'est un agent simple — utilisez le parcours **Nouvel assistant**.

Si elle vous a remis **un dossier**, ouvrez-le : s'il contient un fichier nommé `agent.toml` à sa racine, c'est un **package**. Un package permet de regrouper plusieurs agents qui travaillent ensemble (un agent principal et ses assistants) et de programmer leur déclenchement automatique (chaque matin à 7h, à chaque nouveau fichier déposé, etc.). Utilisez alors le parcours **Installer un package**.

En cas de doute, demandez à la personne qui vous a livré l'agent.

## Étapes — Installer un fichier Python seul

1. Dans la sidebar, ouvrez **Mes assistants**. La page liste vos assistants existants, et le bouton **Nouvel assistant** est en haut à droite.

   ![Page Mes assistants : liste à gauche, détail de l'agent sélectionné à droite, bouton "Nouvel assistant" en haut à droite](../_screenshots/agents-installer-un-agent-1.png)

2. En haut à droite, cliquez sur **Nouvel assistant**. Un sélecteur de fichier s'ouvre, filtré sur les fichiers `.py`.

3. Choisissez le fichier qu'on vous a livré et validez. Apollia le copie dans son dossier d'installation et enregistre l'agent.

4. Le nouvel agent apparaît dans la colonne de gauche, sous **Mes assistants**, avec une pastille grise (statut **arrêté**). Vous pouvez maintenant le démarrer.

## Étapes — Installer un package (dossier)

1. Dans la sidebar, ouvrez **Mes assistants**.

2. En haut à droite, cliquez sur **Installer un package**. Une fenêtre **Installer un package d'agents** s'ouvre.

3. Cliquez sur **Choisir un dossier** et sélectionnez le dossier qu'on vous a livré. Apollia lit son descripteur — si quelque chose cloche (dossier sans `agent.toml`, manifeste invalide), un message d'erreur vous l'indique précisément.

4. **Aperçu du package.** Apollia affiche un résumé : nom, version, auteur, la liste des agents du package, leurs déclencheurs (s'il y en a) et le nombre de dépendances. Prenez le temps de vérifier que ça correspond bien à ce que vous attendez.

   ![Dialogue d'installation, étape preview : sections Agents et Triggers, badge vert Valide](../_screenshots/agents-installer-un-agent-2.png)

   Si le package déclare un déclencheur **webhook**, une ligne supplémentaire le signale avec un badge orange « config » et le bouton du bas devient **Configurer →**.

   ![Aperçu avec un trigger webhook nécessitant une configuration, bouton Configurer →](../_screenshots/agents-installer-un-agent-2bis.png)

5. Cliquez sur **Installer**. Si le package contient des déclencheurs **webhook** à paramétrer, le bouton affiche **Configurer →** à la place, voir l'étape suivante.

6. **(Optionnel) Configuration des webhooks.** Si on vous le demande, chaque webhook nécessite un **secret** (au moins 32 caractères) qui sécurise les appels entrants. Trois cas :
   - Si la personne qui a préparé le package vous a fourni un secret, copiez-le dans le champ.
   - Sinon, générez-en un long et imprévisible (n'importe quel mot de passe robuste fait l'affaire) et conservez-le précieusement, vous en aurez besoin pour configurer le service qui appellera le webhook.
   - L'URL affichée au-dessus du champ est l'adresse à laquelle ce webhook répondra : copiez-la avec le bouton dédié.

   ![Dialogue d'installation, étape configure : carte d'un trigger webhook avec endpoint URL et champ secret HMAC-SHA256](../_screenshots/agents-installer-un-agent-3.png)

7. Cliquez sur **Installer**. Apollia copie le package, enregistre les agents et active leurs déclencheurs. Un écran final confirme l'installation avec le nombre d'agents et de déclencheurs créés.

   ![Écran de confirmation Package installé ! avec compteur agents et triggers, bouton Fermer](../_screenshots/agents-installer-un-agent-4.png)

8. Fermez le dialogue. Le package apparaît dans la colonne de gauche, sous **Mes packages**. Les agents qu'il contient sont aussi listés sous **Mes assistants** (sauf ceux qui sont uniquement appelés en interne par d'autres agents).

## Vérification

- Un fichier seul → l'agent apparaît sous **Mes assistants** avec une pastille grise.
- Un package → la carte du package apparaît sous **Mes packages** avec un compteur du type `0/2 agents · 0/1 triggers`. Cliquez dessus pour voir le détail.
- Le bouton **Démarrer** (icône lecture) à droite de la ligne est actif.

Pour la suite, voir [Démarrer un agent](demarrer-un-agent.md).

## Si ça ne marche pas

- **« Le dossier doit contenir un fichier `agent.toml` »** : vous avez sans doute sélectionné un dossier parent. Ouvrez le dossier livré et cherchez à quel niveau se trouve `agent.toml` — c'est ce niveau-là qu'il faut sélectionner.
- **Badge rouge « Invalide » dans l'aperçu** : le descripteur du package contient une erreur. Le message rouge sous le badge précise laquelle. Renvoyez-le à la personne qui a préparé le package, c'est à elle de le corriger.
- **« Le secret doit faire au moins 32 caractères »** : votre secret est trop court. Tapez (ou collez) une chaîne plus longue.
- **L'agent installé n'apparaît pas** : l'enregistrement a échoué silencieusement. Ouvrez les logs depuis la carte pour lire l'erreur précise.
- **Avertissements « trigger » sur l'écran final** : l'agent est installé, mais certains de ses déclencheurs n'ont pas pu être activés. Notez le détail affiché et signalez-le à la personne qui a préparé le package.

> **Pour les profils techniques :** [Briques-Tool-Registry](https://github.com/nidal-z/apollia-os/wiki/Briques-Tool-Registry) (format `agent.toml`, outils natifs activables, structure d'un package).
