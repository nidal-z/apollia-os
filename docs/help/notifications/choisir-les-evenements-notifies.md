# Choisir les événements notifiés

> Pour les operators qui veulent recevoir uniquement les notifications utiles, sans bruit ni manque, en associant chaque type d'événement au bon canal.

## Prérequis
- Au moins un canal de notification est configuré (voir *Configurer un canal de notification*).
- Vous savez quels événements méritent réellement une alerte (par exemple : approbations en attente, échecs d'agent).

## Étapes

1. Dans la sidebar, cliquez sur **Notifications**, puis sur le canal que vous voulez paramétrer.

2. Ouvrez l'onglet **Événements**. La liste des types disponibles s'affiche avec une case à cocher chacun.
   `[SCREENSHOT: onglet Événements d'un canal, liste de cases à cocher avec identifiants d'événements]`

3. Cochez les événements pour lesquels ce canal doit envoyer une notification. Les événements disponibles sont :
   - `task.completed` — une tâche agent s'est terminée avec succès.
   - `task.failed` — une tâche agent a échoué.
   - `task.input_required` — un agent attend votre approbation (HITL).
   - `agent.degraded` — un agent est passé en état dégradé (outil optionnel indisponible).
   - `trigger.error` — une automatisation a rencontré une erreur.
   - `pipeline.completed` — un pipeline s'est terminé avec succès.
   - `pipeline.failed` — un pipeline a échoué.
   - `pipeline.suspended` — un pipeline est suspendu (étape HITL en attente).

   > **Note :** les événements sont affichés sous leur identifiant technique. Les événements "tâche annulée" et "erreur générique" ne sont pas disponibles en tant qu'événements de notification séparés.

4. Chaque modification est appliquée instantanément. Le bouton **Tester** envoie un exemplaire de chaque type coché pour vérifier que la chaîne fonctionne.

5. Pour mettre un canal en pause sans perdre sa configuration, basculez l'interrupteur **Actif** en haut de la page sur **Off**.

## Vérification
Lorsqu'un événement coché survient, vous recevez une notification sur le canal correspondant. Les événements non cochés restent silencieux.

## Si ça ne marche pas
- **Trop de notifications** : décochez les événements de routine (`task.completed`, `pipeline.completed`) et gardez uniquement `task.failed`, `pipeline.failed`, `task.input_required`.
- **Aucune notification reçue** : vérifiez que le canal est **Actif** et que la case correspondant à l'événement est cochée. Lancez le bouton **Tester** pour confirmer.
- **Notification reçue avec un délai important** : si le canal est un webhook, l'endpoint cible est peut-être lent. Consultez l'historique d'envoi en bas de la page pour voir les durées.

> **Référence technique :** [Briques-Notifications](https://github.com/nidal-z/apollia-os/wiki/Briques-Notifications)
