# Un agent est bloqué

> Pour tout operator qui voit un agent en statut "En cours" sans progression depuis plusieurs minutes : identifier la cause et relancer le travail.

## Vérifications rapides (par ordre de probabilité)

### 1. L'agent attend votre approbation

C'est de loin le cas le plus fréquent. L'agent a tenté une action sensible (écrire un fichier, envoyer un mail, appeler un outil externe) et patiente jusqu'à votre validation.

**Solution :**
1. Dans la sidebar, cliquez sur **Boîte de réception**.
2. Onglet **À traiter** par défaut. Filtrez sur la chip **Approbations** pour ne voir que les approbations en attente, et utilisez le sélecteur **Agent** à droite pour isoler l'agent concerné.
   ![Boîte de réception → À traiter, chips de filtre + sélecteur Agent, une carte d'approbation dépliée avec ses...](../_screenshots/troubleshooting-un-agent-est-bloque-1.png)
3. Cliquez sur l'item pour le déplier en carte HITL, puis **Autoriser** / **Refuser**. L'agent reprend immédiatement son travail. Voir [Approuver ou refuser une action d'agent](../controle/approuver-ou-refuser-une-action.md) pour le détail.

### 2. L'agent attend une réponse d'un outil externe

Un appel à un serveur MCP (Notion, GitHub, Slack) ou à une API distante peut prendre du temps si le service est lent ou injoignable.

**Solution :**
1. Dans la sidebar, ouvrez **Mes assistants**.
2. Repérez la carte de l'agent concerné, passez la souris dessus pour faire apparaître les actions à droite.
3. Cliquez sur le menu **⋯** → **Voir les logs**. Un panneau coulissant s'ouvre depuis la droite avec l'historique des tâches.
4. Utilisez la **barre de recherche** en haut du panneau pour taper un nom d'outil (`notion`, `github`, `fetch`, etc.) - seules les tâches dont l'entrée ou la sortie mentionne cet outil restent affichées.
5. Si la dernière tâche en cours est figée depuis plusieurs minutes sur cet outil, ouvrez **Connexions** dans la sidebar et testez le serveur correspondant.

### 3. Un identifiant ou une autorisation a expiré

Les jetons OAuth (Google, Notion, GitHub) expirent régulièrement. L'agent reste alors en boucle d'erreur silencieuse.

**Solution :**
1. Ouvrez le panneau **Logs** de l'agent (cf. étape 2 ci-dessus).
2. Filtrez sur le statut **Échouée** dans la barre des chips. Tapez `401`, `403`, `expired` ou `unauthorized` dans la **recherche** pour cibler les erreurs d'autorisation.
3. Si vous en trouvez, allez dans **Connexions** et reconnectez le service concerné.

### 4. L'agent tourne en boucle sur la même action

Certains agents peuvent rester coincés sur une étape qu'ils retentent indéfiniment. Apollia applique une limite (StepBudget), mais elle peut être large.

**Solution :**
1. Dans le panneau **Logs** de l'agent, repérez plusieurs tâches consécutives avec la même entrée ou sortie.
2. Sur la carte de l'agent dans **Mes assistants**, cliquez sur l'**icône Stop** (action inline visible au hover). Une confirmation s'affiche.
3. Confirmez l'arrêt. L'agent passe en statut **ARRÊTÉ**. Relancez-le après avoir corrigé la cause (instructions trop vagues, outil manquant).

   > **Note :** il n'existe pas de bouton *« Forcer l'arrêt »* distinct - l'icône Stop envoie un signal d'arrêt normal. Si l'agent ne réagit pas après quelques secondes, redémarrez l'application.

### 5. Une dépendance manque (outil, fichier, modèle)

L'agent peut requérir un outil MCP non installé, un fichier introuvable ou un modèle local non téléchargé.

**Solution :**
1. Dans le panneau **Logs**, filtrez sur **Échouée** et tapez `not found`, `missing` ou `unavailable` dans la recherche.
2. Installez l'outil manquant via **Connexions**, ou téléchargez le modèle depuis **Paramètres → Hub de modèles**.
3. Relancez l'agent depuis sa carte.

## Si rien ne fonctionne

1. Cliquez sur l'**icône Stop** sur la carte de l'agent, puis relancez-le.
2. Si le blocage se reproduit immédiatement, désactivez l'agent depuis la liste **Mes assistants** (toggle on/off sur la carte) pour éviter toute consommation de ressources.
3. **Récupérer les logs pour le support** : ouvrez le panneau **Logs** de l'agent, cliquez sur l'**icône Copier** (à gauche du bouton Rafraîchir dans l'en-tête). Les tâches actuellement affichées (filtres + recherche pris en compte) sont copiées dans le presse-papiers au format texte, prêtes à coller dans un ticket. Un toast confirme le nombre de tâches copiées.

> **Référence technique :** [Briques-Runtime-Core](https://github.com/Apollia-OS/apollia-os/wiki/Briques-Runtime-Core) - comprendre le superviseur qui surveille l'avancement des agents et déclenche les timeouts.
