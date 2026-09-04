---
title: Configurer le chargement différé des outils MCP
slug: /operator-help/integrations/configure-deferred-mcp-loading
sidebar_position: 8
---

# Configurer le chargement différé des outils MCP

> Pour tout operator qui veut contrôler quand les outils MCP sont chargés en mémoire : au démarrage (eager) ou à la demande de l'agent (deferred).

## Prérequis

- Apollia lancé avec au moins un serveur MCP connecté.
- Accès à `apollia.toml` pour modifier la configuration.

## Eager vs deferred : quand choisir quoi

| Mode | Ce qui est montré au modèle | Conseillé quand |
|---|---|---|
| `deferred` (défaut) | L'outil `tool_search`, et les outils indexés seulement si l'index entier tient dans `tool_search_limit`. | Vous avez beaucoup de serveurs MCP connectés, ou des serveurs avec de nombreux outils. L'agent cherche l'outil qu'il lui faut via `tool_search`. |
| `eager` | Tous les outils de tous les serveurs, schéma compris. | L'ensemble d'outils est petit et fixe, et vous préférez ne pas dépenser un tour en recherche. |

En mode `deferred`, le daemon ne place pas tous les schémas d'outils devant le modèle au démarrage. Il indexe ce que chaque serveur expose, et l'agent trouve ce dont il a besoin via `tool_search`.

**Ce que le mode ne change pas.** Les deux modes envoient le même unique `tools/list` à chaque serveur à la connexion, et le chemin différé garde en cache les schémas que cette réponse rapporte au lieu de les jeter. Le processus détient donc la même chose dans les deux cas : différer porte sur ce qui entre dans le prompt, pas sur ce que la machine charge. Ne choisissez pas ce mode pour économiser de la mémoire ou raccourcir le démarrage, choisissez-le pour garder le prompt léger.

Une nuance décide de la façon dont l'agent atteint un outil une fois qu'il l'a trouvé. Quand l'index entier tient dans `tool_search_limit`, Apollia déclare ces outils directement, schémas compris, et l'agent les appelle comme n'importe quel autre outil. Au-delà de cette borne, `tool_search` reste le seul point d'entrée et ses résultats portent le schéma de chaque outil, que l'agent lit avant d'appeler. Dans les deux cas l'outil est appelable ; la borne décide seulement s'il est annoncé d'emblée ou découvert.

## Étapes - Configurer le mode de chargement

Éditez `apollia.toml` :

```toml
[mcp]
tool_loading      = "deferred"
tool_search_limit = 20
```

- `tool_loading` : `"deferred"` (défaut) ou `"eager"`.
- `tool_search_limit` : nombre maximum d'outils retournés par un appel `tool_search` (défaut : `20`, bornes : `1` à `500`). Cette valeur sert aussi de borne en dessous de laquelle l'index entier est déclaré d'emblée : l'augmenter déclare plus d'outils directement et coûte plus de prompt, la diminuer renvoie davantage de découverte vers `tool_search`.

Redémarrez le daemon après modification.

## Vérification

Observez les logs du daemon au démarrage. Chaque serveur connecté émet une ligne portant un compte d'outils, et le nom d'événement dit quel mode a tourné : `mcp.tools.index.discovered` en mode `deferred`, `mcp.tools.discovered` en mode `eager`. Il n'y a de listing outil par outil dans aucun des deux modes, et aucun message ne nomme un serveur pendant son chargement.

En mode `deferred`, une ligne de plus est écrite quand la liste d'outils de l'assistant est construite : `mcp.deferred.index_advertised` si l'index entier tenait dans `tool_search_limit` et a été déclaré d'emblée, `mcp.deferred.index_reachable_through_search_only` sinon, `tool_search` étant alors le seul chemin. Les deux portent le nombre indexé et la borne, ce qui vous dit de quel côté du seuil vous êtes.

## Si ca ne marche pas

- **« L'agent ne trouve pas un outil pourtant connecté » en mode `deferred` :** il n'y a rien à déclarer dans le manifest de l'agent, et aucune clé de manifest nommée `tool_search` n'est lue par quoi que ce soit. L'outil est injecté par le runtime, et seulement dans l'assistant conversationnel intégré. Un agent Python installé n'a pas de `tool_search` du tout : en mode `deferred`, il n'atteint donc les outils MCP que lorsque l'index entier tient dans `tool_search_limit` et se trouve déclaré d'emblée. Augmentez cette borne, ou passez en `eager`.
- **Le temps de démarrage reste long malgré le mode `deferred` :** ce n'est pas le mode qu'il faut regarder. Les deux modes paient le même `tools/list` par serveur à la connexion : un démarrage lent vient donc d'un serveur lent à y répondre. Testez-les un par un depuis la page **Connexions**.
- **`tool_search` retourne trop peu de résultats :** augmentez `tool_search_limit` dans `apollia.toml`. La borne maximale est `500`.

> **Référence technique :** [Référence Apollia](/reference) - architecture du client MCP, protocole `tool_search`, gouvernance des outils, scoping.
