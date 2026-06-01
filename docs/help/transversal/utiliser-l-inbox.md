# Utiliser la Boîte de réception

> Pour les operators qui veulent traiter au même endroit tout ce qui demande leur attention : approbations d'actions, questions d'agents, échecs récents, historique des notifications envoyées.

## Prérequis

- Au moins un agent ou trigger a tourné récemment.

## Où la trouver

Dans la sidebar, l'entrée **Boîte de réception** (icône bouclier). Le badge à côté du libellé indique le nombre d'**actions à traiter** (approbations + questions en attente). Ce compteur reste strictement sur les actions concrètes - il n'inclut pas les événements informatifs ni les notifications passées.

## Architecture de la page

La page est organisée en **trois onglets**, accessibles via la barre d'onglets en tête :

| Onglet | Contenu |
|---|---|
| **À traiter** | Approbations HITL en attente + questions `ask_user` que les agents vous posent. C'est ici que vit l'interactivité bloquante. |
| **Activité** | Événements récents qui demandent votre attention sans bloquer un agent : échecs de tâche, erreurs de déclencheur, agents en mode dégradé, fournisseur LLM indisponible. Fenêtre glissante de 14 jours. |
| **Notifications envoyées** | Historique des notifications poussées vers vos canaux Desktop / Webhook (50 dernières). |

L'onglet actif est mémorisé entre les sessions (rafraîchir la page conserve votre dernière vue). Vous pouvez aussi y arriver directement via un lien (la page **Paramètres → Notifications** redirige par exemple sur l'onglet *Notifications envoyées*).

## Onglet "À traiter"

![onglet À traiter - barre d'onglets en haut avec compteurs en chip, chips de filtre (Tous / Approbations / Q...](../_screenshots/transversal-utiliser-l-inbox-1.png)

### Filtrer la liste

- **Trois chips de type** au-dessus de la liste - **Tous** / **Approbations** / **Questions**. Cliquez pour ne voir que ce qui vous intéresse.
- **Sélecteur Agent** à droite de la rangée de chips - isole le travail d'un seul agent.
- Les items sont **groupés automatiquement par jour** (Aujourd'hui / Hier / Plus tôt), du plus récent en haut.

### Traiter une approbation

Cliquez sur une ligne d'approbation pour la déplier en carte de décision. Voir la page dédiée [Approuver ou refuser une action d'agent](../controle/approuver-ou-refuser-une-action.md) pour le détail des 4 portées de *Toujours autoriser* et du dialog de raison de refus (5 à 500 caractères).

### Répondre à une question d'agent (`ask_user`)

Quand un agent utilise l'outil `ask_user` pour vous interroger, l'item apparaît dans cet onglet sous le libellé **Question**. Cliquez pour déplier le formulaire dynamique.

![formulaire ask_user déplié - contexte en encart bleu en haut, une question texte avec hint, une question à...](../_screenshots/transversal-utiliser-l-inbox-2.png)

Le formulaire affiche, dans l'ordre :

- Un **encart bleu** avec le contexte fourni par l'agent (s'il en a fourni un - *"pourquoi je vous pose ces questions"*).
- Une à dix **questions**, chacune rendue selon son type :
  - **Question ouverte** - champ texte libre, avec une indication d'aide si l'agent en a précisé une.
  - **Choix unique** - radios, une seule option sélectionnable.
  - **Choix multiple** - cases à cocher, plusieurs options possibles.

Deux actions en bas :

- **Répondre** *(bouton primaire)* - envoie vos réponses à l'agent, qui reprend son exécution. Les champs laissés vides sont marqués *« non répondu »* dans la réponse transmise - pas de blocage.
- **Refuser de répondre** *(ghost rouge)* - ouvre le dialog de raison (5 à 500 caractères). L'agent reçoit votre refus motivé et adapte sa suite.

> **Note :** une notification (toast desktop ou webhook si configuré) part automatiquement quand un agent appelle `ask_user`. Vous pouvez désactiver cette notification dans **Paramètres → Notifications** en décochant *Question d'agent*.

### Historique récent

Sous la liste des items en attente, une section **Historique récent (14 jours)** liste les 50 dernières décisions HITL résolues : ✅ Autorisé / 🛡 Toujours autorisé / ❌ Refusé (avec la raison saisie). Lecture seule.

## Onglet "Activité"

![onglet Activité - 4 chips de filtre Toutes / Échecs / Dégradations / LLM, liste de cartes avec icône coloré...](../_screenshots/transversal-utiliser-l-inbox-3.png)

Cet onglet liste les événements qui n'ont pas demandé d'action immédiate mais méritent un coup d'œil. Quatre catégories couvertes sur la fenêtre de 14 jours :

- **Échecs** ❌ - `task.failed` (tâche d'agent échouée) et `trigger.error` (déclencheur en erreur).
- **Dégradations** ⚠️ - `agent.degraded` (un outil optionnel n'est plus disponible, l'agent continue en capacités réduites).
- **LLM** 🔌 - `llm.backend_down` (fournisseur d'IA injoignable).

Filtrez avec les chips en haut. Chaque ligne propose un bouton **Voir les logs** qui ouvre **Observabilité → Chronologie** pré-positionnée sur la tâche concernée si disponible.

> **Source :** cet onglet consomme la même base de données que la page Notifications. Un événement apparaît ici uniquement s'il a déclenché au moins une tentative d'envoi vers un canal (notification globale activée). Sinon, retrouvez-le dans **Observabilité → Chronologie**.

## Onglet "Notifications envoyées"

![onglet Notifications envoyées - sélecteur de filtre par canal, tableau 4 colonnes Horodatage / Canal / Évén...](../_screenshots/transversal-utiliser-l-inbox-4.png)

Tableau des **50 dernières notifications** poussées vers vos canaux Desktop ou Webhook, avec :

- **Horodatage** relatif (avec date absolue en tooltip).
- **Canal** - affiché par son nom (label) si défini, sinon son identifiant.
- **Événement** - libellé humain (*Tâche échouée*, *Approbation requise*, etc.).
- **Statut** - badge vert *« envoyé »* ou rouge *« échoué »*.

Un sélecteur en haut filtre par canal. Pour configurer les canaux ou les événements globaux, allez sur **Paramètres → Notifications** (voir [Configurer un canal](../notifications/configurer-un-canal.md) et [Choisir les événements](../notifications/choisir-les-evenements-notifies.md)).

## Vérification

- L'onglet "À traiter" se vide quand vous avez tout résolu - le badge sidebar redescend à zéro.
- Une nouvelle approbation ou question d'agent fait remonter le badge **sans rafraîchissement manuel** (push temps réel).
- Refuser un item le déplace immédiatement dans la section *Historique récent*.

## Si ça ne marche pas

- **Le badge sidebar reste à zéro alors qu'un agent attend** : vérifiez le **point d'état** à côté du mot *Apollia* dans le bandeau supérieur. S'il est rouge, le runtime est déconnecté ; quittez et rouvrez l'application.
- **Un item `ask_user` ne propose pas le formulaire** : c'est un cas limite. Ouvrez la conversation correspondante (id de session affiché sur la carte) ; l'agent y attend une réponse via le chat lui-même.
- **L'onglet Activité est vide alors qu'une tâche a clairement échoué** : l'événement n'a probablement pas été poussé dans la chaîne de notifications (aucun canal abonné). Ouvrez **Observabilité → Chronologie** pour la trace brute.
- **Vous voulez éviter de réapprouver toujours la même action** : utilisez les options *Toujours autoriser* sur la carte. Voir [Approuver ou refuser une action d'agent](../controle/approuver-ou-refuser-une-action.md).

## Demandes provenant d'un serveur MCP

À partir de v0.1.0, deux types d'événements émis par des serveurs MCP peuvent atterrir dans votre boîte de réception :

- **Demande d'input structurée** (`elicitation/create` MCP) - le serveur veut une saisie utilisateur (formulaire dynamique généré depuis un JSON Schema). Tombe dans **À traiter** sous la même forme qu'un `ask_user` classique.
- **Demande de sampling LLM** (`sampling/createMessage` MCP) - le serveur demande un appel LLM. Le prompt complet + l'identifiant du serveur source sont affichés avant approbation. Au-delà de 100 sampling/heure par serveur, les demandes sont automatiquement rejetées sans arriver ici.

Ces deux types réutilisent les composants existants (`AskUserForm` pour elicitation, `HITLCard` pour sampling). Pas de nouvel onglet. Plus de détails : [Comprendre les permissions MCP](../integrations/comprendre-les-permissions-mcp.md).

> **Concept :** [book ch10 - HITL et contrôle](https://github.com/Apollia-OS/apollia-os/blob/main/book/src/ch10-00-hitl.md)
