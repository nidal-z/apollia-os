---
title: Comprendre les permissions MCP
slug: /operator-help/integrations/understand-mcp-permissions
sidebar_position: 7
---

# Comprendre les permissions MCP

> Pour tout operator qui veut savoir pourquoi un outil demande une approbation, comment changer une règle, et ce que fait le mode local-only.

## Prérequis

- Au moins un connecteur ou un serveur MCP connecté.
- Vous connaissez la différence entre connecteur natif et serveur MCP (voir [Vue d'ensemble des intégrations](vue-d-ensemble-integrations.md)).

## Pourquoi cet outil demande une approbation

Chaque outil exposé par un connecteur ou un MCP a une **policy d'approbation** :

| Policy | Comportement | Cas d'usage |
|---|---|---|
| `auto_approve` | L'outil s'exécute immédiatement. | Lectures sans effet de bord, `gcal.list_events`, `gdrive.workspace_list`, `outlook.search`. |
| `always_require_approval` | Une popup HITL apparaît, vous décidez. | Écritures, `gmail.send`, `outlook.send`, `gcal.create_event`. |
| `confirm_phrase` | Popup HITL + il faut taper une phrase de confirmation (le nom de la chose à supprimer par exemple). | Suppressions irréversibles, `gcal.delete_event`, `outlook_cal.delete_event`. |

La carte porte trois boutons, **Autoriser une fois**, **Refuser** et **Toujours autoriser**. La portée est un choix distinct, proposé à côté : *Pour cette session*, *Toujours pour cet assistant*, *Toujours pour ce projet* (indisponible si la session n'est rattachée à aucun projet) ou *Toujours, partout*. Répondre **Toujours autoriser** sur une portée autre que la session crée une **règle persistante** que vous retrouverez et pourrez révoquer dans **Paramètres, Permissions**.

Il n'existe pas d'écran d'approbations propre au MCP, ni de file d'attente de demandes MCP à traiter. Un outil exposé par un serveur MCP passe par la même porte qu'un outil natif, celle qu'ouvre la boucle d'exécution : la carte dans la conversation si vous y êtes, la **Boîte de réception** pour une tâche d'agent en attente. Tout ce que le reste de ce centre d'aide dit sur l'approbation d'une action s'y applique sans changement.

![Popup d'approbation dans le chat : le titre de l'outil, les paramètres exposés, les boutons Autoriser une fois, Refuser et Toujours autoriser, et le choix de portée](/img/operator-help/integration-comprendre-les-permissions-mcp-1.png)

## Voir et changer les règles

Ouvrez **Paramètres, Permissions**. Quatre sections :

- **Règles** : les règles persistantes, créées via les demandes d'approbation, par l'agent d'onboarding, ou à la main. Filtrables par portée (Toutes, Ce projet, Agent de chat, Partout) et par outil. L'auteur de la règle est affiché sur chaque ligne mais n'est pas un filtre.
- **Sessions actives** : autorisations valables uniquement pour la session de chat en cours.
- **Apollia Chat** : les règles qui s'appliquent spécifiquement au chat libre.
- **Audit récent** : une lecture seule de la table `permission_audit`. Rien dans le runtime n'écrit dans cette table en `v0.1.0-preview`, donc la section reste vide quel que soit le nombre d'approbations auxquelles vous répondez. Ce qui a réellement tourné se lit sur l'onglet **Audit Trail** de la page **Observabilité**.

Bouton **Révoquer** sur chaque règle. Pour révoquer toutes les règles d'une portée d'un coup, le bouton **Révoquer tout** vous fait choisir la portée et affiche combien de règles seraient retirées, puis Annuler ou Révoquer. Il ne demande aucune confirmation à taper.

Le niveau d'approbation par serveur n'est pas sur cette page : il se trouve dans **Connexions**, sur l'onglet **Réglages** du serveur. Il propose deux choix, *Autoriser automatiquement* et *Me demander à chaque fois*. Un niveau lecture seule a été retiré volontairement : il enregistrait le même octet que *Autoriser automatiquement*, donc le libellé le plus restrictif produisait le réglage le moins protecteur.

![Page Paramètres, Autorisations : les règles de permission empilées avec un bouton Révoquer par ligne](/img/operator-help/integration-comprendre-les-permissions-mcp-2.png)

Une règle se crée de deux façons. La plupart du temps elle apparaît toute seule, quand vous répondez « Toujours autoriser » à une demande d'approbation. Vous pouvez aussi en créer une à la main depuis cette page, via le formulaire **Ajouter une règle**, ce qui permet d'autoriser quelque chose avant même qu'un agent le demande.

## Ce que fait le profil de souveraineté

Le **profil de souveraineté** est une décision globale, indépendante des règles par outil.

Il se règle dans **Réglages, Profil**, sous **Souveraineté des données**, et prend l'une de trois valeurs.

- **Cloud autorisé** et **Local préféré** : ouvrir une connexion OAuth cloud est permis.
- **Local strict** : ouvrir une connexion OAuth cloud est refusé, et il l'est aussi tant que le réglage n'a jamais reçu de réponse, un réglage sensible sans réponse ne valant pas accord.

C'est tout ce que le profil applique en `v0.1.0-preview`. Il est vérifié à un seul endroit, au clic sur **Connecter** d'un connecteur natif, et nulle part ailleurs. Il ne filtre **pas** les serveurs MCP : un serveur HTTP ou SSE distant déjà installé reste joignable sous *Local strict*, et aucun agent ne reçoit d'erreur de souveraineté, puisqu'aucune n'est levée sur le chemin des outils. Voyez le profil comme une barrière à la connexion d'un compte cloud, pas comme une frontière réseau.

## Ce qu'un serveur MCP peut, et ne peut pas, vous demander

La spécification permet à un serveur de rappeler son client de trois façons.
Apollia n'en honore aucune aujourd'hui, et en annonce une.

- **Roots**, annoncé mais sans réponse : Apollia déclare la capacité pendant la poignée de main, et rien ne répond à une requête `roots/list`. Aucun répertoire n'est déclaré : ce n'est donc pas une frontière filesystem et il ne faut pas la lire comme telle. Ce qui borne réellement un serveur local, c'est la commande et les arguments que vous lui avez donnés.
- **Sampling**, non implémenté : un serveur ne peut pas demander à Apollia de faire un appel LLM pour lui.
- **Elicitation**, non implémenté : un serveur ne peut pas vous demander une saisie structurée.

Les deux dernières ne sont pas annoncées pendant la poignée de main : un serveur
découvre donc leur absence à la connexion, et non en envoyant une requête qui
reste sans réponse. Roots est le seul cas où l'annonce devance l'implémentation ;
les trois sont prévues, et passeront toutes par votre approbation le jour où
elles arriveront.

## Vérification

- Ouvrez **Paramètres, Permissions**, les quatre sections s'affichent.
- Dans le chat, déclenchez une écriture (envoi de mail), la popup apparaît.
- Répondez **Toujours autoriser** avec la portée *Toujours pour ce projet*, confirmez, vérifiez qu'une nouvelle ligne apparaît dans **Règles**.

## Si ça ne marche pas

- **Un outil read-only demande une approbation alors que ce n'est pas attendu** : la policy par défaut a été durcie. Vérifiez dans **Autorisations** et restaurez le mode `auto`.
- **Un outil sensible s'exécute sans demande** : vous avez répondu **Toujours autoriser** un jour, sur une portée plus large que la session. Allez révoquer cette règle.
- **Un serveur MCP ne répond plus après un changement de souveraineté** : le profil n'est pas en cause. Il ne filtre aucun transport MCP, local ou distant. Regardez le serveur lui-même, via [Tester une connexion MCP](test-an-mcp-connection.md).

> **Référence technique :** [Référence Apollia](/reference) , gouvernance complète, audit trail, format des règles dans `governance.db`.
