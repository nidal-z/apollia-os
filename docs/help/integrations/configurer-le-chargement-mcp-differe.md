# Configurer le chargement différé des outils MCP

> Pour tout operator qui veut contrôler quand les outils MCP sont chargés en mémoire : au démarrage (eager) ou à la demande de l'agent (deferred).

## Prérequis

- Apollia lancé avec au moins un serveur MCP connecté.
- Accès à `apollia.toml` pour modifier la configuration.

## Eager vs deferred : quand choisir quoi

| Mode | Chargement | Mémoire au démarrage | Conseillé quand |
|---|---|---|---|
| `deferred` (défaut) | À la demande, via `tool_search` | Faible : seuls les métadonnées sont indexées. | Vous avez beaucoup de serveurs MCP connectés, ou des serveurs avec de nombreux outils. L'agent cherche l'outil qu'il lui faut via `tool_search`. |
| `eager` | Au démarrage du daemon | Plus élevée : tous les outils sont chargés immédiatement. | L'ensemble d'outils est petit et fixe. Vos agents ne savent pas utiliser `tool_search` (par exemple : agents anciens ou agents très spécialisés). |

En mode `deferred`, le daemon ne charge pas les schémas d'outils complets au démarrage. L'agent émet des appels `tool_search` pour trouver et charger les outils à la volée. Cela réduit le temps de démarrage et la consommation mémoire sur les installations avec de nombreux serveurs MCP.

## Étapes - Configurer le mode de chargement

Éditez `apollia.toml` :

```toml
[mcp]
tool_loading      = "deferred"
tool_search_limit = 20
```

- `tool_loading` : `"deferred"` (défaut) ou `"eager"`.
- `tool_search_limit` : nombre maximum d'outils retournés par un appel `tool_search` (défaut : `20`, bornes : `1` à `500`). Augmentez cette valeur si vos agents ont besoin de parcourir un catalogue étendu en une seule recherche.

Redémarrez le daemon après modification.

## Vérification

En mode `deferred`, observez les logs au démarrage du daemon : aucun message de type "loading tools from <serveur>" n'apparaît. Les messages de chargement apparaissent uniquement quand un agent émet un appel `tool_search`.

En mode `eager`, les logs au démarrage listent tous les outils chargés par serveur.

## Si ca ne marche pas

- **"L'agent ne trouve pas un outil pourtant connecté" en mode `deferred` :** vérifiez que l'agent déclare `tool_search` dans son manifest (clé `skills` ou `tools`). Sans cette déclaration, l'agent ne peut pas chercher des outils à la demande. Passez temporairement en `eager` pour déboguer et confirmer que l'outil est bien exposé par le serveur MCP.
- **Le temps de démarrage reste long malgré le mode `deferred` :** un autre serveur MCP déclare ses outils automatiquement au démarrage, indépendamment de ce paramètre. Vérifiez la configuration de chaque serveur dans la page **Connexions**.
- **`tool_search` retourne trop peu de résultats :** augmentez `tool_search_limit` dans `apollia.toml`. La borne maximale est `500`.

> **Référence technique :** [Briques-MCP](https://github.com/Apollia-OS/apollia-os/wiki/Briques-MCP) - architecture du client MCP, protocole `tool_search`, gouvernance des outils, scoping.
