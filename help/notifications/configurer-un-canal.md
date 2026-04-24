# Configurer un canal de notification

> Pour les operators qui veulent recevoir les alertes Apollia là où ils travaillent réellement : sur leur bureau, dans Slack, dans Discord ou dans un système maison.

## Prérequis
- Vous savez où vous voulez recevoir les notifications (bureau, Slack, Discord, endpoint personnalisé).
- Pour un canal webhook, vous avez l'URL d'envoi prête (par exemple une URL d'integration Slack).

## Étapes

1. Dans la sidebar, cliquez sur **Notifications**. La liste de vos canaux existants s'affiche, avec le canal **Bureau** déjà présent par défaut.

2. Cliquez sur **+ Nouveau canal** en haut à droite.
   `[SCREENSHOT: page Notifications, liste de canaux et bouton Nouveau canal en haut à droite]`

3. Donnez un **nom** clair au canal (par exemple : *Alertes Slack équipe* ou *Webhook supervision*). Ce nom apparaîtra dans les abonnements et dans l'historique.

4. Choisissez le **type** :
   - **Bureau** — notification système de votre ordinateur (toast, son).
   - **Webhook** — envoi HTTP POST vers une URL externe.

5. Pour un canal **Bureau**, le canal est prêt immédiatement. Passez à l'étape 7.

6. Pour un canal **Webhook**, renseignez l'**URL** d'envoi. Si l'endpoint demande une authentification, ajoutez l'en-tête correspondant dans la section **En-têtes personnalisés**.
   `[SCREENSHOT: dialogue Nouveau canal, type Webhook sélectionné, URL et en-têtes visibles]`

7. Cliquez sur **Tester**. Apollia envoie une notification de test : vous devez la recevoir sur le canal cible en quelques secondes.

8. Cliquez sur **Créer**. Le canal apparaît dans la liste, prêt à être abonné à des événements.

## Vérification
La notification de test arrive bien sur le canal choisi. Le canal figure dans la liste avec un voyant vert.

## Si ça ne marche pas
- **Notification bureau invisible** : vérifiez que les notifications de l'application sont autorisées dans les réglages système de votre ordinateur.
- **Webhook en erreur 401 ou 403** : l'URL ou l'en-tête d'authentification est incorrect. Re-générez l'URL côté outil cible (Slack, Discord) et collez-la à nouveau.
- **Webhook en erreur 404 ou délai dépassé** : l'URL est mal recopiée ou l'endpoint est hors-ligne. Testez l'URL avec un outil comme un client HTTP avant de réessayer.

> **Référence technique :** [Briques-Notifications](https://github.com/nidal-z/apollia-os/wiki/Briques-Notifications)
