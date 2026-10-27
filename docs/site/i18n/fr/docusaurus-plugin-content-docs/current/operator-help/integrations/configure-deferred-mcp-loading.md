---
title: Configurer le chargement différé des outils MCP
slug: /operator-help/integrations/configure-deferred-mcp-loading
sidebar_position: 8
---

# Configurer le chargement différé des outils MCP

> Pour tout operator qui veut contrôler quand les outils MCP sont chargés en mémoire : au démarrage (eager) ou à la demande de l'agent (deferred).

## Prérequis

- Apollia lancé avec au moins un service connecté depuis **Connexions**.
- Accès à `apollia.toml` pour modifier la configuration. Avec le mode développeur allumé, le fichier s'ouvre dans **Réglages**, section **Configuration**.

## Eager vs deferred : quand choisir quoi

| Mode | Ce qui est montré au modèle | Conseillé quand |
|---|---|---|
| `deferred` (défaut) | L'outil `tool_search`, et les outils MCP accordés à l'agent seulement s'ils tiennent tous dans `tool_search_limit`. | Vous avez beaucoup de serveurs MCP connectés, ou des serveurs avec de nombreux outils. L'agent cherche l'outil qu'il lui faut via `tool_search`. |
| `eager` | Tous les outils MCP accordés à l'agent, schéma compris. | L'ensemble d'outils est petit et fixe, et vous préférez ne pas dépenser un tour en recherche. |

En mode `deferred`, le daemon ne place pas tous les schémas d'outils devant le modèle au démarrage. Il indexe ce que chaque serveur expose, et l'agent trouve ce dont il a besoin via `tool_search`.

**Ce que le mode ne change pas.** Les deux modes envoient le même unique `tools/list` à chaque serveur à la connexion, et le chemin différé garde en cache les schémas que cette réponse rapporte au lieu de les jeter. Le processus détient donc la même chose dans les deux cas : différer porte sur ce qui entre dans le prompt, pas sur ce que la machine charge. Ne choisissez pas ce mode pour économiser de la mémoire ou raccourcir le démarrage, choisissez-le pour garder le prompt léger.

Une nuance décide de la façon dont l'agent atteint un outil une fois qu'il l'a trouvé. L'index dans lequel un agent cherche contient les outils MCP qui lui sont accordés, et rien d'autre. Quand cet index tient dans `tool_search_limit`, Apollia déclare ces outils directement, schémas compris, et l'agent les appelle comme n'importe quel autre outil. Au-delà de cette borne, `tool_search` reste le seul point d'entrée et ses résultats portent le schéma de chaque outil, que l'agent lit avant d'appeler. Dans les deux cas l'outil est appelable ; la borne décide seulement s'il est annoncé d'emblée ou découvert.

## Étapes - Configurer le mode de chargement

Éditez `apollia.toml`, dans un éditeur de texte ou dans **Réglages**, section **Configuration**, avec le mode développeur allumé :

```toml
[mcp]
tool_loading      = "deferred"
tool_search_limit = 20
```

- `tool_loading` : `"deferred"` (défaut) ou `"eager"`.
- `tool_search_limit` : nombre maximum d'outils retournés par un appel `tool_search` (défaut : `20`, bornes : `1` à `500`). Cette valeur sert aussi de borne en dessous de laquelle l'index entier est déclaré d'emblée : l'augmenter déclare plus d'outils directement et coûte plus de prompt, la diminuer renvoie davantage de découverte vers `tool_search`.

Redémarrez Apollia après avoir changé `tool_loading` : le mode est lu une fois, au démarrage d'Apollia, et un changement enregistré pendant qu'il tourne attend le démarrage suivant. Un changement de `tool_search_limit` s'applique sans redémarrage.

## Quand un serveur tourne

Apollia ne démarre aucun serveur à son lancement, et son lancement n'en attend aucun. Chaque serveur démarre au premier appel de l'un de ses outils, et l'appel attend qu'il réponde. Après un temps sans appel, il s'arrête, et redémarre à l'appel suivant.

```toml
[mcp]
idle_stop_secs = 900
```

- `idle_stop_secs` : secondes sans appel après lesquelles un serveur est arrêté (défaut : `900`, soit 15 minutes, bornes : `60` à `86400`). Redémarrez Apollia après l'avoir changé.

Pendant qu'un serveur dort, Apollia garde ce qu'il a publié en dernier, si bien que rien ne disparaît des écrans :

- ses outils restent proposés aux agents qui les tiennent, dans les deux modes, et listés sur la fiche de la connexion avec leur famille ;
- ses ressources restent proposées par le sélecteur **@** du Chat. En lire une démarre le serveur, comme un appel de l'un de ses outils. Un serveur dont les ressources n'ont jamais été listées est démarré une fois en arrière-plan quand le sélecteur les demande, sans retenir les autres, et sa liste rejoint le sélecteur quand elle arrive.

Un serveur dont Apollia ne connaît pas encore les outils est démarré au lancement, en arrière-plan, pour les apprendre ; le lancement ne l'attend pas non plus. Un serveur en pause n'est jamais démarré, ni par un appel, ni par le sélecteur, ni au lancement : un appel est refusé avec la raison que sa connexion est en pause, et ses ressources ne sont pas proposées.

## Vérification

Observez les logs du daemon. Chaque serveur qui démarre émet une ligne portant un compte d'outils, et le nom d'événement dit quel mode a tourné : `mcp.tools.index.discovered` en mode `deferred`, `mcp.tools.discovered` en mode `eager`. Il n'y a de listing outil par outil dans aucun des deux modes, et aucun message ne nomme un serveur pendant son chargement.

En mode `deferred`, une ligne de plus est écrite quand la liste d'outils de l'assistant est construite : `mcp.deferred.index_advertised` si l'index entier tenait dans `tool_search_limit` et a été déclaré d'emblée, `mcp.deferred.index_reachable_through_search_only` sinon, `tool_search` étant alors le seul chemin. Les deux portent le nombre indexé et la borne, ce qui vous dit de quel côté du seuil vous êtes.

## Si ca ne marche pas

- **« L'agent ne trouve pas un outil pourtant connecté » en mode `deferred` :** `tool_search` cherche parmi les outils accordés à l'agent, pas parmi tout ce qui est connecté. Ouvrez la fiche de l'agent, cliquez sur **Ajuster**, et accordez la connexion sous **Accès**. Il n'y a rien à déclarer pour `tool_search` lui-même : le runtime le propose de lui-même dans ce mode.
- **Le premier appel d'un outil prend plus de temps que les suivants :** son serveur dormait et a démarré pour cet appel. Ce n'est pas le mode qu'il faut regarder : les deux modes paient le même `tools/list` quand un serveur démarre. Augmentez `idle_stop_secs` pour garder les serveurs en marche plus longtemps, ou cliquez sur **Vérifier** sur la fiche d'un serveur lent pour voir comment il répond.
- **Un changement de `tool_loading` ne fait rien :** Apollia le lit au démarrage. Redémarrez Apollia.
- **`tool_search` retourne trop peu de résultats :** augmentez `tool_search_limit` dans `apollia.toml`. La borne maximale est `500`.

> **Référence technique :** [Référence Apollia](/reference) - architecture du client MCP, protocole `tool_search`, gouvernance des outils, scoping.
