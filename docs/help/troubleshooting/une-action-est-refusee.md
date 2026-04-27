# Une action a été refusée

> Pour tout operator qui voit "Action refusée" dans un chat ou dans l'Inbox, ou dont l'agent semble avoir abandonné une tâche : comprendre pourquoi et débloquer la suite.

## Vérifications rapides (par ordre de probabilité)

### 1. Vous avez vous-même refusé l'action récemment

Chaque refus manuel envoie un message clair à l'agent, qui s'adapte ou s'arrête. C'est le comportement normal.

**Solution :**
1. Ouvrez **Inbox** et cliquez sur la carte de l'action refusée.
   `[SCREENSHOT: carte d'approbation refusée dans Inbox, raison du refus visible en bas]`
2. Lisez la raison que vous avez saisie : elle explique à l'agent ce qu'il doit changer.
3. Si le refus était une erreur, relancez l'agent ou demandez-lui dans le chat de retenter avec la nouvelle instruction.

### 2. Une règle de permissions bloque l'action automatiquement

Vous avez peut-être créé une règle (ou utilisé "Toujours refuser") qui rejette ce type d'action sans demander.

**Solution :**
1. Dans la sidebar, cliquez sur **Paramètres**, puis sur la section **Autorisations**.
2. Parcourez la liste des règles actives et repérez celle qui correspond à l'action (par exemple : *écriture dans `~/Documents`*).
3. Supprimez la règle ou modifiez-la pour autoriser le cas légitime.

### 3. L'agent n'a pas le droit d'accéder au dossier ou à l'outil

Apollia restreint par défaut l'accès à votre disque et à certains outils sensibles. Une action portant sur un chemin interdit est refusée d'office.

**Solution :**
1. Lisez le message complet de refus : il mentionne le chemin ou l'outil concerné.
2. Si l'accès est légitime, ouvrez **Paramètres → Autorisations** et ajoutez une règle qui autorise ce dossier ou cet outil pour cet agent.
3. Relancez la tâche depuis le chat.

### 4. Le même type d'action est refusé en boucle

Si un agent voit plusieurs refus consécutifs, il peut s'arrêter de lui-même. Cela indique souvent une instruction mal formulée plutôt qu'un vrai blocage.

**Solution :**
1. Ouvrez le chat ou la fiche de l'agent et lisez les dernières actions tentées.
2. Reformulez votre demande en précisant le périmètre autorisé (par exemple : *travaille uniquement dans `~/Rapports`*).
3. Pour automatiser les approbations futures sur ce type d'action, cochez **Toujours autoriser** lors de la prochaine demande.

## Si rien ne fonctionne

1. Si vous ne comprenez pas l'origine du refus, ouvrez **Observabilité → Audit trail** : chaque refus y est enregistré avec son auteur (vous, une règle, le runtime) et son horodatage.
2. Révoquez les règles liées à l'agent depuis **Paramètres → Autorisations** si le comportement est devenu incohérent.
3. En dernier recours, supprimez l'agent et réinstallez-le pour repartir d'une configuration propre.

> **Concept :** [Securite-Permissions](https://github.com/nidal-z/apollia-os/wiki/Securite-Permissions) — comprendre comment Apollia décide d'approuver, refuser ou demander pour chaque action sensible.
