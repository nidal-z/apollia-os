---
title: Comprendre les permissions MCP
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
| `auto_approve` | L'outil s'exécute immédiatement. | Lectures sans effet de bord, `gmail.list_drafts`, `outlook.search`, `gcal.list_events`. |
| `always_require_approval` | Une popup HITL apparaît, vous décidez. | Écritures, `gmail.send`, `outlook.send`, `gcal.create_event`. |
| `confirm_phrase` | Popup HITL + il faut taper une phrase de confirmation (le nom de la chose à supprimer par exemple). | Suppressions irréversibles, `gcal.delete_event`, `outlook_cal.delete_event`. |

Quand vous voyez la popup, vous pouvez cocher *"Toujours autoriser pour ce projet"*. Cela crée une **règle persistante** que vous retrouverez et pourrez révoquer dans **Paramètres, Permissions**.

Il n'existe pas d'écran d'approbations propre au MCP, ni de file d'attente de demandes MCP à traiter. Un outil exposé par un serveur MCP passe par la même porte qu'un outil natif, celle qu'ouvre la boucle d'exécution : la carte dans la conversation si vous y êtes, la **Boîte de réception** pour une tâche d'agent en attente. Tout ce que le reste de ce centre d'aide dit sur l'approbation d'une action s'y applique sans changement.

![Popup d'approbation dans le chat : le titre de l'outil, les paramètres exposés, les boutons Autoriser une fois et Refuser, et le menu Toujours autoriser](/img/operator-help/integration-comprendre-les-permissions-mcp-1.png)

## Voir et changer les règles

Ouvrez **Paramètres, Permissions**. Quatre sections :

- **Règles** : les règles persistantes, créées via les demandes d'approbation, par l'agent d'onboarding, ou à la main. Filtrables par portée (Toutes, Ce projet, Agent de chat, Partout) et par outil. L'auteur de la règle est affiché sur chaque ligne mais n'est pas un filtre.
- **Sessions actives** : autorisations valables uniquement pour la session de chat en cours.
- **Apollia Chat** : les règles qui s'appliquent spécifiquement au chat libre.
- **Audit récent** : les vingt dernières décisions d'outil, la plus récente en tête.

Bouton **Révoquer** sur chaque règle. Pour révoquer toutes les règles d'une portée d'un coup, le bouton **Révoquer tout** vous fait choisir la portée et affiche combien de règles seraient retirées, puis Annuler ou Révoquer. Il ne demande aucune confirmation à taper.

Le niveau d'approbation par serveur (`auto` / `ask` / `readonly`) n'est pas sur cette page : il se trouve dans **Connexions**, sur l'onglet **Réglages** du serveur.

![Page Paramètres, Autorisations : les règles de permission empilées avec un bouton Révoquer par ligne](/img/operator-help/integration-comprendre-les-permissions-mcp-2.png)

Une règle se crée de deux façons. La plupart du temps elle apparaît toute seule, quand vous répondez « Toujours autoriser » à une demande d'approbation. Vous pouvez aussi en créer une à la main depuis cette page, via le formulaire **Ajouter une règle**, ce qui permet d'autoriser quelque chose avant même qu'un agent le demande.

## Que fait le mode local-only

Le **profil de souveraineté** est une décision globale, indépendante des règles par outil.

- **`cloud_allowed`** (défaut) : tous les connecteurs cloud (Google, Microsoft) sont actifs, tous les serveurs MCP distants sont actifs.
- **`local_only`** : connecteurs Google et Microsoft désactivés, serveurs MCP HTTP et SSE distants désactivés. Seuls les serveurs MCP stdio purement locaux restent disponibles (Filesystem, Memory, SQLite, Git, Time).

Quand un agent tente d'utiliser un outil bloqué par le profil, il reçoit l'erreur `SovereigntyBlocked`. Il peut soit demander un changement de profil, soit choisir un outil alternatif.

En v0.1.0, le profil se règle côté configuration backend, pas encore via une bascule dans l'interface.

## Ce qu'un serveur MCP peut, et ne peut pas, vous demander

La spécification permet à un serveur de rappeler son client de trois façons.
Apollia en implémente une.

- **Roots**, implémenté : Apollia déclare au serveur les répertoires accessibles (workspace de l'agent, dossier projet). Le serveur ne voit rien d'autre côté filesystem.
- **Sampling**, non implémenté : un serveur ne peut pas demander à Apollia de faire un appel LLM pour lui.
- **Elicitation**, non implémenté : un serveur ne peut pas vous demander une saisie structurée.

Les deux capacités non implémentées ne sont pas annoncées pendant la poignée de
main : un serveur découvre donc leur absence à la connexion, et non en envoyant
une requête qui reste sans réponse. Les deux sont prévues, et passeront par votre
approbation le jour où elles arriveront.

## Vérification

- Ouvrez **Paramètres, Permissions**, les 3 sections s'affichent.
- Dans le chat, déclenchez une écriture (envoi de mail), la popup apparaît.
- Cochez *"Toujours autoriser pour ce projet"*, confirmez, vérifiez qu'une nouvelle ligne apparaît dans **Règles de permission**.

## Si ça ne marche pas

- **Un outil read-only demande une approbation alors que ce n'est pas attendu** : la policy par défaut a été durcie. Vérifiez dans **Autorisations** et restaurez le mode `auto`.
- **Un outil sensible s'exécute sans demande** : vous avez créé une règle de permission persistante en cochant la case un jour. Allez la révoquer.
- **`local_only` bloque mon MCP local** : vérifiez que votre MCP est bien en transport `stdio`. Un MCP en `http://localhost:...` est quand même bloqué (le profil filtre par transport, pas par host).

> **Référence technique :** [Référence Apollia](/reference) , gouvernance complète, audit trail, format des règles dans `governance.db`.
