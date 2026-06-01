# Vue d'ensemble des intégrations

> Pour tout operator qui veut comprendre les deux mécanismes d'extension d'Apollia, connecteurs natifs et serveurs MCP, et savoir par où commencer.

## Prérequis

- Apollia lancé, page **Connexions** accessible depuis la sidebar.
- Un compte chez le service que vous voulez brancher (Google, Microsoft, Notion, etc.) si l'intégration est authentifiée.

## Les deux familles

Apollia distingue deux mécanismes complémentaires.

### Connecteurs natifs OAuth

Maintenus directement par Apollia pour les services qui n'exposent pas (encore) de serveur MCP officiel : **Google Workspace** (Gmail, Calendar, Drive, Sheets, Docs, Slides, Forms, Tasks, YouTube) et **Microsoft 365** (Outlook, Calendar, OneDrive).

- Tokens stockés dans le trousseau du système (Keychain macOS, Credential Manager Windows, Secret Service Linux).
- Appels directs depuis votre machine vers `gmail.googleapis.com` ou `graph.microsoft.com`, aucun relai cloud Apollia.
- Approbation HITL automatique sur toutes les écritures.
- Multi-comptes supporté.

### Serveurs MCP

Le standard ouvert Model Context Protocol. Processus tiers, locaux (stdio via `npx` ou `uvx`) ou distants (HTTP/SSE), qui exposent des outils consommables par n'importe quel client MCP. Apollia inclut un catalogue de **18 entrées** curées :

Notion, Slack, GitHub, Linear, Atlassian, Stripe, Figma, Sentry, Cloudflare, PostgreSQL, SQLite, Git, Time, Fetch, Filesystem, Memory, Puppeteer, Brave Search.

Vous pouvez aussi ajouter vos propres serveurs ou modifier le catalogue.

`[SCREENSHOT: page Connexions, sidebar gauche avec liste connecteurs natifs et MCPs, panneau Aperçu, boutons "+ Découvrir" et "+ Ajouter personnalisé" en haut]`

## Par où commencer

- Mail, calendrier, drive perso ou pro : voir [Connecter Google Workspace](connecter-google-workspace.md) ou [Connecter Microsoft 365](connecter-microsoft-365.md).
- Notion, GitHub, Linear, Atlassian, Stripe, etc. : voir [Connecter un serveur MCP](connecter-un-serveur-mcp.md).
- Vos serveurs MCP internes : voir [Câbler son propre serveur MCP](cabler-son-propre-serveur-mcp.md).
- Adapter le catalogue à votre équipe : voir [Personnaliser le catalogue MCP](personnaliser-le-catalogue-mcp.md).

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

- **Approbation HITL** : toutes les écritures (envoi mail, création événement, écriture fichier) demandent votre confirmation avant exécution. Voir [Comprendre les permissions MCP](comprendre-les-permissions-mcp.md).
- **Tokens locaux** : aucun secret ne quitte votre machine. Voir [Gérer les tokens OAuth](gerer-les-tokens-oauth.md).
- **Profil de souveraineté** : Apollia accepte par défaut les connecteurs cloud (`cloud_allowed`). En profil `local_only`, les boutons de connexion cloud sont désactivés et seuls les MCPs stdio purement locaux restent disponibles. En v0.1.0, le profil se règle côté configuration backend (pas encore de bascule dans l'interface).

## Vérification

- La page **Connexions** s'ouvre et affiche la sidebar des connecteurs (vide si rien n'est encore branché).
- Les boutons **+ Découvrir** et **+ Ajouter personnalisé** sont visibles en haut.

## Si ça ne marche pas

- **La page Connexions est vide ou ne charge pas** : redémarrez Apollia, le runtime n'a peut-être pas fini d'initialiser le client MCP.
- **Le bouton Connecter d'un connecteur natif est grisé** : votre profil de souveraineté est `local_only`, voir la section précédente.
- **Vous voyez "Section en cours de refonte"** : votre application est antérieure à la v0.1.0, mettez à jour.

> **Référence technique :** [Briques-Tool-Registry](https://github.com/Apollia-OS/apollia-os/wiki/Briques-Tool-Registry) , architecture du Tool Registry, scoping, gouvernance des outils.
