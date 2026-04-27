# Démarrer un agent

> Pour tout operator qui a installé un agent : le mettre en marche pour pouvoir discuter avec lui (Assistant) ou le rendre disponible aux automatisations (Worker).

## Prérequis

- L'agent est installé et visible dans **Mes agents**.
- Un fournisseur d'IA est connecté (pastille verte dans le bandeau supérieur).
- Pour un Assistant : vous êtes prêt à ouvrir une conversation avec lui.

## Étapes

1. Dans la sidebar, cliquez sur **Mes assistants**, puis repérez votre agent dans la liste.
   `[SCREENSHOT: page Mes assistants avec sections "Assistants" et "Agents workers", cartes agents avec statut gris "ARRÊTÉ"]`

2. Localisez votre agent dans la bonne section :
   - **Assistants** pour les agents conversationnels.
   - **Agents workers** pour les agents appelés par les triggers ou les pipelines.

3. Cliquez sur **Démarrer** sur la carte de l'agent. Le statut passe au vert et le bouton se transforme en **Arrêter**.
   `[SCREENSHOT: carte agent avec statut vert "ACTIF" et bouton "Arrêter" + bouton secondaire "Ouvrir le chat"]`

4. Si c'est un **Assistant**, cliquez sur **Ouvrir le chat**. Une conversation dédiée s'ouvre, prête à recevoir vos missions.

5. Si c'est un **Worker**, l'agent est désormais sélectionnable comme cible dans les pages **Automatisations** et **Pipelines**.

6. (Optionnel) Cliquez sur **Logs** pour vérifier que le démarrage s'est bien passé. Une ligne récente *"Agent démarré"* doit apparaître.

7. Pour libérer les ressources quand vous n'en avez plus besoin, cliquez sur **Arrêter**. Le statut redevient gris.

## Vérification

Le statut de la carte agent est vert et indique **ACTIF**. Pour un Assistant, l'envoi d'un message dans la conversation déclenche une réponse en streaming.

## Si ça ne marche pas

- **Statut reste orange :** ouvrez les logs de l'agent depuis sa carte pour lire l'erreur précise.
- **Erreur "fournisseur d'IA indisponible" :** vérifiez la pastille du bandeau supérieur et reconnectez le fournisseur si besoin.
- **L'agent démarre mais ne répond pas :** consultez [Un agent est bloqué](../troubleshooting/un-agent-est-bloque.md).

> **Concept :** [book ch11 — Worker Agent Pattern](https://github.com/nidal-z/apollia-os/blob/main/book/src/ch11-00-worker-agent-pattern.md) — comprendre la différence entre Assistant et Worker et leur cycle de vie.
