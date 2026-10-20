---
title: Un agent est bloqué
slug: /operator-help/troubleshooting/an-agent-is-stuck
sidebar_position: 4
---

# Un agent est bloqué

> Pour toute personne qui voit un agent au travail sans progression depuis plusieurs minutes : identifier la cause et relancer le travail.

## Vérifications rapides (par ordre de probabilité)

### 1. L'agent attend votre réponse

C'est de loin le cas le plus fréquent. L'agent a tenté une action sensible (écrire un fichier, envoyer un mail, appeler un outil externe) et attend votre consentement, ou il vous a posé une question.

**Solution :**
1. Dans la barre latérale, cliquez sur **À valider**.
2. Tout ce qui vous attend est dans une seule liste : consentements, questions, tâches en pause et éléments de lot. Le filtre **Décisions** ne garde que les consentements, et chaque élément dit d'où il vient.
3. Répondez sur la carte : **Une fois**, **Toujours pour cet agent** ou **Refuser**. La conversation, la tâche ou le lot concerné reprend aussitôt. Voir [Approuver ou refuser une action d'agent](../controle/approve-or-reject-an-action.md) pour le détail.

### 2. L'agent attend une réponse d'un outil externe

Un appel à un serveur MCP (Notion, GitHub, Slack) ou à une API distante peut prendre du temps si le service est lent ou injoignable.

**Solution :**
1. Dans la barre latérale, ouvrez **Agents**.
2. Repérez l'agent concerné dans la liste et faites un clic droit dessus.
3. Choisissez **Logs**. Un panneau coulissant s'ouvre depuis la droite avec l'historique de ses tâches.
4. Utilisez le champ de recherche en haut du panneau pour saisir un nom d'outil (`notion`, `github`, `fetch`, etc.) : seules les tâches dont l'entrée ou le résultat mentionne cet outil restent affichées.
5. Si la dernière tâche en cours est figée depuis plusieurs minutes sur cet outil, ouvrez **Connexions** dans la barre latérale et testez la connexion correspondante.

### 3. Un identifiant ou une autorisation a expiré

Les jetons OAuth (Google, Notion, GitHub) expirent régulièrement. L'agent échoue alors à chaque appel à ce service.

**Solution :**
1. Ouvrez le panneau **Logs** de l'agent (voir la vérification 2 ci-dessus).
2. Filtrez sur le statut **ÉCHOUÉE**. Saisissez `401`, `403`, `expired` ou `unauthorized` dans le champ de recherche pour cibler les erreurs d'autorisation.
3. Si vous en trouvez, allez dans **Connexions** et reconnectez le service concerné.

### 4. L'agent tourne en boucle sur la même action

Certains agents peuvent rester coincés sur une étape qu'ils retentent sans fin. Apollia applique une limite sur le nombre d'étapes, dont le plafond varie selon le palier d'autonomie : il est plus bas en `assisted` et plus élevé en `bounded_autonomous` ou `long_autonomous`.

**Solution :**
1. Dans le panneau **Logs** de l'agent, repérez plusieurs tâches consécutives avec la même entrée ou le même résultat.
2. Si le travail se déroule dans une conversation, ouvrez cette conversation et cliquez sur **Arrêter** dans la zone de saisie.
3. Redemandez après avoir corrigé la cause (instructions trop vagues, outil manquant).

   > **Note :** un agent n'a ni démarrage, ni arrêt, ni interrupteur qui lui soit propre : il est chargé par sa première tâche et libéré une fois inactif. Si une tâche qui ne se déroule pas dans une conversation ne se termine jamais, quittez Apollia et rouvrez-la.

### 5a. L'agent prend plus de temps que prévu en palier supervised ou bounded_autonomous

En palier `supervised`, `bounded_autonomous` ou `long_autonomous`, l'agent contrôle son travail après l'exécution et tente une auto-correction si nécessaire. Cela allonge la durée apparente avant que l'agent se déclare terminé. C'est un comportement normal, pas un blocage.

Si la durée vous semble excessive, ouvrez le panneau **Logs** (voir [Consulter les logs d'un agent](../agents/view-an-agent-logs.md)). La vérification n'a pas de statut propre : la tâche reste **EN COURS** pendant que l'agent contrôle son travail. Ce qui vous signale une boucle qui ne converge pas, c'est une tâche qui reste en cours bien au-delà du moment où son résultat aurait dû tomber. Arrêtez le travail, puis reformulez les instructions.

### 5b. Une dépendance manque (outil, fichier, modèle)

L'agent peut requérir une connexion qui n'est pas en place, un fichier introuvable ou un modèle local non téléchargé.

**Solution :**
1. Dans le panneau **Logs**, filtrez sur **ÉCHOUÉE** et saisissez `not found`, `missing` ou `unavailable` dans le champ de recherche.
2. Ajoutez la connexion manquante depuis **Connexions**, ou téléchargez le modèle depuis l'écran **IA** avec **Découvrir des modèles**.
3. Redemandez à l'agent.

## Si rien ne fonctionne

1. Arrêtez le travail dans sa conversation, puis redemandez avec une requête plus courte et plus précise.
2. Si le blocage se reproduit immédiatement, ouvrez l'agent dans **Agents** et relisez sa fiche : sa mission, son modèle, ses accès et ses sources.
3. **Récupérer les logs pour le support** : ouvrez le panneau **Logs** de l'agent et cliquez sur l'icône **Copier les logs** dans son en-tête, à côté de l'icône **Actualiser** (survolez une icône pour lire son nom). Les tâches actuellement affichées (filtres et recherche pris en compte) sont copiées dans le presse-papiers au format texte, prêtes à coller dans un signalement. Un message confirme le nombre de tâches copiées.

> **Référence technique :** [Référence Apollia](/reference).
