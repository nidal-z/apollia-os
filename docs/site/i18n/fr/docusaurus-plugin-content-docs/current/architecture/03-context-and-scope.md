---
sidebar_position: 3
title: 3. Contexte et périmètre
format: mdx
---

# 3. Contexte et périmètre

Ceci est le contexte système C4 : Apollia OS comme une seule boîte, et tout ce
qui se trouve à l'extérieur avec quoi elle communique. Il pose la frontière
avant que les pages suivantes n'ouvrent la boîte.

## Contexte système

```mermaid
flowchart TB
    subgraph external_people [Personnes]
        operator["Opérateur / administrateur<br/>lance et supervise les agents"]
        author["Auteur de paquet<br/>déclare des agents, écrit des outils et des workflows"]
    end

    subgraph external_systems [Systèmes externes]
        host["Produit hôte<br/>pilote Apollia via son API"]
        mcp["Serveurs MCP externes<br/>exposent les données et outils de l'hôte"]
        cloud["Serveurs de modèles distants<br/>point d'accès compatible OpenAI, Ollama"]
        saas["SaaS Google / Microsoft<br/>Gmail, Calendar, Drive, Outlook"]
    end

    apollia(["Apollia OS<br/>runtime souverain pour agents"])

    localmodel[("Modèle local<br/>llama-server, sur cette machine")]
    localdb[("SQLite local<br/>mémoire, audit, configuration")]

    operator -->|CLI, application desktop| apollia
    author -->|construit et installe des paquets| apollia
    host -->|pilote via l'API HTTP + le SDK hôte| apollia
    apollia -->|client MCP, lit les outils| mcp
    apollia -->|appelle une fois configuré| cloud
    apollia -->|OAuth, à portée limitée| saas
    apollia -->|sidecar supervisé, HTTP en boucle locale| localmodel
    apollia -->|persiste| localdb

    classDef sys fill:#1f6feb,stroke:#0b3d91,color:#fff;
    class apollia sys;
```

## Acteurs et voisins

| Voisin | Relation | Direction |
|---|---|---|
| **Opérateur / administrateur** | Exécute, supervise et approuve. Utilise la [CLI](/reference/cli) et l'application desktop opérateur. | Pilote Apollia |
| **Auteur de paquet** | Déclare des agents dans un manifeste et écrit des outils et des workflows Python typés conformes au [contrat du SDK](/reference/sdk), puis construit le paquet et l'installe. | Développe pour Apollia |
| **Produit hôte** | Embarque et pilote le runtime via l'[API HTTP](/reference/api/apollia-os-runtime-api) stable et les SDK hôtes générés. Dans le modèle de fédération, l'hôte est souvent à la fois client d'Apollia et l'inverse. | Bidirectionnel |
| **Serveurs MCP externes** | Apollia est un client MCP : elle découvre et appelle leurs outils via stdio, Streamable HTTP, ou SSE. Elle peut aussi exposer un serveur MCP entrant limité. | Apollia appelle vers l'extérieur (majoritairement) |
| **Serveurs de modèles distants** | Optionnel. Un modèle distant est atteint par une connexion générique à un serveur compatible OpenAI, à l'adresse que l'utilisateur désigne (un serveur auto-hébergé ou une passerelle), ou par Ollama. Le local reste la valeur par défaut. | Apollia appelle vers l'extérieur |
| **Google / Microsoft** | Les connecteurs natifs agissent sur le courrier, le calendrier et les fichiers via OAuth, avec des permissions à portée limitée. | Apollia appelle vers l'extérieur |
| **Modèle et base de données locaux** | Les fichiers de modèle, le moteur d'inférence que le daemon supervise et le magasin SQLite vivent tous sur la même machine. Rien ici n'est distant. | À l'intérieur du périmètre |

## Ce qui est à l'intérieur du périmètre

Tout ce qui accomplit le travail de l'agent : le raisonnement et la
planification, les appels d'outils, la mémoire, la gouvernance (permissions,
audit, budgets), l'inférence locale, et les surfaces (API, CLI, desktop). La
page suivante, [Stratégie de solution](/architecture/solution-strategy),
énonce les décisions structurantes qui façonnent l'intérieur ; la
[vue de construction](/architecture/building-blocks) l'ouvre en ses parties.

## Ce qui est délibérément à l'extérieur

Le magasin de données propre à l'hôte reste du côté de l'hôte et se lit via
des outils MCP, jamais copié en bloc dans le runtime. L'inférence distante est
optionnelle, pas une dépendance. Aucun service cloud exploité par Apollia
n'intervient dans la boucle : le runtime est le produit, qui s'exécute sur la
machine de l'adoptant.
