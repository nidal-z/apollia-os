# Vue d'ensemble des intégrations

> Pour les operators qui veulent comprendre comment Apollia donne accès à des services externes à leurs agents, et choisir entre connecteur natif et serveur MCP selon le service à brancher.

## Deux mécanismes complémentaires

Apollia distingue deux mécanismes pour donner accès à un service externe.

### Connecteurs natifs

Les connecteurs natifs sont des intégrations OAuth maintenues directement par Apollia pour les services qui n'exposent pas (encore) de serveur MCP officiel : **Google Workspace** (Gmail, Calendar, Drive Workspace) et **Microsoft 365** (Outlook Mail, Outlook Calendar, OneDrive).

Caractéristiques :

- **Tokens locaux** : les tokens OAuth sont stockés dans le keyring de votre OS. Apollia ne les transmet jamais à un serveur tiers.
- **Appels directs** : les requêtes API partent depuis votre machine vers `gmail.googleapis.com` ou `graph.microsoft.com`. Aucun relai cloud Apollia.
- **HITL natif** : toutes les opérations d'écriture (envoyer un mail, créer un événement, partager un fichier) demandent votre approbation explicite avant exécution.
- **Multi-compte** : vous pouvez connecter plusieurs comptes par provider (un compte personnel + un compte pro par exemple).

### Serveurs MCP

Les serveurs MCP (Model Context Protocol) sont des processus tiers — locaux (lancés via `npx`, `uvx`) ou distants (HTTP) — qui exposent des outils consommables par n'importe quel client MCP. Apollia inclut un catalogue de 18 entrées curées (Notion, Slack, GitHub, Linear, Atlassian, Stripe, etc.) et permet d'ajouter votre propre serveur MCP via le wizard.

Caractéristiques :

- **Standard ouvert** : conforme à la spec MCP 2025-11-25.
- **Catalogue extensible** : ajoutez vos propres entrées via `~/.apollia/mcp-overrides.json` ([voir doc](personnaliser-le-catalogue-mcp.md)).
- **Tokens** : chaque serveur gère son authentification — Apollia n'ajoute qu'une couche de stockage local pour les clés API saisies dans le wizard.

## Comment choisir ?

| Service | Connecteur natif | MCP officiel SaaS | Recommandation |
|---|---|---|---|
| Gmail / Google Calendar / Drive | ✅ Apollia | ❌ | Connecteur natif |
| Outlook / Calendar / OneDrive | ✅ Apollia | ❌ | Connecteur natif |
| Notion | ❌ | ✅ Notion | MCP officiel |
| Slack | ❌ | ✅ Slack | MCP officiel |
| Linear | ❌ | ✅ Linear | MCP officiel |
| GitHub | ❌ | ✅ GitHub | MCP officiel |
| Atlassian (Jira + Confluence) | ❌ | ✅ Atlassian Rovo | MCP officiel |
| Stripe / Figma / Sentry / Cloudflare | ❌ | ✅ Officiel | MCP officiel |
| Votre serveur interne | ❌ | À ajouter | MCP custom via wizard |

## Profil souveraineté

Si vous activez le profil `local_only` dans vos paramètres de souveraineté, les connecteurs natifs cloud et les serveurs MCP HTTP/SSE distants sont **désactivés**. Seuls les serveurs MCP stdio purement locaux (Filesystem, Memory, SQLite, Git, Time) restent disponibles. Les sessions actives sont préservées mais aucune nouvelle connexion ne peut être créée tant que le profil reste `local_only`.

## Pour aller plus loin

- [Connecter Google Workspace](connecter-google-workspace.md)
- [Connecter Microsoft 365](connecter-microsoft-365.md)
- [Connecter un serveur MCP](connecter-un-serveur-mcp.md)
- [Mode expert Google : scopes restricted](mode-expert-google-restricted-scopes.md)
- [Câbler votre propre serveur MCP](mcp-vous-avez-votre-propre-serveur.md)
- [Personnaliser le catalogue MCP](personnaliser-le-catalogue-mcp.md)
- [Gérer les tokens OAuth](gerer-les-tokens-oauth.md)
- [Comprendre les permissions MCP](comprendre-les-permissions-mcp.md)

> **Référence technique :** [Briques-MCP](https://github.com/nidal-z/apollia-os/wiki/Briques-MCP)
