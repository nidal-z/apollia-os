---
title: Vue d'ensemble des intégrations
slug: /operator-help/integrations/integrations-overview
sidebar_position: 1
---

# Vue d'ensemble des intégrations

> Pour tout operator qui veut comprendre les deux mécanismes d'extension d'Apollia, connecteurs natifs et serveurs MCP, et savoir par où commencer.

## Prérequis

- Apollia lancé, page **Connexions** accessible depuis la sidebar.
- Un compte chez le service que vous voulez brancher (Google, Microsoft, Notion, etc.) si l'intégration est authentifiée.

## Les deux familles

Apollia distingue deux mécanismes complémentaires.

### Connecteurs natifs OAuth

Maintenus directement par Apollia pour les services qui n'exposent pas (encore) de serveur MCP officiel : **Google Workspace** (Gmail, Calendar, Drive, Sheets, Docs, Slides, Forms, Tasks) et **Microsoft 365** (Outlook, Calendar, OneDrive).

- Tokens stockés dans le trousseau du système (Keychain macOS, Credential Manager Windows, Secret Service Linux).
- Appels directs depuis votre machine vers `gmail.googleapis.com` ou `graph.microsoft.com`, aucun relai cloud Apollia.
- Toute écriture demande votre accord avant de s'exécuter.
- Plusieurs comptes peuvent être connectés. Chacun est une connexion à part entière, accordée à un agent compte par compte.

**Les deux ne coûtent pas la même chose à démarrer.** Bon à savoir avant de cliquer, car la différence n'apparaît qu'une fois sur la page du connecteur :

| | Ce qu'il faut pour connecter |
|---|---|
| **Microsoft 365** | Rien. Apollia fournit l'identifiant de sa propre application inscrite : vous vous connectez, c'est fini. |
| **Google Workspace** | Une dizaine de minutes dans la console Google Cloud d'abord. Vous inscrivez votre propre client OAuth et vous en confiez les identifiants à Apollia, avec le mode développeur activé. |

Ce n'est pas un oubli côté Google. Google exige un écran de consentement vérifié avant qu'une application puisse servir des comptes hors de son propre projet, et ses clients de bureau portent en plus un secret qu'aucun binaire distribué ne peut détenir. Les clients publics de bureau de Microsoft n'exigent ni l'un ni l'autre. [Connecter Google Workspace](connecter-google-workspace.md) explique ce que coûte le statut Testing, notamment une reconnexion environ une fois par semaine, et [Créer un client OAuth Google](/how-to/set-up-a-google-oauth-client) nomme chaque écran de la console.

### Serveurs MCP

Le standard ouvert Model Context Protocol. Processus tiers, locaux (stdio via `npx` ou `uvx`) ou distants (HTTP/SSE), qui exposent des outils consommables par n'importe quel client MCP. **Ajouter une connexion** ouvre une seule fenêtre avec une seule recherche, dont les réponses viennent en quatre groupes : **Proposés par Apollia** (les comptes Google Workspace et Microsoft 365, puis la sélection de services, chaque entrée avec son libellé, sa description et un badge **Officiel** ou **Communauté**), **Sur cet ordinateur** (un dossier, une boîte mail), le **Catalogue public** et **Brancher un service par son adresse**. Le registre MCP public n'est lu que si vous cliquez sur **Explorer tout**, et ses entrées sont marquées **Non vérifié** ; il est paginé au fil du défilement, sa taille est donc celle du registre le jour où vous le consultez.

Apollia lit sa sélection dans un index signé, en ligne, à l'ouverture de la fenêtre : une entrée peut ainsi être ajoutée ou corrigée sans nouvelle version d'Apollia. Un index dont la signature ne tient pas est refusé, et la copie déjà gardée sur cet ordinateur, ou la sélection intégrée à Apollia, est montrée à la place. Le réglage `[mcp] catalogue_enabled = false` coupe l'index en ligne. La sélection contient ces entrées :

Notion, Linear, Atlassian (Jira + Confluence), Stripe, Figma (Dev Mode), Sentry, Cloudflare, PostgreSQL, Heure, Récupération web, Mémoire (graphe de connaissances), Recherche Brave.

Vous pouvez aussi ajouter votre propre serveur, avec **Brancher un service par son adresse**, toujours en dernier dans la fenêtre. Avec le mode développeur activé, le même formulaire accepte aussi une commande locale. Modifier le catalogue lui-même n'est pas possible depuis l'interface.

## Par où commencer

- Mail, calendrier, drive perso ou pro : voir [Connecter Google Workspace](connecter-google-workspace.md) ou [Connecter Microsoft 365](connecter-microsoft-365.md).
- Notion, Linear, Atlassian, Stripe, etc. : voir [Connecter un serveur MCP](connecter-un-serveur-mcp.md).
- Vos serveurs MCP internes : voir [Câbler son propre serveur MCP](cabler-son-propre-serveur-mcp.md).
- Une connexion déjà en place : voir [Vérifier une connexion et la réparer](test-an-mcp-connection.md) et [Mettre en pause ou retirer une connexion](pause-or-remove-a-connection.md).
- Adapter le catalogue à votre équipe n'est pas possible.

## Comment choisir

| Service | Connecteur natif | MCP officiel | Recommandation |
|---|---|---|---|
| Gmail, Google Calendar, Drive | Apollia | Aucun | Connecteur natif |
| Outlook, Calendar, OneDrive | Apollia | Aucun | Connecteur natif |
| Notion, Linear | Aucun | Officiel | Catalogue des connexions |
| Atlassian (Jira + Confluence) | Aucun | Atlassian Rovo | Catalogue des connexions |
| Stripe, Figma, Sentry, Cloudflare | Aucun | Officiel | Catalogue des connexions |
| Votre serveur interne | Aucun | À câbler | Brancher un service par son adresse |

## Garder le contrôle

- **Votre accord d'abord** : toute écriture (envoi d'un mail, création d'un événement, écriture d'un fichier) demande votre accord avant exécution. Voir [Comprendre les permissions MCP](understand-mcp-permissions.md).
- **Tokens locaux** : les tokens restent sur votre machine et ne sont envoyés qu'au service auquel ils appartiennent. Voir [Gérer les tokens OAuth](manage-oauth-tokens.md).
- **Politique de l'organisation** : interdire le cloud pour les connexions est une règle de la politique signée que votre organisation peut déployer, `connections.remote = "deny"`, vérifiée par le runtime. Sous cette règle, Apollia refuse d'ajouter une connexion à un service en ligne, un compte Google ou Microsoft compris, et l'annonce dans la fenêtre de connexion. Sans politique, vous êtes libre : aucune connexion n'est refusée parce qu'elle est en ligne. La politique est lue au démarrage d'Apollia : une connexion qu'elle interdit, déjà en place, indique alors **Bloquée par l'organisation** et ses outils sont refusés.
- **Pause** : une connexion mise en pause garde les accès de ses agents, et aucun de ses outils ne s'exécute avant sa reprise. Voir [Mettre en pause ou retirer une connexion](pause-or-remove-a-connection.md).
- **Secrets au coffre** : une clé tapée à l'ajout d'un service, un en-tête, et une valeur dont le nom la marque secrète partent au coffre de secrets de cet ordinateur, jamais dans les réglages de la connexion.

## Vérification

- La page **Connexions** s'ouvre et liste vos connexions, chacune avec son état.
- **Ajouter une connexion** ouvre une fenêtre avec une seule recherche, et ses quatre groupes de réponses.

## Si ça ne marche pas

- **La page Connexions affiche un bandeau** : un bandeau nomme ce qui n'a pas pu être lu, comme la liste des agents ou les descriptions du catalogue, et le reste de la page demeure. Redémarrez Apollia s'il ne disparaît pas.
- **Connecter un connecteur natif s'arrête sur « Refusé par la politique de l'organisation »** : la politique de votre organisation interdit les connexions à des services en ligne sur ce poste. Rien n'a été envoyé ; demandez une exception à la personne qui gère la politique.

> **Référence technique :** [Référence Apollia](/reference) , architecture du Tool Registry, scoping, gouvernance des outils.
