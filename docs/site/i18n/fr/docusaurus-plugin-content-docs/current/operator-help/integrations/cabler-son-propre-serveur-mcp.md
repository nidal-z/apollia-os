---
title: Câbler son propre serveur MCP
slug: /operator-help/integrations/wire-your-own-mcp-server
sidebar_position: 9
---

# Câbler son propre serveur MCP

> Pour tout operator ou builder qui veut connecter un serveur MCP qui n'est pas dans le catalogue, en local (stdio) ou distant (HTTP, SSE).

## Prérequis

- Apollia lancé.
- Un serveur MCP conforme à la spec, en transport **stdio** (sous-processus local), **streamable-http** ou **sse**.
- L'accès au serveur : commande locale + arguments, ou URL distante + en-têtes d'authentification.

## Étapes

1. Dans la barre latérale **Connexions**, cliquez sur **Ajouter un connecteur** en bas. Une feuille s'ouvre sur l'onglet **Catalogue MCP** ; passez sur l'onglet **MCP personnalisé**.

   ![Onglet Personnalisé du catalogue : le formulaire vierge](/img/operator-help/integration-cabler-son-propre-serveur-mcp-1.png)

2. Remplissez le formulaire selon le transport choisi (voir sous-sections).

3. Cliquez sur **Tester**. Apollia tente une connexion réelle et compte les outils déclarés par le serveur.

4. Si le test passe, cliquez sur **Installer**. Le serveur apparaît dans la sidebar.

### Cas stdio (commande locale)

- **Nom** : identifiant unique. Le champ ne refuse jamais ce que vous tapez, il le réécrit : passage en minuscules, et toute suite de caractères hors `a-z`, `0-9`, `_` et `-` remplacée par un tiret unique, tirets de tête et de queue supprimés. Les tirets bas sont conservés. Un nom vidé de tout devient `custom-mcp`. Ainsi `Mon Serveur !` est installé sous `mon-serveur`, en silence ; vérifiez ensuite le nom affiché dans la sidebar.
- **Transport** : `stdio`.
- **Commande** : exécutable à lancer (par exemple `npx`, `uvx`, ou un chemin absolu).
- **Arguments** : séparés par des espaces (par exemple `-y @modelcontextprotocol/server-filesystem ~/Documents`).
- **Exiger approbation** : cochez si vous voulez une approbation HITL à chaque appel d'outil.

![Formulaire Personnalisé en transport stdio, avec la commande et les arguments remplis](/img/operator-help/integration-cabler-son-propre-serveur-mcp-2.png)

### Cas streamable-http (serveur distant)

- **Nom** : identifiant unique.
- **Transport** : `streamable-http`.
- **URL** : endpoint HTTP du serveur (`https://...`).
- **En-têtes** (optionnel) : un par ligne au format `Nom-Header=valeur`. Exemple : `Authorization=Bearer sk-...` ou `X-API-Key=...`.

![Formulaire Personnalisé en transport streamable-http, avec l'URL et les en-têtes d'authentification](/img/operator-help/integration-cabler-son-propre-serveur-mcp-3.png)

### Cas SSE

Identique au cas streamable-http mais avec **Transport** : `sse`. Utilisé pour les serveurs qui maintiennent une connexion SSE persistante.

## OAuth 2.1, automatique

Si votre serveur MCP annonce un endpoint OAuth conforme à la spec d'autorisation MCP (RFC 9728 Protected Resource Metadata + RFC 8414 Authorization Server Metadata), Apollia gère seul :

1. La découverte des métadonnées (PRM puis OIDC fallback).
2. L'identification client via le CIMD Apollia, ou Dynamic Client Registration (RFC 7591) en fallback.
3. L'échange de code avec PKCE S256 et Resource Indicators (RFC 8707).
4. Le stockage du token dans le trousseau local et le refresh proactif avec singleflight.

Vous n'avez rien à configurer côté Apollia. Le serveur déclenche tout au premier 401.

## Découverte mDNS locale

Apollia peut découvrir des serveurs MCP sur votre réseau local via mDNS (service type `_apollia-mcp._tcp.local.`). Activez l'option dans **Connexions, Préférences** si votre serveur l'annonce.

## Vérification

- Pastille verte à côté du serveur dans la sidebar.
- La vue détail affiche les sections `tools`, `resources`, `prompts` renseignées avec ce que le serveur annonce.
- Un test annonce le serveur opérationnel ou joignable, avec son nombre d'outils (voir [Tester une connexion MCP](test-an-mcp-connection.md)). Aucune latence n'est mesurée sur le chemin MCP.

## Sécurité, ce qu'Apollia applique par défaut

- **Trust level** : un serveur ajouté à la main n'en porte aucun. Le niveau est une surcouche que seules les dix-huit entrées enrichies du catalogue portent, et aucun code n'attribue de niveau `custom` à quoi que ce soit : votre serveur n'affiche donc aucun badge. C'est cohérent avec la dernière section de cette page, où il apparaît dans la liste sans logo, sans description et sans badge de confiance.
- **Approbation HITL** : par défaut, l'outil est en mode *requires_approval*, chaque appel demande votre validation. Vous pouvez assouplir par outil dans la page [Comprendre les permissions MCP](understand-mcp-permissions.md).
- **Roots** : Apollia annonce la capacité pendant la poignée de main, et ne répond à aucune requête `roots/list`. Aucun répertoire n'est déclaré : roots ne borne donc rien aujourd'hui. Ce qui borne un serveur stdio, c'est la commande et les arguments que vous lui avez donnés, et c'est pourquoi le chemin passé dans **Arguments** est le vrai périmètre.
- **Sampling et elicitation** : non implémentés. Apollia n'annonce pas ces deux capacités pendant la poignée de main, si bien qu'un serveur qui les supporte n'essaiera pas de rappeler par ce biais.

## Mode de chargement deferred

Par défaut, Apollia charge les outils d'un serveur MCP en mode `deferred` : ils ne sont pas injectés en contexte au démarrage. L'agent utilise `tool_search` pour les récupérer à la demande. C'est le bon réglage pour la plupart des serveurs.

Si votre serveur expose peu d'outils (moins d'une dizaine) ou si vos agents les utilisent systématiquement à chaque exécution, vous pouvez passer en mode `eager` dans votre configuration :

```toml
[mcp]
tool_loading = "eager"
```

En mode `eager`, tous les outils du serveur sont chargés en contexte à chaque appel. Cela simplifie le comportement de l'agent mais augmente la consommation de tokens.

Le paramètre `tool_search_limit` borne le nombre d'outils renvoyés par `tool_search` en mode `deferred`, et sous cette même borne l'index est déclaré d'emblée à l'agent plutôt que cherché. Valeur par défaut : `20`. Plage valide : `1` à `500`. Une installation à deux serveurs passe généralement en dessous, ce qui explique que `deferred` s'y comporte comme `eager`.

```toml
[mcp]
tool_loading = "deferred"
tool_search_limit = 20
```

## Si ça ne marche pas

- **"Commande introuvable" en stdio** : votre binaire n'est pas dans le PATH d'Apollia. Donnez le chemin absolu ou ajustez votre PATH avant de lancer Apollia.
- **"Connexion refusée" en HTTP ou SSE** : URL ou port incorrects, ou firewall bloque la sortie. Vérifiez l'accessibilité depuis votre machine avec `curl <url>`.
- **OAuth en boucle** : votre serveur annonce mal son endpoint metadata, ou un scope refusé. Apollia refuse fail-fast les serveurs d'autorisation non conformes (PKCE S256 obligatoire). Vérifiez les logs côté serveur.
- **"Aucun outil détecté"** : le handshake réussit mais le serveur ne déclare pas la capability `tools` dans son `InitializeResult`. Vérifiez l'implémentation côté serveur.

## Faire apparaître votre serveur dans le catalogue UI

Votre serveur interne apparaît dans la liste une fois connecté, mais sans le logo, la description et le badge de niveau de confiance que portent les entrées officielles. Personnaliser le catalogue n'est pas possible en `v0.1.0-preview` : le fichier de surcharge n'est lu par rien.

> **Référence technique :** [Référence Apollia](/reference) , schéma complet du protocole, capabilities, transports, sécurité.
