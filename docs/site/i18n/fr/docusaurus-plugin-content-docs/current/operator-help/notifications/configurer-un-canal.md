---
title: Configurer un canal de notification
sidebar_position: 1
---

# Configurer un canal de notification

> Pour les operators qui veulent recevoir les alertes Apollia là où ils travaillent : sur leur bureau, dans Slack, dans Discord, ou sur un endpoint maison.

## Prérequis

- Vous savez où vous voulez recevoir les notifications (bureau, Slack, Discord, endpoint personnalisé).
- Pour un canal Webhook, vous avez l'URL d'envoi prête (par exemple une URL d'intégration entrante Slack).

> **Note :** la page **Notifications** est accessible directement depuis la sidebar en mode **Opérateur** comme en mode **Builder**. Plus besoin de basculer en Builder pour configurer vos canaux.

## Le modèle à deux niveaux

Le contrôle des notifications se joue sur deux étages, et les confondre est la
cause la plus fréquente d'un canal muet.

1. **Événements globaux**, section en haut de la page **Notifications** : décide quels événements le système suit et route. Un événement décoché ici ne partira **sur aucun canal**, quoi qu'ait été réglé par ailleurs.
2. **Événements par canal**, dans le dialogue Créer ou Modifier d'un canal : parmi les événements activés globalement, filtre ceux qui vont sur **ce canal**. Liste vide = ce canal reçoit tous les événements globaux.

Dans l'ordre : activez d'abord au global ce qui vous intéresse, affinez ensuite canal par canal.

## Les 7 événements disponibles

| Identifiant | Libellé | Description |
|---|---|---|
| `task.completed` | **Tâche terminée avec succès** | Un agent vient d'achever sa mission. |
| `task.failed` | **Tâche échouée** | Une tâche d'agent s'est interrompue sur un échec. |
| `task.input_required` | **Approbation requise** | Un agent attend votre décision (HITL). |
| `agent.degraded` | **Agent en mode dégradé** | L'agent tourne mais quelque chose d'optionnel n'est pas monté : soit un outil optionnel déclaré et non résolu, soit l'installation de son environnement Python qui a échoué. |
| `trigger.error` | **Erreur de déclencheur** | Une automatisation programmée n'a pas pu se déclencher. |
| `llm.backend_down` | **Fournisseur LLM indisponible** | Le fournisseur d'IA configuré ne répond plus. |
| `chat.user_input_required` | **Question d'agent** | Un agent attend votre réponse à une question (`ask_user`). |

## Activer ou désactiver un événement globalement

1. Dans la sidebar, cliquez sur **Notifications**.
2. Repérez la section **Événements globaux** en haut de la page : une grille de cases à cocher, une par type, avec libellé, description courte et identifiant technique.

   ![section Événements globaux, grille de 7 cases à cocher avec libellé, description et identifiant technique](/img/operator-help/notifications-choisir-les-evenements-notifies-1.png)

3. Cochez ou décochez selon ce que vous voulez voir remonter.
4. Cliquez sur **Enregistrer**. Un toast *« Événements globaux enregistrés »* confirme. **Sans ce clic, rien n'est appliqué** : les coches restent locales à l'écran.

## Au premier lancement

Apollia crée automatiquement un canal **Bureau** par défaut au tout premier démarrage : il s'appelle *« Bureau de {votre prénom} »* (ou *« Bureau »* si votre profil ne contient pas encore de prénom). Vous le retrouvez dans la liste, activé, sans événements filtrés (il reçoit donc tous les événements globaux). Si vous le supprimez, il n'est pas recréé.

## Étapes - créer un nouveau canal

1. Dans la sidebar, cliquez sur **Notifications**. La liste affiche vos canaux existants, plus une section *« Événements globaux »* en haut.

2. Cliquez sur **+ Nouveau canal** en haut à droite. Le dialog **Créer un canal** s'ouvre.
   ![page Notifications - section Événements globaux, liste de canaux, bouton "Nouveau canal" en haut à droite](/img/operator-help/notifications-configurer-un-canal-1.png)

3. **Nom** (premier champ, focus automatique) - saisissez un nom clair, libre (espaces, accents et emojis acceptés, 80 caractères max). Exemples : *Alertes Slack équipe*, *Webhook supervision*, *Bureau perso*. Ce nom apparaîtra dans la liste, dans l'historique d'envoi et dans les toasts.

4. **Identifiant technique** (section repliable, optionnelle) - au fur et à mesure que vous tapez le nom, Apollia génère automatiquement un identifiant en *kebab-case* (`alertes-slack-equipe`). Ouvrez le bloc *« Identifiant technique »* pour le voir et le personnaliser si nécessaire ; il reste validé sur la regex *minuscules + tirets*. Une fois posé, il ne change plus.

5. **Type** - choisissez :
   - **Desktop** - notification système de votre ordinateur (toast / centre de notifications natif).
   - **Webhook** - POST HTTP JSON vers une URL externe. C'est la voie pour **Slack**, **Discord**, **Teams** ou tout endpoint maison ; collez simplement leur URL d'intégration entrante.

6. Pour **Webhook** uniquement :
   - **URL du webhook** - colle l'URL d'envoi.
   - **En-têtes (JSON)** *(optionnel)* - un Textarea monospace qui accepte un objet JSON, par exemple `{"Authorization": "Bearer xyz"}`. Le format est validé à la frappe ; un message d'erreur s'affiche si le JSON est mal formé. Laissez vide si l'endpoint ne demande pas d'authentification supplémentaire.

7. **Événements** *(visible dès que des événements globaux sont actifs)* - cochez les types d'événements qui doivent partir sur **ce canal**. Chaque ligne affiche :
   - Un **libellé humain** (*Tâche échouée*, *Approbation requise*…).
   - Une **description courte** sous le libellé.
   - L'**identifiant technique** en monospace plus petit (utile si vous parsez le payload côté Slack/Discord).

   Laissez tout décoché pour recevoir **tous les événements globaux** sur ce canal. Le détail des 7 types et la logique à deux niveaux sont en haut de cette page.

8. **Limiter les notifications** *(anti-spam, par canal et par type d'événement)* - un sélecteur déroulant :
   - **Aucune limite** (défaut).
   - **1 par minute** (60 s).
   - **1 toutes les 5 min** (300 s).
   - **1 par heure** (3600 s).
   - **Personnalisé…** - affiche un champ numérique pour saisir un intervalle entre 1 s et 86 400 s (24 h).

   La limite est calculée **par couple (canal, type d'événement)**. Exemple : régler 60 s laisse passer la première *Tâche terminée*, ignore les suivantes pendant 60 s, et envoie en fin de fenêtre un **récapitulatif** du type *« 12 événements « task.completed » au cours des 60 dernières secondes »*. Pendant ce temps, les *Approbations requises* continuent à passer sans throttling (couple différent).

9. **Activé** *(case cochée par défaut)* - décochez pour créer le canal en pause.

10. Cliquez sur **Créer le canal**. Un toast confirme la création et le canal apparaît dans la liste.

## Anatomie d'une carte de canal

Une fois créé, chaque canal est rendu sous forme de carte avec :

- Une **fine barre d'accent horizontale en haut** - bleu *info* pour un Desktop, bleu *primary* pour un Webhook ; grisée si le canal est désactivé.
- À gauche du titre, une **vignette colorée** avec l'icône du type (écran pour Desktop, prise webhook pour Webhook).
- Le **nom** du canal en titre, son **identifiant technique** en monospace en sous-texte (si vous avez personnalisé le nom), puis un badge **Desktop** ou **Webhook**.
- À droite du titre, le **toggle on/off** pour mettre en pause / réactiver.
- En dessous, les **pastilles d'événements** filtrés (ou la mention *« Tous les événements »* si la liste est vide).
- À l'extrême droite de la ligne d'événements, un petit indicateur **⏱ … s** apparaît si un throttling est configuré.
- En pied de carte, séparé par un trait fin, **trois icônes d'action** côte à côte : avion en papier (Tester), crayon (Modifier), corbeille rouge (Supprimer). Survolez chaque icône pour voir son tooltip.

![Carte d'un canal de notification, barre d'accent, icone du canal, nom et identifiant, et ses badges](/img/operator-help/notifications-configurer-un-canal-2.png)

## Tester le canal

Le test n'est pas dans le dialog de création ; il vit sur la **carte du canal** une fois créé.

1. Repérez la carte du canal dans la liste.
2. Cliquez sur l'**icône avion en papier** dans le pied de carte (tooltip *« Tester »*). Apollia envoie une notification de test générique.
3. Un badge **OK · xxx ms** vert s'affiche dans le pied de carte pendant ~5 s + un toast *« Notification de test envoyée »*. En cas d'échec, un badge rouge porte le message d'erreur de l'endpoint.

> **Note :** l'icône **Tester** est désactivée tant que le canal est en pause (toggle off).

## Mettre en pause un canal

Le **toggle on/off** est directement en en-tête de la carte. Un clic suffit pour basculer ; le changement est persisté immédiatement (toast *« Canal {nom} activé »* / *« désactivé »*). La barre d'accent en haut de la carte passe de sa couleur de type au gris.

## Modifier ou supprimer un canal

- **Icône crayon** *(tooltip « Modifier »)* - ouvre le même dialog que la création, pré-rempli. Le nom reste éditable, l'identifiant technique non. Toute modification doit être confirmée par **Enregistrer**.
- **Icône corbeille** rouge *(tooltip « Supprimer »)* - ouvre une modale de confirmation (*« Supprimer le canal {id} ? Cette action est irréversible »*). Aucun undo n'est proposé.

## Ce que fait exactement une limite

La limite agit **par couple (canal, type d'événement)**. Quand plusieurs
notifications du même type tombent dans la fenêtre :

- la **première** part normalement ;
- les **suivantes** sont absorbées silencieusement ;
- en fin de fenêtre, Apollia envoie un **récapitulatif**, *« 12 événements « task.completed » au cours des 60 dernières secondes »*.

Les autres types continuent de partir sans contrainte : un throttle agressif sur `task.completed` ne retiendra jamais une `task.input_required`. Dès qu'une limite est posée, la carte du canal affiche un indicateur **⏱ … s** à droite de la rangée des événements.

## Vérifier ce qui est réellement parti

L'historique d'envoi n'est **pas** sur la page Notifications. Cette page ne porte
plus qu'une ligne qui renvoie ailleurs. Ouvrez la **Boîte de réception**, puis
l'onglet **Notifications envoyées**.

La liste donne, pour chaque entrée, la date d'envoi, le canal qui l'a prise,
l'événement déclencheur, et si l'envoi a réussi ou échoué.

Le motif d'erreur n'y figure pas. Pour l'obtenir, lancez **Tester** depuis la
carte du canal, sur la page Notifications.

## Vérification

- Après création, la carte du canal apparaît dans la liste avec sa **barre d'accent en haut** colorée selon le type et son **toggle activé** à droite du titre.
- Cliquer sur l'**icône Tester** envoie un message qui arrive sur le canal cible en quelques secondes.
- Le canal apparaît dans la **Boîte de réception**, onglet **Notifications envoyées**, dès le premier envoi réel.

## Si ça ne marche pas

- **Notification bureau invisible** : vérifiez que les notifications de l'application sont autorisées dans les réglages système (macOS : *Réglages → Notifications → Apollia* ; Windows 11 : *Paramètres → Système → Notifications*).
- **Webhook en erreur 401 / 403** : l'URL ou l'en-tête d'authentification est incorrect. Re-générez l'URL côté Slack/Discord/Teams et collez-la à nouveau.
- **Webhook en erreur 404 ou délai dépassé** : l'URL est mal recopiée ou l'endpoint est hors-ligne.
- **Webhook bloqué (SSRF)** : si le message d'erreur contient *« SSRF blocked »*, l'URL pointe vers une adresse privée (`10.x.x.x`, `192.168.x.x`, `127.0.0.1`, ou un endpoint metadata cloud). Apollia OS refuse ces envois par sécurité ; utilisez uniquement des URLs publiques.
- **Nom déjà utilisé** : le nom est libre mais l'identifiant technique doit être unique. Si la création échoue avec un conflit, ouvrez la section *« Identifiant technique »* et changez-le manuellement.
- **Notification de test reçue mais pas les vraies** : vérifiez la section *Événements globaux* en haut de la page **Notifications** - un événement décoché globalement ne partira sur aucun canal, même si ce canal le coche localement.

> **Référence technique :** [Référence Apollia](/reference)
