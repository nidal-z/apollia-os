---
title: Configurer un canal de notification
slug: /operator-help/notifications/set-up-a-channel
sidebar_position: 1
---

# Configurer un canal de notification

> Pour les operators qui veulent recevoir les alertes Apollia là où ils travaillent : sur leur bureau, dans Slack, dans Discord, ou sur un endpoint maison.

## Prérequis

- Vous savez où vous voulez recevoir les notifications (bureau, Slack, Discord, endpoint personnalisé).
- Pour un appel web, vous avez sous la main l'URL de livraison (une URL de webhook entrant Slack, par exemple).

> **Note :** les notifications sont une section des réglages. Dans la barre latérale, cliquez sur **Réglages**, puis sur **Notifications**. Chaque changement fait là est enregistré aussitôt : la section n'a pas de bouton **Enregistrer**.

## Le modèle à deux niveaux

Le contrôle des notifications se fait sur deux niveaux, et les confondre est la
cause la plus fréquente d'un canal muet.

1. **Événements notifiés**, un bloc de la section **Notifications** : décide quels événements peuvent déclencher une notification. Un événement éteint ici ne part **sur aucun canal**, quel que soit le réglage fait ailleurs.
2. **Événements du canal**, dans la fenêtre qui ajoute ou modifie un canal : parmi les événements notifiés, garde ceux qui partent vers **ce canal**. Par défaut, un canal reçoit tous les événements notifiés.

Dans l'ordre : allumez d'abord ce qui vous intéresse, puis affinez canal par canal.

## Les 7 événements disponibles

| Identifiant | Libellé | Description |
|---|---|---|
| `task.completed` | **Tâche terminée** | Un agent vient d'achever sa mission. |
| `task.failed` | **Tâche échouée** | Une tâche d'agent s'est interrompue sur un échec. |
| `task.input_required` | **Décision attendue** | Un agent attend votre décision. |
| `agent.degraded` | **Agent en capacité réduite** | Un outil optionnel est indisponible ; l'agent continue avec des capacités réduites. |
| `trigger.error` | **Automatisation non déclenchée** | Une automatisation programmée n'a pas pu se déclencher. |
| `llm.backend_down` | **IA indisponible** | Le fournisseur d'IA configuré ne répond plus. |
| `chat.user_input_required` | **Question d'un agent** | Un agent attend votre réponse à une question. |

L'identifiant n'est pas affiché dans la section. C'est le nom de l'événement dans ce que reçoit un appel web.

## Allumer ou éteindre un événement

1. Dans la barre latérale, cliquez sur **Réglages**, puis sur **Notifications**.
2. Repérez le bloc **Événements notifiés** : une ligne par événement, avec son libellé, une courte description et un interrupteur.
3. Basculez l'interrupteur de l'événement. Le changement est enregistré aussitôt.

## Au premier lancement

Apollia crée un canal par défaut au tout premier démarrage, une notification sur ce poste. Il porte le prénom de votre profil, sous la forme *« Ada's desktop »*, ou *« Desktop »* si votre profil ne contient pas encore de nom. Vous le retrouvez dans la liste, allumé, recevant tous les événements notifiés. Si vous le supprimez, il n'est pas recréé.

## Étapes - ajouter un canal

1. Dans la barre latérale, cliquez sur **Réglages**, puis sur **Notifications**. Le bloc **Canaux** liste vos canaux existants.

2. Cliquez sur **Ajouter un canal**, dans l'en-tête du bloc **Canaux**. La fenêtre s'ouvre.

3. **Nom** : tapez un nom clair et libre (80 caractères au plus). Exemples : *Alertes Slack équipe*, *Webhook supervision*, *Bureau personnel*. Ce nom apparaît dans la liste et dans les derniers envois.

4. **Identifiant technique (avancé)** (repliable, optionnel) : à mesure que vous tapez le nom, Apollia génère un identifiant en minuscules avec des tirets (`alertes-slack-equipe`). Ouvrez le bloc pour le voir et le changer si besoin. Il n'est proposé qu'à l'ajout d'un canal, et ne se change plus ensuite.

5. **Type** : choisissez :
   - **Notification sur ce poste** : une notification système de votre ordinateur.
   - **Appel web** : un appel HTTP vers une URL externe. C'est la voie pour **Slack**, **Discord**, **Teams** ou tout endpoint maison ; collez leur URL de webhook entrant.

6. Pour un **Appel web** uniquement :
   - **Adresse de l'appel web** : collez l'URL de livraison.
   - **En-têtes (JSON)** *(repliable, optionnel)* : un objet JSON, par exemple `{"Authorization": "Bearer xyz"}`. Un message d'erreur s'affiche si le JSON est mal formé. Laissez vide si l'endpoint ne demande pas d'authentification supplémentaire.

7. **Événements du canal** : **Tous les événements** est coché pour un nouveau canal, qui reçoit donc tous les événements notifiés. Décochez-le pour choisir les événements qui partent sur **ce canal**. Un événement éteint dans les événements notifiés est grisé et indique *Éteint dans les événements notifiés*.

8. **Fréquence maximale** *(par canal et par événement)* : un sélecteur déroulant :
   - **Aucune limite** (défaut).
   - **1 par minute (60 s)**.
   - **1 toutes les 5 min (300 s)**.
   - **1 par heure (3600 s)**.
   - **Personnalisé…** : affiche le champ **Intervalle minimal (secondes)**, jusqu'à 86 400 s (24 h).

   Au-delà de la limite, Apollia regroupe les notifications dans un récapitulatif. La section sur la limite, plus bas dans cette page, dit comment.

9. **Activé** *(coché par défaut)* : décochez pour ajouter le canal éteint.

10. Cliquez sur **Ajouter un canal**. Un message le confirme et le canal apparaît dans la liste.

## Anatomie d'une ligne de canal

Chaque canal est une ligne du bloc **Canaux**, avec :

- L'**icône** de son type.
- Le **nom** du canal.
- Une **ligne de détail** : son type, les événements qu'il reçoit (**Tous les événements** ou leur nombre) et sa fréquence maximale.
- Le bouton **Tester**.
- Un **menu** qui contient **Modifier** et **Supprimer**.
- L'**interrupteur** qui allume ou éteint le canal.

## Tester le canal

Le test n'est pas dans la fenêtre qui ajoute un canal ; il se trouve sur la **ligne** du canal.

1. Repérez la ligne du canal dans la liste.
2. Cliquez sur **Tester**. Apollia envoie une notification de test.
3. La ligne indique *Envoi en cours…*, puis *Test réussi*, avec le temps pris, ou *Échec*, suivi de la raison.

> **Note :** un canal éteint ne peut pas être testé.

## Mettre en pause un canal

L'**interrupteur** se trouve en bout de ligne. Un clic suffit à le basculer ; le changement est enregistré aussitôt. Un canal éteint ne reçoit rien.

## Modifier ou supprimer un canal

- **Modifier**, dans le menu de la ligne, ouvre la même fenêtre que l'ajout, préremplie. Le nom reste modifiable, l'identifiant technique n'est pas affiché. Validez avec **Enregistrer**.
- **Supprimer**, dans le même menu, ouvre une confirmation : le canal ne recevra plus rien, et les autres canaux et les événements notifiés ne changent pas.

## Ce que fait exactement une limite

La limite agit **par paire (canal, type d'événement)**. Quand plusieurs
notifications du même type arrivent dans la fenêtre :

- la **première** part normalement ;
- les **suivantes** sont absorbées silencieusement ;
- à la fin de la fenêtre, Apollia envoie un **récapitulatif**, rédigé en anglais : *« 12 'task.completed' events over the last 60 seconds »*.

Les autres types continuent de partir sans contrainte : une limite agressive sur `task.completed` ne retiendra jamais un `task.input_required`. La ligne de détail du canal affiche la limite en vigueur.

## Vérifier ce qui est réellement parti

En bas de la section **Notifications**, le bloc **Derniers envois** liste les
cinq dernières notifications envoyées par Apollia, de la plus récente à la plus
ancienne. Tant qu'il n'y a aucun envoi, une phrase le dit.

Chaque ligne donne l'événement, les canaux par lesquels il est passé, le moment
de l'envoi, et s'il est arrivé : **Envoyée** ou **Échec**.

Pour obtenir la raison d'un échec, lancez **Tester** depuis la ligne du canal :
la ligne affiche alors la raison.

## Vérification

- Après l'ajout, le canal apparaît dans le bloc **Canaux** avec son interrupteur allumé.
- Cliquer sur **Tester** envoie un message qui arrive sur la cible en quelques secondes, et la ligne indique *Test réussi*.
- La première livraison réelle figure dans **Derniers envois**.

## Si ça ne marche pas

- **Notification de bureau invisible** : vérifiez que les notifications de l'application sont autorisées dans les réglages du système (macOS : *Réglages → Notifications → Apollia* ; Windows 11 : *Paramètres → Système → Notifications*).
- **Appel web qui renvoie 401 / 403** : l'URL ou l'en-tête d'authentification est faux. Régénérez l'URL côté Slack/Discord/Teams et collez-la de nouveau.
- **Appel web qui renvoie 404 ou expire** : l'URL a été mal copiée ou l'endpoint est hors ligne.
- **Appel web bloqué (SSRF)** : si le message d'erreur contient *« SSRF blocked »*, l'URL pointe vers une adresse privée (`10.x.x.x`, `192.168.x.x`, `127.0.0.1`, ou un endpoint de métadonnées cloud). Apollia refuse ces livraisons par sécurité ; utilisez uniquement des URL publiques.
- **Identifiant déjà pris** : le nom est libre mais l'identifiant technique doit être unique. Si l'ajout échoue sur un conflit, ouvrez le bloc **Identifiant technique (avancé)** et changez-le.
- **Notification de test reçue mais pas les vraies** : vérifiez le bloc **Événements notifiés**. Un événement éteint là ne part sur aucun canal, même si ce canal le coche.

> **Référence technique :** [Référence Apollia](/reference)
