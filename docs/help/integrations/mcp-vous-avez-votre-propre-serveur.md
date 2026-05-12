# Câbler votre propre serveur MCP

> **Référence technique :** [Briques-MCP-Client](https://github.com/nidal-z/apollia-os/wiki/Briques-MCP-Client)

Apollia est un client MCP compliant 2025-11-25. Vous pouvez brancher n'importe quel serveur MCP — communautaire, officiel SaaS, ou interne à votre entreprise — sans modifier Apollia.

## Cas d'usage type

- **Serveur MCP interne enterprise** : votre équipe expose vos systèmes propriétaires (Salesforce custom, base interne, Confluence on-prem) à travers un serveur MCP que vous hébergez.
- **Serveur MCP communautaire** : un package npm/pypi qui couvre un service que vous utilisez (ex. `mcp-server-gmail-community`).
- **Prototype local** : un serveur MCP que vous développez et testez en stdio.

## Trois modes de transport

| Transport | Quand l'utiliser | Configuration |
|---|---|---|
| **stdio** | Serveur lancé en sous-process local (npx, uvx, binaire). | `command` + `args` + variables d'env. |
| **Streamable HTTP** | Serveur distant exposé en HTTP avec un endpoint unique. | `url` + headers d'auth optionnels. |
| **SSE legacy** | Serveur ancien deux-endpoints (déprécié spec, supporté pour compat). | `url` + headers. |

## Procédure d'ajout

1. Ouvrez **Connexions** dans Apollia Desktop.
2. Cliquez **+ Ajouter un serveur MCP custom**.
3. Le wizard 5 étapes vous guide :
   - **Transport** : stdio / streamable-http / sse.
   - **Auth** : headers (Authorization Bearer, X-API-Key, etc.) ou OAuth si supporté.
   - **Paramètres** : command + args (stdio) ou URL (HTTP/SSE).
   - **Test** : Apollia connecte le serveur, lit ses tools, resources, prompts et affiche le résultat.
   - **Disclaimer** : confirmation que vous comprenez les implications de sécurité.
4. Le serveur apparaît dans la liste des connexions actives.

## OAuth 2.1 automatique pour les serveurs HTTP

Si votre serveur MCP HTTP retourne un `WWW-Authenticate: Bearer resource_metadata="…"` sur la première requête sans token, Apollia déclenche **automatiquement** le flow OAuth 2.1 :

1. Fetch du document Protected Resource Metadata (RFC 9728).
2. Discovery de l'authorization server (RFC 8414 puis OIDC fallback).
3. Identification client via CIMD (`https://apollia.fr/.well-known/mcp-client-metadata`) ou DCR (RFC 7591) en fallback.
4. Code flow avec PKCE S256 + Resource Indicators (RFC 8707 MUST).
5. Token stocké dans le keyring local, refresh automatique avec singleflight.

Vous n'avez **rien à configurer** pour un serveur MCP qui respecte la spec MCP authorization 2025-11-25.

## Auto-discovery via mDNS

Apollia peut découvrir des serveurs MCP sur votre réseau local via mDNS (service type `_apollia-mcp._tcp.local.`). Activez l'option dans **Connexions → Préférences**.

## Sécurité

Lors de l'ajout d'un serveur MCP custom :

- **Trust level** : le wizard marque les serveurs ajoutés manuellement comme `community` ou `custom`. Aucun serveur tiers n'est traité comme `verified_official` automatiquement.
- **HITL approval** : par défaut, tous les outils du serveur sont en mode "requires_approval" (chaque appel demande votre validation). Vous pouvez assouplir par tool dans `governance.db`.
- **Roots** : Apollia annonce ses roots filesystem au serveur via `roots/list`. Le serveur ne voit que les répertoires que vous avez autorisés (workspace de l'agent, projet courant).
- **Sampling** : si le serveur demande un appel LLM via `sampling/createMessage`, vous voyez le prompt dans l'inbox et devez approuver avant exécution (rate-limit à 100 sampling/heure par serveur par défaut).
- **Elicitation** : si le serveur veut une saisie utilisateur via `elicitation/create`, elle arrive dans votre boîte de réception comme une demande HITL standard.

## Configuration TOML directe

En CLI, vous pouvez aussi ajouter un serveur via :

```bash
apollia mcp add-server --name acme-internal \
    --transport http \
    --url https://mcp.acme.internal \
    --header "Authorization=Bearer $ACME_TOKEN"
```

Et le mettre dans `~/.apollia/config.toml` :

```toml
[[mcp_servers]]
name = "acme-internal"
transport = "streamable-http"
url = "https://mcp.acme.internal"
requires_approval = true

[mcp_servers.env]
ACME_TOKEN = "${ACME_TOKEN}"
```

## Override du catalogue

Pour faire apparaître votre serveur interne dans le catalogue de l'UI Apollia (avec un logo, une description), utilisez le mécanisme [override du catalogue](personnaliser-le-catalogue-mcp.md).
