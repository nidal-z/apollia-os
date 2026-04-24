# Un agent est bloqué

> Pour tout operator qui voit un agent en statut "En cours" sans aucune progression depuis plusieurs minutes : identifier la cause et relancer le travail.

## Vérifications rapides (par ordre de probabilité)

### 1. L'agent attend votre approbation

C'est de loin le cas le plus fréquent. L'agent a tenté une action sensible (écrire un fichier, envoyer un mail) et patiente jusqu'à votre validation.

**Solution :**
1. Dans la sidebar, cliquez sur **Inbox**.
2. Repérez les cartes marquées **En attente d'approbation** liées à l'agent.
   `[SCREENSHOT: page Inbox avec une carte d'approbation en attente, bouton Approuver visible]`
3. Cliquez sur **Approuver** ou **Refuser** : l'agent reprend immédiatement son travail.

### 2. L'agent attend une réponse d'un outil externe

Un appel à un serveur MCP (Notion, GitHub, Slack) ou à une API distante peut prendre du temps si le service est lent ou injoignable.

**Solution :**
1. Dans la sidebar, ouvrez **Agents** et sélectionnez l'agent concerné.
2. Cliquez sur l'onglet **Logs**.
3. Cherchez la dernière ligne contenant un nom d'outil ("notion", "github", "fetch"). Si elle est figée depuis plusieurs minutes, l'outil ne répond pas.
4. Ouvrez **Intégrations → MCP** et testez la connexion du serveur concerné.

### 3. Un identifiant ou une autorisation a expiré

Les jetons OAuth (Google, Notion, GitHub) expirent régulièrement. L'agent reste alors en boucle d'erreur silencieuse.

**Solution :**
1. Ouvrez **Agents → [votre agent] → Logs**.
2. Cherchez les mots **expired**, **unauthorized**, **401** ou **403**.
3. Si vous en trouvez, allez dans **Intégrations** et reconnectez le service concerné.

### 4. L'agent tourne en boucle sur la même action

Certains agents peuvent rester coincés sur une étape qu'ils retentent indéfiniment. Apollia applique une limite (StepBudget), mais elle peut être large.

**Solution :**
1. Dans **Agents → [votre agent] → Logs**, repérez la même ligne qui se répète sans changement.
2. Cliquez sur le bouton **Forcer l'arrêt** en haut de la fiche agent.
3. L'agent passe en statut **Arrêté**. Relancez-le après avoir corrigé la cause (instructions trop vagues, outil manquant).

### 5. Une dépendance manque (outil, fichier, modèle)

L'agent peut requérir un outil MCP non installé, un fichier introuvable ou un modèle local non téléchargé.

**Solution :**
1. Consultez les **Logs** de l'agent et cherchez les mots **not found**, **missing** ou **unavailable**.
2. Installez l'outil manquant via **Intégrations**, ou téléchargez le modèle depuis **Settings → Model Hub**.
3. Relancez l'agent.

## Si rien ne fonctionne

1. Forcez l'arrêt de l'agent depuis sa fiche, puis relancez-le.
2. Si le blocage se reproduit immédiatement, désactivez l'agent dans la liste **Agents** pour éviter toute consommation de ressources.
3. Exportez les logs de l'agent (bouton **Exporter** dans l'onglet Logs) et joignez-les à votre demande de support.

> **Concept :** [Briques-Runtime-Core](https://github.com/nidal-z/apollia-os/wiki/Briques-Runtime-Core) — comprendre le superviseur qui surveille l'avancement des agents et déclenche les timeouts.
