# Choisir les événements notifiés

> Pour les operators qui veulent recevoir uniquement les notifications utiles, sans bruit ni manque, en associant chaque type d'événement au bon canal.

## Le modèle à deux niveaux

Apollia répartit le contrôle des notifications sur **deux étages** :

1. **Événements globaux** *(section en haut de la page Notifications)* — décide quels événements le système doit suivre et router. Un événement décoché ici ne partira **sur aucun canal**, quoi qu'il arrive.
2. **Événements par canal** *(dans le dialog Créer / Modifier d'un canal)* — pour chaque événement activé globalement, filtre s'il doit aller sur **ce canal précis**. Liste vide = ce canal reçoit **tous** les événements globaux.

Logique pratique : commencez par activer les événements qui vous intéressent au niveau global, puis affinez le routage canal par canal selon vos besoins.

## Prérequis

- Au moins un canal de notification est configuré (voir [Configurer un canal de notification](configurer-un-canal.md)).
- Vous savez quels événements méritent une alerte pour vous.

## Les 6 événements disponibles

| Identifiant | Libellé | Description |
|---|---|---|
| `task.completed` | **Tâche terminée avec succès** | Un agent vient d'achever sa mission. |
| `task.failed` | **Tâche échouée** | Une tâche d'agent s'est interrompue sur un échec. |
| `task.input_required` | **Approbation requise** | Un agent attend votre décision (HITL). |
| `agent.degraded` | **Agent en mode dégradé** | Un outil optionnel est indisponible ; l'agent continue avec des capacités réduites. |
| `trigger.error` | **Erreur de déclencheur** | Une automatisation programmée n'a pas pu se déclencher. |
| `llm.backend_down` | **Fournisseur LLM indisponible** | Le fournisseur d'IA configuré ne répond plus. |

## Étapes — activer / désactiver un événement globalement

1. Dans la sidebar, cliquez sur **Notifications**.

2. Repérez la section **Événements globaux** en haut de la page. Elle affiche une grille de cases à cocher, une par type d'événement, avec libellé humain, description courte et identifiant technique en sous-texte muted.
   ![section Événements globaux — grille de 6 cases à cocher avec libellé humain, description et identifiant en...](../_screenshots/notifications-choisir-les-evenements-notifies-1.png)

3. Cochez ou décochez les événements selon ce que vous voulez voir remonter.

4. Cliquez sur **Enregistrer**. Un toast *« Événements globaux enregistrés »* confirme. Les modifications **ne sont pas appliquées instantanément** — sans clic sur Enregistrer, les coches restent locales à l'écran.

## Étapes — affiner le routage par canal

1. Dans la liste de canaux, cliquez sur **Modifier** sur la carte du canal voulu.

2. Dans le dialog, descendez à la section **Événements**. Elle affiche **les mêmes 6 événements** que la section globale, sous forme de cases à cocher.

3. Cochez les événements à router vers **ce canal**. Ne rien cocher = ce canal reçoit **tous les événements globaux** activés (mention *« Tous les événements »* affichée sur la carte).

4. Cliquez sur **Enregistrer** pour persister. Le canal affichera désormais des badges d'événement correspondants sous son nom dans la liste.

## Réguler le bruit avec le throttling

Pour les événements bavards (`task.completed` en particulier), configurez **Limiter les notifications** dans le dialog Créer / Modifier du canal :

- **Aucune limite** (défaut).
- **1 par minute** (60 s).
- **1 toutes les 5 min** (300 s).
- **1 par heure** (3600 s).
- **Personnalisé…** — secondes au choix entre 1 et 86 400 (24 h).

La limite agit **par couple (canal, type d'événement)**. Si plusieurs notifications du même type tombent pendant la fenêtre :

- La **première** part comme d'habitude.
- Les **suivantes** sont absorbées silencieusement.
- En fin de fenêtre, Apollia envoie un **récapitulatif** : *« 12 événements « task.completed » au cours des 60 dernières secondes »*.

Pendant ce temps, les autres types d'événements continuent à partir sans contrainte — un throttle agressif sur `task.completed` ne bloque jamais une `task.input_required`.

Dès qu'un throttling est posé sur un canal, sa carte affiche un petit indicateur **⏱ … s** à droite de la rangée des événements, pour vous rappeler en un coup d'œil que ce canal applique une limite.

## Mettre un canal en pause

Le toggle on/off en en-tête de la carte du canal le met en silence en un clic (voir [Configurer un canal](configurer-un-canal.md#mettre-en-pause-un-canal)). Le canal reste configuré mais n'émet plus rien tant qu'il est désactivé.

## Vérifier l'envoi réel

La section **Historique** en bas de la page **Notifications** liste les 50 derniers événements traités, avec 4 colonnes :

- **Horodatage** (relatif : `5min ago`).
- **Canal** — affiché par son **nom** (label) si défini, sinon son identifiant technique.
- **Événement** — libellé humain si traduisible, sinon l'identifiant technique brut.
- **Statut** — badge vert *« envoyé »* ou rouge *« échoué »*.

Un filtre par canal en haut du tableau permet de cibler une cible précise. Le motif d'erreur n'est pas affiché ici — pour le voir, lancez **Tester** depuis la carte du canal concerné.

## Vérification

- Cocher un événement globalement puis en provoquer un (ex. lancer un agent qui termine) → l'événement apparaît dans l'**Historique** avec statut *« envoyé »*, et la notification réelle arrive sur les canaux abonnés.
- Décocher un événement globalement → plus aucune occurrence ne remonte, sur aucun canal.
- Régler 60 s de throttling sur un canal puis générer 5 *Tâches terminées* rapidement → 1 notification immédiate, puis 1 récapitulatif en fin de fenêtre.

## Si ça ne marche pas

- **Trop de notifications** : utilisez d'abord le throttling sur les événements de routine (`task.completed`), puis seulement décochez en dernier recours.
- **Aucune notification reçue alors qu'un événement est arrivé** :
  1. Vérifiez que l'événement est coché dans la section **Événements globaux** en haut de la page.
  2. Vérifiez que le canal cible est **activé** (toggle vert sur la carte).
  3. Vérifiez la section **Événements** du canal : si la liste n'est pas vide, l'événement doit y figurer.
  4. Cliquez sur l'**icône Tester** (avion en papier) dans le pied de la carte du canal pour confirmer que la chaîne d'envoi fonctionne.
- **Récapitulatifs en cascade** : si vous voyez beaucoup de notifications *« N événements … »*, c'est que le throttling est probablement trop strict ou l'événement trop bavard. Réduisez la fenêtre ou décochez l'événement pour ce canal.

> **Référence technique :** [Briques-Notifications](https://github.com/nidal-z/apollia-os/wiki/Briques-Notifications)
