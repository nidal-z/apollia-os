---
title: Câbler son propre serveur MCP
slug: /operator-help/integrations/wire-your-own-mcp-server
sidebar_position: 9
---

# Câbler son propre serveur MCP

> Pour tout opérateur ou builder qui veut brancher un serveur MCP absent du catalogue, par son adresse, ou comme commande locale avec le mode développeur activé.

## Prérequis

- Apollia lancé.
- Un serveur MCP conforme à la spec : joint à une adresse (**streamable-http**, ou **sse** avec le mode développeur activé), ou lancé comme commande locale (**stdio**, avec le mode développeur activé).
- Ce dont le serveur a besoin : son adresse et, s'il en demande une, sa clé ; ou la commande et ses arguments.

## Étapes

1. Dans **Connexions**, cliquez sur **Ajouter une connexion**. Dans la fenêtre, choisissez **Brancher un service par son adresse**, toujours en dernier dans la liste quelle que soit la recherche. Son formulaire s'ouvre aussitôt.
2. Remplissez le formulaire, comme décrit plus bas.
3. Cliquez sur **Tester**. Apollia tente une vraie connexion et compte les outils déclarés par le serveur.
4. Cliquez sur **Installer**. L'ajout se poursuit comme pour toute connexion : **Qui peut s'en servir**, où vous cochez les agents qui reçoivent ses outils, puis **Essai**. Voir [Connecter un serveur MCP depuis le catalogue](connecter-un-serveur-mcp.md).

À l'écran : le formulaire d'un service par son adresse, avec ses champs Nom et Adresse et les boutons Tester et Installer.

### Un serveur joint à son adresse

- **Nom (interne, unique)** : facultatif. Laissé vide, le serveur prend le nom de l'hôte de son adresse. Ce que vous tapez est réécrit : passé en minuscules, et chaque suite de caractères hors `a-z`, `0-9`, `_` et `-` remplacée par un seul tiret, tirets de début et de fin retirés. Ainsi `Mon Serveur!` est installé sous `mon-serveur` ; vérifiez ensuite le nom affiché dans la liste.
- **Adresse** : le point d'accès HTTP du serveur (`https://...`).
- **Clé d'accès** : le champ apparaît quand le test constate que le serveur demande une clé, ou quand vous cochez **Le service m'a donné une clé**. La clé est envoyée au serveur comme son autorisation, et rangée dans le coffre de secrets de cet ordinateur, jamais dans les réglages de la connexion.

### Avec le mode développeur activé

Un choix **Transport** apparaît, avec trois valeurs : **Streamable HTTP**, **SSE**, et **stdio (subprocess local)**.

- **stdio** : **Commande**, l'exécutable à lancer (par exemple `npx`, `uvx`, ou un chemin absolu) ; **Arguments**, séparés par des espaces (par exemple `-y @modelcontextprotocol/server-filesystem ~/Documents`) ; **Variables d'environnement**, une par ligne au format `NOM=valeur`.
- **Streamable HTTP** et **SSE** : l'**Adresse**, et les **En-têtes HTTP**, un par ligne au format `NOM=valeur`, par exemple `X-API-Key=...`.

### Où vont les secrets

Une valeur tapée comme en-tête, et la valeur d'une variable dont le nom la marque secrète (il contient `KEY`, `TOKEN`, `SECRET`, `PASSWORD`, `AUTH` ou un mot voisin), part au coffre de secrets de cet ordinateur quand le serveur est ajouté ou que ses réglages sont enregistrés. Les réglages de la connexion n'en gardent qu'une référence. Un serveur ajouté avec une version précédente, dont les valeurs étaient gardées dans ses réglages, les voit déplacées au coffre au premier lancement de cette version.

## OAuth 2.1, depuis le catalogue seulement

Quand un serveur du catalogue annonce un endpoint OAuth conforme à la spec d'autorisation MCP (RFC 9728 Protected Resource Metadata + RFC 8414 Authorization Server Metadata), la fenêtre qui l'ajoute gère seule la connexion :

1. La découverte des métadonnées (PRM puis OIDC fallback).
2. L'identification client via le CIMD Apollia, ou Dynamic Client Registration (RFC 7591) en fallback.
3. L'échange de code avec PKCE S256 et Resource Indicators (RFC 8707).
4. Le stockage du jeton dans le trousseau local et son rafraîchissement proactif, avec singleflight.

Le formulaire d'un service par son adresse ne lance pas ce parcours. Quand son **Tester** rencontre un serveur qui exige OAuth, il répond « Ce serveur exige une authentification OAuth. Installez-le depuis le catalogue plutôt que via le formulaire personnalisé. » Un serveur qui accepte une clé statique se câble ici avec **Clé d'accès**.

## Découverte mDNS locale

Apollia peut chercher sur votre réseau local les serveurs MCP qui s'annoncent via mDNS (service type `_apollia-mcp._tcp.local.`). Aucune option de l'interface ne lance cette recherche : c'est une commande, `apollia-os mcp list --discover`, qui liste les serveurs configurés et y ajoute ceux qu'elle a trouvés sur le réseau.

## Vérification

- Le serveur figure dans la liste des connexions, sous **Services**.
- Sa fiche liste les outils que le serveur annonce, chacun avec sa famille.
- **Vérifier** sur sa fiche l'annonce **Fonctionne** (voir [Vérifier une connexion et la réparer](test-an-mcp-connection.md)).

## Sécurité, ce qu'Apollia applique par défaut

- **Présentation** : un serveur ajouté ainsi apparaît dans la liste sans le logo ni la description que portent les entrées de la sélection d'Apollia.
- **Approbation** : le serveur ne porte aucun réglage d'approbation qui lui soit propre. Chaque outil qu'il publie se décide sur sa famille d'action : ce que le serveur déclare dans les annotations de l'outil distingue une lecture d'une écriture, et un outil qu'il ne décrit pas est traité comme sensible. Sur la fiche, **Changer** donne une autre famille à un outil quand vous en savez davantage ; le changement est inscrit au **Journal**. Un agent qui appelle un outil sensible demande d'abord, et vous répondez une fois, toujours pour cet agent, ou vous refusez.
- **Roots** : non implémenté. Apollia n'annonce pas la capacité pendant la poignée de main et ne déclare aucun répertoire : roots ne borne donc rien. Ce qui borne un serveur stdio, c'est la commande et les arguments que vous lui avez donnés, et c'est pourquoi le chemin passé dans **Arguments** est le vrai périmètre.
- **Sampling et elicitation** : non implémentés. Apollia n'annonce pas ces deux capacités pendant la poignée de main, si bien qu'un serveur qui les supporte n'essaiera pas de rappeler par ce biais.

## Quand le serveur tourne

Le serveur n'est pas gardé en marche. Il démarre au premier appel de l'un de ses outils et s'arrête après 15 minutes sans appel, par défaut ; ses outils restent listés entre les deux. La façon dont ses outils sont présentés à l'agent, tous d'emblée ou trouvés par `tool_search`, est un réglage à part. Voir [Configurer le chargement différé des outils MCP](configure-deferred-mcp-loading.md).

## Si ça ne marche pas

- **« Commande introuvable » en stdio** : votre binaire n'est pas dans le PATH d'Apollia. Donnez le chemin absolu ou ajustez votre PATH avant de lancer Apollia.
- **« Erreur réseau - le service n'a pas répondu à temps. » en HTTP ou SSE** : URL ou port incorrects, ou pare-feu qui bloque la sortie. Vérifiez l'accessibilité depuis votre machine avec `curl <url>`.
- **OAuth en boucle** : votre serveur annonce mal son endpoint de métadonnées, ou un scope a été refusé. Apollia refuse d'emblée les serveurs d'autorisation non conformes (PKCE S256 obligatoire). Vérifiez les journaux côté serveur.
- **Le test affiche « 0 outil(s) détecté(s). »** : la poignée de main réussit mais le serveur ne liste aucun outil dans sa réponse à `tools/list`. Vérifiez l'implémentation côté serveur.

## Faire apparaître votre serveur dans le catalogue

Votre serveur apparaît dans la liste une fois connecté, mais sans le logo ni la description que portent les entrées de la sélection d'Apollia. Personnaliser le catalogue n'est pas possible depuis l'interface.

> **Référence technique :** [Référence Apollia](/reference) , schéma complet du protocole, capabilities, transports, sécurité.
