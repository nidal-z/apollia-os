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

1. Dans **Connexions**, cliquez sur **Ajouter une connexion** en haut de la page. Une feuille s'ouvre sur l'onglet **Catalogue des connexions** ; passez sur l'onglet **Connexion personnalisée**.

   ![Onglet Connexion personnalisée : le formulaire vierge, affiché avec le mode développeur activé](/img/operator-help/integration-cabler-son-propre-serveur-mcp-1.png)

2. Remplissez le formulaire. Sans le mode développeur, il demande un nom, une adresse et des en-têtes facultatifs : c'est le cas distant ci-dessous. Avec le mode développeur activé, un choix **Transport** apparaît et ouvre les deux autres cas.

3. Cliquez sur **Tester**. Apollia tente une connexion réelle et compte les outils déclarés par le serveur.

4. Si le test passe, cliquez sur **Installer**. Le serveur apparaît dans la liste des connexions.

### Cas stdio (commande locale)

- **Nom** : identifiant unique. Le champ ne refuse jamais ce que vous tapez, il le réécrit : passage en minuscules, et toute suite de caractères hors `a-z`, `0-9`, `_` et `-` remplacée par un tiret unique, tirets de tête et de queue supprimés. Les tirets bas sont conservés. Un nom vidé de tout devient `custom-mcp`. Ainsi `Mon Serveur !` est installé sous `mon-serveur`, en silence ; vérifiez ensuite le nom affiché dans la liste.
- **Transport** : `stdio (subprocess local)`, proposé avec le mode développeur activé.
- **Commande** : exécutable à lancer (par exemple `npx`, `uvx`, ou un chemin absolu).
- **Arguments** : séparés par des espaces (par exemple `-y @modelcontextprotocol/server-filesystem ~/Documents`).
- **Variables d'environnement** (facultatif) : une par ligne, au format `NOM=valeur`.

![Formulaire Personnalisé en transport stdio, avec la commande et les arguments remplis](/img/operator-help/integration-cabler-son-propre-serveur-mcp-2.png)

### Cas streamable-http (serveur distant)

- **Nom** : identifiant unique.
- **Transport** : `Streamable HTTP`. C'est le seul sans le mode développeur, et le choix n'est alors pas affiché.
- **Adresse** : endpoint HTTP du serveur (`https://...`).
- **Headers HTTP** (facultatif) : un par ligne, au format `NOM=valeur`. Exemple : `Authorization=Bearer sk-...` ou `X-API-Key=...`.

![Formulaire Personnalisé en transport streamable-http, avec l'URL et les en-têtes d'authentification](/img/operator-help/integration-cabler-son-propre-serveur-mcp-3.png)

### Cas SSE

Identique au cas streamable-http mais avec **Transport** : `SSE`, proposé avec le mode développeur activé. Utilisé pour les serveurs qui maintiennent une connexion SSE persistante.

## OAuth 2.1, depuis le catalogue seulement

Quand un serveur du catalogue annonce un endpoint OAuth conforme à la spec d'autorisation MCP (RFC 9728 Protected Resource Metadata + RFC 8414 Authorization Server Metadata), la fenêtre qui l'ajoute gère seule la connexion :

1. La découverte des métadonnées (PRM puis OIDC fallback).
2. L'identification client via le CIMD Apollia, ou Dynamic Client Registration (RFC 7591) en fallback.
3. L'échange de code avec PKCE S256 et Resource Indicators (RFC 8707).
4. Le stockage du token dans le trousseau local et le refresh proactif avec singleflight.

Le formulaire personnalisé ne lance pas ce parcours. Quand son **Tester** rencontre un serveur qui exige OAuth, il répond « Ce serveur exige une authentification OAuth. Installez-le depuis le catalogue plutôt que via le formulaire personnalisé. » Un serveur qui accepte un jeton statique se câble ici en plaçant le jeton dans **Headers HTTP**.

## Découverte mDNS locale

Apollia peut chercher sur votre réseau local les serveurs MCP qui s'annoncent via mDNS (service type `_apollia-mcp._tcp.local.`). Aucune option de l'interface ne lance cette recherche : c'est une commande, `apollia-os mcp list --discover`, qui liste les serveurs configurés et y ajoute ceux qu'elle a trouvés sur le réseau.

## Vérification

- Le serveur figure dans la liste des connexions avec un badge **actif**.
- L'onglet **Outils** de sa fiche liste les outils que le serveur annonce.
- Un test annonce le serveur opérationnel ou joignable, avec son nombre d'outils (voir [Tester une connexion MCP](test-an-mcp-connection.md)). Aucune latence n'est mesurée sur le chemin MCP.

## Sécurité, ce qu'Apollia applique par défaut

- **Présentation** : un serveur ajouté à la main est affiché comme une **Connexion personnalisée**. Il apparaît dans la liste sans le logo ni la description que portent les entrées de la sélection d'Apollia.
- **Approbation** : le serveur ne porte aucun réglage d'approbation qui lui soit propre. Chaque outil qu'il publie se décide sur sa famille d'action : ce que le serveur déclare dans les annotations de l'outil distingue une lecture d'une écriture, et un outil qu'il ne décrit pas est traité comme sensible. Un agent qui appelle un outil sensible demande d'abord, et vous répondez une fois, toujours pour cet agent, ou vous refusez.
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

Votre serveur interne apparaît dans la liste une fois connecté, mais sans le logo ni la description que portent les entrées de la sélection d'Apollia. Personnaliser le catalogue n'est pas possible : le fichier de surcharge n'est lu par rien.

> **Référence technique :** [Référence Apollia](/reference) , schéma complet du protocole, capabilities, transports, sécurité.
