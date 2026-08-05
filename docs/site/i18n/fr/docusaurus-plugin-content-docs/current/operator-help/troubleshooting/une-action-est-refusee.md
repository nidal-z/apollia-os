# Une action a été refusée

> Pour tout operator qui voit une action refusée dans la Boîte de réception, ou dont l'agent semble avoir abandonné une tâche : comprendre pourquoi et débloquer la suite.

## Vérifications rapides (par ordre de probabilité)

### 1. Vous avez vous-même refusé l'action récemment

Chaque refus manuel envoie un message clair à l'agent, qui s'adapte ou s'arrête. C'est le comportement normal.

**Solution :**
1. Dans la sidebar, cliquez sur **Boîte de réception**.
2. Onglet **À traiter**. Faites défiler vers le bas jusqu'à la section **Historique récent (14 jours)** sous la liste des items en attente.
3. Repérez la ligne avec l'icône ❌ **Refusé** correspondant à l'action. La **raison saisie au moment du refus** est affichée en rouge en sous-texte juste en dessous. Elle explique à l'agent ce qu'il doit changer.
   ![Boite de reception sur l'onglet A traiter, historique recent en bas montrant une ligne refusee et son motif](/img/operator-help/troubleshooting-une-action-est-refusee-1.png)
4. Si le refus était une erreur, relancez l'agent ou demandez-lui dans le chat de retenter avec une nouvelle instruction.

### 2. Une règle de permissions persistée bloque l'action

Une règle créée auparavant (via le parcours d'onboarding ou la configuration initiale) peut refuser ce type d'action automatiquement sans afficher de carte d'approbation.

**Solution :**
1. Dans la sidebar, ouvrez **Paramètres → Autorisations**.
2. Faites défiler la **section Audit récent** en bas de page : elle liste les 20 dernières décisions de permission (autorisé / refusé) avec l'outil, la portée, le numéro de règle appliquée et l'agent concerné. C'est ici que vous identifierez si un refus est dû à une règle persistée - la colonne **décision** affiche `deny` et la colonne suivante précise quelle règle a appliqué le refus.
3. Repérez la règle dans la liste principale **Autorisations actives** au-dessus, puis cliquez sur **Révoquer** (icône corbeille) pour la supprimer si elle est en cause.

   > **Note :** il n'existe pas de bouton *« Toujours refuser »* dans les cartes HITL - un refus est toujours ponctuel. Les règles `deny` proviennent uniquement de la configuration initiale ou d'une édition directe.

### 3. L'agent n'a pas accès au dossier ou à l'outil

Apollia restreint par défaut certains chemins et outils sensibles. Une action sur un chemin interdit est refusée sans même afficher de carte d'approbation.

**Solution :**
1. Si vous voyez la ligne de refus dans l'historique de la Boîte de réception, lisez la raison technique - elle mentionne le chemin ou l'outil concerné.
2. Si l'accès est légitime, ouvrez **Paramètres → Autorisations** et utilisez la portée *Toujours pour cet assistant* / *Toujours pour ce projet* lors de la **prochaine** approbation (au lieu d'une règle deny). Voir [Approuver ou refuser une action d'agent](../controle/approuver-ou-refuser-une-action.md).
3. Relancez la tâche depuis le chat.

### 4. Le même type d'action est refusé en boucle

Si un agent enchaîne plusieurs refus, il peut s'arrêter de lui-même. Cela indique souvent une instruction mal formulée plutôt qu'un blocage de permissions.

**Solution :**
1. Dans la Boîte de réception → Historique récent, comptez les refus consécutifs sur le même outil/chemin.
2. Reformulez votre demande dans le chat en précisant le périmètre autorisé (par exemple : *« travaille uniquement dans `~/Rapports` »*).
3. Pour automatiser les approbations futures sur ce type d'action, utilisez **Toujours autoriser → Pour cet assistant / Pour ce projet** lors de la prochaine carte d'approbation.

## Si rien ne fonctionne

1. **Vue d'ensemble** : la section **Audit récent** de **Paramètres → Autorisations** affiche les 20 dernières décisions de permission avec leur outil, leur décision (`allow` / `deny`), leur portée, le numéro de règle appliquée et l'agent. Si une suite de refus inattendus apparaît, la règle responsable se voit directement.
2. **Tout révoquer** : si le comportement est devenu incohérent, ouvrez **Paramètres → Autorisations**, cliquez sur **Tout révoquer** (bouton rouge en haut à droite) et sélectionnez la portée concernée (*Ce projet* / *Chat / agent* / *Partout* / *Toutes portées*). Confirmez. Les approbations recommenceront à zéro.
3. **Dernier recours :** désactiver l'agent depuis sa carte (toggle on/off dans **Mes assistants**), supprimer toutes ses règles dédiées (filtre `agent_id` dans Autorisations), puis le réactiver pour repartir d'une configuration propre.

> **Référence technique :** [Référence Apollia](/reference) - comprendre comment Apollia décide d'approuver, refuser ou demander pour chaque action sensible.
