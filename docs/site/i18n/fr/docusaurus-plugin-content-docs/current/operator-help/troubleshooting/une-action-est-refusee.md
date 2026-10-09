---
title: Une action a été refusée
slug: /operator-help/troubleshooting/an-action-was-refused
sidebar_position: 3
---

# Une action a été refusée

> Pour toute personne dont l'agent signale une action refusée, ou semble avoir abandonné une tâche : comprendre pourquoi et débloquer la suite.

## Vérifications rapides (par ordre de probabilité)

### 1. Vous avez vous-même refusé l'action récemment

Chaque refus est transmis à l'agent, avec la raison quand vous en avez saisi une, et l'agent s'adapte ou s'arrête. C'est le comportement normal.

**Solution :**
1. Dans la barre latérale, cliquez sur **Journal**.
2. Utilisez le filtre **Agent** pour ne garder que l'agent concerné, et le filtre **Résultat** pour ne garder que les entrées **Refusé**.
3. Repérez la ligne qui indique *Vous avez refusé une action de* suivi du nom de l'agent. Cliquez dessus pour lire son détail : l'action, sa cible, sa famille et le moment où elle a eu lieu.
4. Si le refus était une erreur, redemandez à l'agent dans le chat. La demande de consentement revient, et vous répondez **Une fois** cette fois-ci.

### 2. Une règle refuse l'action

Une règle peut refuser un type d'action sans vous le demander. Le Journal indique alors *Une règle a refusé une action de* suivi du nom de l'agent.

**Solution :**
1. Dans la barre latérale, ouvrez **Autorisations**.
2. Choisissez l'agent concerné, ou **Règles pour tous les agents**, et cherchez sous **Règles fines** une règle dont l'effet est **Refuse**.
3. Cliquez sur **Retirer** sur cette règle si elle est en cause. Le retrait est inscrit au Journal, et l'action redevient soumise aux accès de sa famille.

   > **Note :** aucune réponse d'une carte de consentement ne refuse pour de bon : **Refuser** est toujours ponctuel. Une règle qui refuse s'écrit à la main sur l'écran **Autorisations**, qui le propose avec le mode développeur allumé.

### 3. L'agent n'a pas cet accès, ou l'organisation l'interdit

Ce qu'un agent peut faire est accordé par famille d'action : lire, écrire ou supprimer, envoyer vers l'extérieur, exécuter du code. Une action hors de ses accès est refusée sans vous le demander. Quand le Journal indique *La politique de l'organisation a bloqué une action de*, le refus vient plutôt de la politique de votre organisation.

**Solution :**
1. Sur l'écran **Autorisations**, choisissez l'agent et lisez **Accès accordés, par famille**.
2. Si l'accès est légitime, cliquez sur **Ouvrir la fiche**, puis sur **Ajuster** sur la fiche de l'agent, et accordez-le dans la section **Accès**. Voir [Approuver ou refuser une action d'agent](../controle/approve-or-reject-an-action.md).
3. Redemandez depuis le chat. Pour un refus par la **Politique de l'organisation**, adressez-vous à la personne qui gère Apollia pour vous : la politique ne se lève pas depuis votre poste.

### 4. Le même type d'action est refusé en boucle

Si un agent enchaîne plusieurs refus, il peut s'arrêter de lui-même. Cela indique souvent une instruction mal formulée plutôt qu'un blocage de permissions.

**Solution :**
1. Dans le **Journal**, comptez les entrées **Refusé** consécutives de cet agent sur la même action.
2. Reformulez votre demande dans le chat en précisant le périmètre autorisé (par exemple : *« travaille uniquement dans `~/Rapports` »*).
3. Pour ne plus être sollicité sur ce type d'action, répondez **Toujours pour cet agent** sur la prochaine carte de consentement.

## Si rien ne fonctionne

1. **Vue d'ensemble** : le **Journal** liste chaque action des agents et chaque changement d'accès, du plus récent au plus ancien. Filtré sur un agent, il montre une suite de refus inattendus et ce qui a refusé chacun : vous, une règle ou la politique.
2. **Tout retirer** : si le comportement est devenu incohérent, ouvrez **Autorisations** et cliquez sur **Tout révoquer**. Choisissez ce qui est retiré (les règles fines, les « toujours » ou les deux) et la **Portée** (tous les agents ou un agent), puis confirmez. Les accès accordés par famille ne changent pas, et les consentements seront redemandés.
3. **Dernier recours :** désinstallez l'agent depuis l'écran **Agents**, puis réinstallez-le pour repartir des accès qu'il déclare.

> **Référence technique :** [Référence Apollia](/reference).
