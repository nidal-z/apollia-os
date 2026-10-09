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
| **Google Workspace** | Une dizaine de minutes dans la console Google Cloud d'abord. Vous inscrivez votre propre client OAuth et vous en confiez les identifiants à Apollia. |

Ce n'est pas un oubli côté Google. Google exige un écran de consentement vérifié avant qu'une application puisse servir des comptes hors de son propre projet, et ses clients de bureau portent en plus un secret qu'aucun binaire distribué ne peut détenir. Les clients publics de bureau de Microsoft n'exigent ni l'un ni l'autre. [Connecter Google Workspace](connecter-google-workspace.md) explique ce que coûte le statut Testing, notamment une reconnexion environ une fois par semaine, et [Créer un client OAuth Google](/how-to/set-up-a-google-oauth-client) nomme chaque écran de la console.

### Serveurs MCP

Le standard ouvert Model Context Protocol. Processus tiers, locaux (stdio via `npx` ou `uvx`) ou distants (HTTP/SSE), qui exposent des outils consommables par n'importe quel client MCP. Dans la feuille **Ajouter une connexion**, l'onglet **Catalogue des connexions** s'ouvre sur les connexions intégrées à Apollia (un dossier de votre ordinateur, Google Workspace, Microsoft 365, une boîte mail), puis sur la sélection qu'Apollia fournit, chaque entrée avec son libellé, sa description et un badge **Officiel** ou **Communauté**. Le registre MCP public n'est lu que si vous cliquez sur **Explorer tout** ; il est paginé au fil du défilement, sa taille est donc celle du registre le jour où vous le consultez. La sélection contient ces entrées :

Notion, Linear, Atlassian (Jira + Confluence), Stripe, Figma (Dev Mode), Sentry, Cloudflare, PostgreSQL, Heure, Récupération web, Mémoire (graphe de connaissances), Recherche Brave.

Vous pouvez aussi ajouter votre propre serveur, sur l'onglet **Connexion personnalisée** de la même feuille. Avec le mode développeur activé, les deux onglets s'appellent **Catalogue MCP** et **MCP personnalisé**. Modifier le catalogue lui-même n'est pas possible : le fichier de surcharge n'est lu par rien.

## Par où commencer

- Mail, calendrier, drive perso ou pro : voir [Connecter Google Workspace](connecter-google-workspace.md) ou [Connecter Microsoft 365](connecter-microsoft-365.md).
- Notion, Linear, Atlassian, Stripe, etc. : voir [Connecter un serveur MCP](connecter-un-serveur-mcp.md).
- Vos serveurs MCP internes : voir [Câbler son propre serveur MCP](cabler-son-propre-serveur-mcp.md).
- Adapter le catalogue à votre équipe n'est pas possible.

## Comment choisir

| Service | Connecteur natif | MCP officiel | Recommandation |
|---|---|---|---|
| Gmail, Google Calendar, Drive | Apollia | Aucun | Connecteur natif |
| Outlook, Calendar, OneDrive | Apollia | Aucun | Connecteur natif |
| Notion, Linear | Aucun | Officiel | Catalogue des connexions |
| Atlassian (Jira + Confluence) | Aucun | Atlassian Rovo | Catalogue des connexions |
| Stripe, Figma, Sentry, Cloudflare | Aucun | Officiel | Catalogue des connexions |
| Votre serveur interne | Aucun | À câbler | Connexion personnalisée |

## Garder le contrôle

- **Votre accord d'abord** : toute écriture (envoi d'un mail, création d'un événement, écriture d'un fichier) demande votre accord avant exécution. Voir [Comprendre les permissions MCP](understand-mcp-permissions.md).
- **Tokens locaux** : les tokens restent sur votre machine et ne sont envoyés qu'au service auquel ils appartiennent. Voir [Gérer les tokens OAuth](manage-oauth-tokens.md).
- **Politique de l'organisation** : interdire le cloud pour les connexions est une règle de la politique signée que votre organisation peut déployer, `connections.remote = "deny"`, vérifiée par le runtime. Sous cette règle, Apollia refuse d'ajouter une connexion à un service en ligne, un compte Google ou Microsoft compris, et l'annonce dans la fenêtre de connexion. Sans politique, vous êtes libre : aucune connexion n'est refusée parce qu'elle est en ligne. La politique est lue au démarrage d'Apollia : une connexion qu'elle interdit, déjà en place, est alors listée sous **Interdites par la politique** et ses outils sont refusés.

## Vérification

- La page **Connexions** s'ouvre et liste vos connexions.
- Le bouton **Ajouter une connexion** est visible en haut de la page, et ouvre une feuille avec un onglet **Catalogue des connexions** et un onglet **Connexion personnalisée**.

## Si ça ne marche pas

- **La page Connexions est vide ou ne charge pas** : redémarrez Apollia, le runtime n'a peut-être pas fini d'initialiser le client MCP.
- **Connecter un connecteur natif s'arrête sur « Refusé par la politique de l'organisation »** : la politique de votre organisation interdit les connexions à des services en ligne sur ce poste. Rien n'a été envoyé ; demandez une exception à la personne qui gère la politique.

> **Référence technique :** [Référence Apollia](/reference) , architecture du Tool Registry, scoping, gouvernance des outils.
