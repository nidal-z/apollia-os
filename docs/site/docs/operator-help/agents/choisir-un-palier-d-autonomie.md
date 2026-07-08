# Choisir un palier d'autonomie pour un agent

> Pour tout operator qui veut ajuster jusqu'où un agent peut agir seul avant de demander une confirmation.

## Prérequis

- Apollia lancé et le daemon actif.
- Au moins un agent installé et démarré.
- Familiarité avec la commande `apollia run`.

## Les quatre paliers

| Palier | Quand l'utiliser | Budget d'étapes | Vérification auto | Injection mémoire |
|---|---|---|---|---|
| `assisted` | Travail exploratoire, tâche inconnue, premier lancement. L'agent propose chaque action avant de l'exécuter. | Très court - convient pour valider un prototype. | Approbation HITL à chaque étape. | Non. |
| `supervised` | Usage quotidien standard. L'agent avance seul sur les étapes simples et pause sur les actions à risque. | Modéré - couvre la majorité des tâches courantes. | Pause automatique avant toute écriture ou appel externe. | Non. |
| `bounded_autonomous` | Automatisations configurées, pipelines récurrents dont vous connaissez le comportement. L'agent court jusqu'au bout sauf dépassement de budget. | Généreux - adapté aux workflows longs mais bornés. | Vérification uniquement au dépassement de budget. | Non. |
| `long_autonomous` | Tâches de fond longues durée, travail de nuit sur données volumineuses. Réservé aux agents éprouvés. | Maximum disponible. | Aucune interruption automatique en cours de tâche. | Non. |

> La colonne "Injection mémoire" est `Non` pour tous les paliers : Apollia n'injecte jamais de contexte mémoire automatiquement, quel que soit le palier choisi.

## Étapes - Appliquer un palier pour une exécution

Le palier se précise au lancement, avec `--autonomy`. Il s'applique uniquement à cette exécution et ne modifie pas `apollia.toml`.

```
apollia run mon-agent "ma tâche" --autonomy supervised
```

Remplacez `supervised` par l'une des quatre valeurs : `assisted`, `supervised`, `bounded_autonomous`, `long_autonomous`.

## Étapes - Modifier le palier par défaut global

Pour que toutes les exécutions utilisent un palier donné sans le préciser à chaque fois, éditez `apollia.toml` :

```toml
[autonomy]
default_level = "supervised"
```

Redémarrez le daemon après modification pour que le nouveau défaut prenne effet.

## Vérification

Après le lancement, les premières lignes de log de la tâche indiquent le palier actif :

```
autonomy.level=supervised agent=mon-agent "autonomy.activated"
```

Ouvrez les logs depuis le panneau de détail de l'agent ou via `apollia logs mon-agent --tail`.

## Si ca ne marche pas

- **Valeur inconnue au lancement :** si vous passez une valeur incorrecte à `--autonomy`, la CLI rejette la commande et liste les quatre valeurs valides (`assisted`, `supervised`, `bounded_autonomous`, `long_autonomous`). Vérifiez l'orthographe, les valeurs sont en `snake_case`.
- **Le palier `--autonomy` est ignoré :** vérifiez que vous utilisez bien `apollia run`, pas `apollia start`. La commande `start` démarre le daemon sans exécuter de tâche ; `--autonomy` n'y a pas de sens.
- **Le défaut global ne change pas :** redémarrez le daemon après avoir modifié `apollia.toml`. Un daemon déjà en cours de fonctionnement lit la config au démarrage uniquement.

> **Référence technique :** [Référence Apollia](../../reference/index.md) - StepBudget, ResilienceLayer, comportement de chaque palier d'autonomie.
