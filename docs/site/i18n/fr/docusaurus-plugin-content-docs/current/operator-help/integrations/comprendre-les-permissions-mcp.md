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

![Popup d'approbation dans le chat : le titre de l'outil, les paramètres exposés, les boutons Autoriser une fois et Refuser, et le menu Toujours autoriser](/img/operator-help/integration-comprendre-les-permissions-mcp-1.png)

## Voir et changer les règles

Ouvrez **Paramètres, Permissions**. Trois sections :

- **Autorisations** : par MCP installé, vous pouvez régler le niveau d'approbation global (`auto` / `ask` / `readonly`).
- **Règles de permission** : règles persistantes créées via les popups HITL ou par les agents d'onboarding. Filtrables par origine (Onboarding, HITL utilisateur, Settings, Config import).
- **Sessions actives** : autorisations valables uniquement pour la session de chat en cours.

Bouton **Révoquer** sur chaque règle. Pour révoquer toutes les règles d'un scope d'un coup, le bouton **Révoquer tout** demande de taper votre nom en confirmation.

![Page Paramètres, Autorisations : les règles de permission empilées avec un bouton Révoquer par ligne](/img/operator-help/integration-comprendre-les-permissions-mcp-2.png)

Une règle se crée de deux façons. La plupart du temps elle apparaît toute seule, quand vous répondez « Toujours autoriser » à une demande d'approbation. Vous pouvez aussi en créer une à la main depuis cette page, via le formulaire **Ajouter une règle**, ce qui permet d'autoriser quelque chose avant même qu'un agent le demande.

## Que fait le mode local-only

Le **profil de souveraineté** est une décision globale, indépendante des règles par outil.

- **`cloud_allowed`** (défaut) : tous les connecteurs cloud (Google, Microsoft) sont actifs, tous les serveurs MCP distants sont actifs.
- **`local_only`** : connecteurs Google et Microsoft désactivés, serveurs MCP HTTP et SSE distants désactivés. Seuls les serveurs MCP stdio purement locaux restent disponibles (Filesystem, Memory, SQLite, Git, Time).

Quand un agent tente d'utiliser un outil bloqué par le profil, il reçoit l'erreur `SovereigntyBlocked`. Il peut soit demander un changement de profil, soit choisir un outil alternatif.

En v0.1.0, le profil se règle côté configuration backend, pas encore via une bascule dans l'interface.

## Cas particulier des serveurs MCP, sampling et elicitation

Les serveurs MCP ont trois capabilities qui passent par votre boîte de réception :

- **Sampling** : un serveur peut demander à Apollia de faire un appel LLM via `sampling/createMessage`. Le prompt arrive dans la boîte de réception, vous approuvez ou refusez. Le débit dépend du modèle utilisé par Apollia.
- **Elicitation** : un serveur peut demander un input utilisateur structuré via `elicitation/create`. Un formulaire arrive dans la boîte de réception.
- **Roots** : Apollia déclare au serveur les répertoires accessibles (workspace de l'agent, dossier projet). Le serveur ne voit rien d'autre côté filesystem.

## Vérification

- Ouvrez **Paramètres, Permissions**, les 3 sections s'affichent.
- Dans le chat, déclenchez une écriture (envoi de mail), la popup apparaît.
- Cochez *"Toujours autoriser pour ce projet"*, confirmez, vérifiez qu'une nouvelle ligne apparaît dans **Règles de permission**.

## Si ça ne marche pas

- **Un outil read-only demande une approbation alors que ce n'est pas attendu** : la policy par défaut a été durcie. Vérifiez dans **Autorisations** et restaurez le mode `auto`.
- **Un outil sensible s'exécute sans demande** : vous avez créé une règle de permission persistante en cochant la case un jour. Allez la révoquer.
- **`local_only` bloque mon MCP local** : vérifiez que votre MCP est bien en transport `stdio`. Un MCP en `http://localhost:...` est quand même bloqué (le profil filtre par transport, pas par host).

> **Référence technique :** [Référence Apollia](/reference) , gouvernance complète, audit trail, format des règles dans `governance.db`.
