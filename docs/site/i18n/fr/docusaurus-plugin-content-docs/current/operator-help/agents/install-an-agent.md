---
title: Installer un agent
slug: /operator-help/agents/install-an-agent
sidebar_position: 1
---

# Installer un agent

> Pour toute personne qui veut un agent de plus dans Apollia : installer un paquet reçu, partir d'un exemple, ou créer un agent en décrivant le travail.

## Prérequis

- Pour installer un paquet : le fichier du paquet, dont le nom se termine par `.apo`, est sur ce poste.
- Pour créer un agent : un modèle répond. Les modèles se règlent dans l'écran **IA**.

## Trois façons d'obtenir un agent

L'écran **Agents** liste vos agents dans une colonne et vos workflows dans l'autre. Un nouvel agent arrive de l'une de ces trois façons :

- **Installer un paquet** : quelqu'un vous a remis un fichier `.apo`. Un paquet apporte d'un coup un ou plusieurs agents, workflows et outils.
- **Partir d'un exemple** : Apollia propose des agents prêts à l'emploi, installés seulement si vous le choisissez.
- **Créer un agent** : vous dites à quoi sert l'agent et Apollia rédige sa fiche.

## Étapes - Installer un paquet

1. Dans la barre latérale, ouvrez **Agents**.

2. En haut à droite, cliquez sur **Installer un paquet**. Un sélecteur de fichiers s'ouvre, limité aux fichiers `.apo`.

3. Choisissez le fichier. Une fenêtre d'aperçu s'ouvre. Rien n'est installé avant votre accord.

4. Lisez l'aperçu :
   - **Éditeur** : qui a produit le paquet, et si sa signature le confirme. **Faire confiance à cet éditeur** ajoute un éditeur signé à vos éditeurs de confiance.
   - **Ce que le paquet apporte** : les agents, workflows et outils qu'il contient.
   - **Accès demandés, par famille** : ce que ses agents ont le droit de faire (lire, écrire ou supprimer, envoyer vers l'extérieur, exécuter du code). Les familles sensibles sont signalées.
   - **Connexions** : les connexions dont le paquet a besoin, chacune marquée connectée ou manquante. Une connexion manquante s'ajoute maintenant ou plus tard.

5. Cliquez sur **Installer**. Pour un paquet non signé, la fenêtre le dit et le bouton devient **Installer quand même** : ne continuez que si vous savez d'où vient le fichier.

6. Un message confirme l'installation. Les agents du paquet apparaissent dans la colonne **Agents** et ses workflows dans la colonne **Workflows**.

Ouvrir le fichier d'une version plus récente d'un paquet déjà installé le met à jour. La fenêtre met alors en avant les accès nouveaux et les connexions nouvelles, à cocher avant de cliquer sur **Mettre à jour**. Les réglages faits sur la fiche de l'agent sont conservés.

## Étapes - Partir d'un exemple

1. Dans la barre latérale, ouvrez **Agents**.

2. Sous les deux colonnes, cliquez sur **Voir les exemples**. Tant que vous n'avez ni agent ni workflow, les exemples sont déjà affichés.

3. Sur la carte d'un exemple, cliquez sur **Installer**. Le même aperçu que pour tout paquet s'ouvre.

4. Une fois l'exemple installé, sa carte propose **Ouvrir la fiche**. Certains exemples proposent aussi **Essayer avec l'exemple** : Apollia copie un jeu de démonstration inventé dans le dossier que vous choisissez, pour voir l'agent travailler sans donnée réelle.

## Étapes - Créer un agent

1. Dans la barre latérale, ouvrez **Agents**, puis cliquez sur **Créer un agent** en haut à droite.

2. Remplissez le formulaire. Seul le premier champ est nécessaire :
   - **À quoi sert cet agent ?** : décrivez le travail comme vous le confieriez à un collègue.
   - **Ce qu'il doit vous rendre** : le contenu et la forme de sa réponse.
   - **Ce qu'il doit toujours ou ne jamais faire** : les règles auxquelles il ne déroge pas.
   - **Des demandes qu'il recevra** : une par ligne, telles que vous les écririez.
   - **Ce qu'il peut consulter** : les sources de ce poste, dont votre profil et une mémoire qui lui est propre.
   - **Ce qu'il peut faire, en plus de lire** : sans rien cocher, l'agent lit et répond.

3. Cliquez sur **Préparer la fiche**. Apollia lit votre demande et vous pose jusqu'à trois questions sur ce que le formulaire ne dit pas. Chaque question arrive avec des réponses à choisir. Vous pouvez aussi écrire votre réponse, ou cliquer sur **Passer**.

4. Le brouillon de la fiche s'ouvre sous **Relire le brouillon**. Donnez un **Nom** à l'agent et ajustez ce qu'Apollia a rédigé : mission, modèle, accès et sources restent tous modifiables. **Revenir au formulaire** ramène au formulaire.

5. Cliquez sur **Enregistrer l'agent**. Rien n'est enregistré avant ce clic : quitter l'écran plus tôt ne laisse aucun agent.

6. La fiche du nouvel agent s'ouvre. **Essayer dans le Chat** ouvre une conversation avec lui.

Pour rédiger la fiche sans les questions, cliquez sur **Remplir la fiche moi-même** dans le formulaire.

## Vérification

- L'agent figure dans la colonne **Agents**, et sa fiche porte la mention **disponible**.
- Sa ligne propose **Discuter**, et sa fiche s'ouvre quand vous cliquez sur la ligne.

Pour la suite, voir [Confier une tâche à un agent](start-an-agent.md).

## Retirer un agent

1. Ouvrez **Agents** et cliquez sur la ligne de l'agent pour ouvrir sa fiche.

2. Cliquez sur **Désinstaller** en haut de la fiche. L'en-tête devient une confirmation qui nomme l'agent.

3. Cochez **Supprimer aussi la mémoire et les données de l'agent** si sa mémoire doit partir avec lui.

4. Cliquez sur **Supprimer**. L'agent quitte la liste.

L'agent intégré **Apollia Chat** suit la version de l'application : il ne se désinstalle pas.

## Si ça ne marche pas

- **« Ce fichier ne peut pas être lu comme un paquet. »** : le fichier n'est pas un paquet, ou il est abîmé. Demandez une nouvelle copie à la personne qui vous l'a envoyé.
- **Installation refusée, signature invalide** : le fichier a pu être modifié après sa signature. Rien n'a été installé. Demandez une nouvelle copie à son éditeur.
- **Installation refusée par votre organisation** : la politique de votre organisation exige des paquets signés, ou ne reconnaît pas cet éditeur. **Voir la politique** ouvre les règles en vigueur.
- **« Un agent porte déjà ce nom »** : un agent du même nom existe sur ce poste. La fenêtre dit s'il a été créé dans l'application ou installé par un autre paquet.
- **« Cette version est plus ancienne »** : le fichier contient une version plus ancienne que celle installée. Rien n'est modifié.
- **« Le modèle par défaut ne répond pas »** à la création d'un agent : démarrez le serveur nommé dans le message, ou choisissez un autre modèle par défaut avec **Ouvrir l'écran IA**.
- **« La fiche n'a pas pu être préparée »** : relancez, ou cliquez sur **Remplir la fiche moi-même**.
- **Une source, un accès ou un modèle figure sous « Absent de ce poste »** : ajoutez ce qui manque dans **Connexions**, puis accordez-le sur la fiche de l'agent.

> **Pour les profils techniques :** [Référence Apollia](/reference) (format d'un paquet, outils natifs, ligne de commande).
