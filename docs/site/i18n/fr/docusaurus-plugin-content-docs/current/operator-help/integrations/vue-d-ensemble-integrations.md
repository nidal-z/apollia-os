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
- Approbation HITL automatique sur toutes les écritures.
- Plusieurs comptes peuvent être connectés, et un appel d'outil utilise toujours le premier connecté.

**Les deux ne coûtent pas la même chose à démarrer.** Bon à savoir avant de cliquer, car la différence n'apparaît qu'une fois sur la page du connecteur :

| | Ce qu'il faut pour connecter |
|---|---|
| **Microsoft 365** | Rien. Apollia fournit l'identifiant de sa propre application inscrite : vous vous connectez, c'est fini. |
| **Google Workspace** | Une dizaine de minutes dans la console Google Cloud d'abord. Vous inscrivez votre propre client OAuth et vous en confiez les identifiants à Apollia. |

Ce n'est pas un oubli côté Google. Google exige un écran de consentement vérifié avant qu'une application puisse servir des comptes hors de son propre projet, et ses clients de bureau portent en plus un secret qu'aucun binaire distribué ne peut détenir. Les clients publics de bureau de Microsoft n'exigent ni l'un ni l'autre. [Connecter Google Workspace](connecter-google-workspace.md) explique ce que coûte le statut Testing, notamment une reconnexion environ une fois par semaine, et [Créer un client OAuth Google](/how-to/set-up-a-google-oauth-client) nomme chaque écran de la console.

### Serveurs MCP

Le standard ouvert Model Context Protocol. Processus tiers, locaux (stdio via `npx` ou `uvx`) ou distants (HTTP/SSE), qui exposent des outils consommables par n'importe quel client MCP. Ce que vous parcourez sous **Catalogue MCP** est le registre MCP public, paginé au fil du défilement : sa taille est donc celle du registre le jour où vous le consultez. Apollia ajoute sa propre couche de présentation (libellé operator, description, catégorie, niveau de confiance, aide à la connexion) à **18** de ces entrées :

Notion, Slack, GitHub, Linear, Atlassian (Jira + Confluence), Stripe, Figma (Dev Mode), Sentry, Cloudflare, PostgreSQL, SQLite, Git, Time, Fetch, Fichiers locaux, Mémoire (graphe de connaissances), Navigateur web, Recherche Brave.

Vous pouvez aussi ajouter votre propre serveur, sur l'onglet **MCP personnalisé** de la même feuille. Modifier le catalogue lui-même n'est pas possible en `v0.1.0-preview` : le fichier de surcharge n'est lu par rien.

![page Connexions, sidebar gauche listant les connecteurs natifs (Google Workspace, Microsoft 365) et les serveurs MCP, panneau de droite avec l'onglet Aperçu du connecteur sélectionné et le bouton Ajouter un connecteur en bas](/img/operator-help/integration-overview-1.png)

## Par où commencer

- Mail, calendrier, drive perso ou pro : voir [Connecter Google Workspace](connecter-google-workspace.md) ou [Connecter Microsoft 365](connecter-microsoft-365.md).
- Notion, GitHub, Linear, Atlassian, Stripe, etc. : voir [Connecter un serveur MCP](connecter-un-serveur-mcp.md).
- Vos serveurs MCP internes : voir [Câbler son propre serveur MCP](cabler-son-propre-serveur-mcp.md).
- Adapter le catalogue à votre équipe n'est pas possible en `v0.1.0-preview`.

## Comment choisir

| Service | Connecteur natif | MCP officiel | Recommandation |
|---|---|---|---|
| Gmail, Google Calendar, Drive | Apollia | Aucun | Connecteur natif |
| Outlook, Calendar, OneDrive | Apollia | Aucun | Connecteur natif |
| Notion, Slack, Linear, GitHub | Aucun | Officiel | MCP du catalogue |
| Atlassian (Jira + Confluence) | Aucun | Atlassian Rovo | MCP du catalogue |
| Stripe, Figma, Sentry, Cloudflare | Aucun | Officiel | MCP du catalogue |
| Votre serveur interne | Aucun | À câbler | MCP personnalisé |

## Garder le contrôle

- **Approbation HITL** : toutes les écritures (envoi mail, création événement, écriture fichier) demandent votre confirmation avant exécution. Voir [Comprendre les permissions MCP](understand-mcp-permissions.md).
- **Tokens locaux** : aucun secret ne quitte votre machine. Voir [Gérer les tokens OAuth](manage-oauth-tokens.md).
- **Profil de souveraineté** : le réglage est dans **Réglages, Profil**, sous **Souveraineté des données**, avec trois valeurs, *Local strict*, *Local préféré* et *Cloud autorisé*. Sur *Local strict*, et aussi tant que la question n'a jamais reçu de réponse, Apollia refuse d'ouvrir un flux OAuth cloud et l'annonce sur la page du connecteur. C'est tout ce que le profil verrouille en `v0.1.0-preview` : il ne filtre pas les serveurs MCP, donc un serveur HTTP ou SSE distant déjà installé continue de répondre sous ce profil.

## Vérification

- La page **Connexions** s'ouvre et affiche la sidebar des connecteurs (vide si rien n'est encore branché).
- Le bouton **Ajouter un connecteur** est visible en bas de cette sidebar, et ouvre une feuille avec un onglet **Catalogue MCP** et un onglet **MCP personnalisé**.

## Si ça ne marche pas

- **La page Connexions est vide ou ne charge pas** : redémarrez Apollia, le runtime n'a peut-être pas fini d'initialiser le client MCP.
- **Connecter un connecteur natif renvoie une erreur de souveraineté** : le message dit *Profil souveraineté « local-only » : connecteurs cloud désactivés*. Le bouton n'est pas grisé, le refus arrive au clic. Ouvrez **Réglages, Profil** et mettez **Souveraineté des données** sur *Local préféré* ou *Cloud autorisé*.

> **Référence technique :** [Référence Apollia](/reference) , architecture du Tool Registry, scoping, gouvernance des outils.
