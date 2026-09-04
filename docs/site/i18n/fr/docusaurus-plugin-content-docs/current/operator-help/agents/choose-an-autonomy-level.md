---
title: Choisir un palier d'autonomie pour un agent
slug: /operator-help/agents/choose-an-autonomy-level
sidebar_position: 3
---

# Choisir un palier d'autonomie pour un agent

> Pour tout operator qui veut ajuster jusqu'où un agent peut agir seul avant de demander une confirmation.

## Prérequis

- Apollia lancé et le daemon actif.
- Au moins un agent installé et démarré.
- Familiarité avec la commande `apollia-os run`.

## Les quatre paliers

<!-- claim:tier-sets-budget-runtime-ceiling-caps-it -->
| Palier | Quand l'utiliser | Budget d'étapes | Vérification auto | Injection mémoire |
|---|---|---|---|---|
| `assisted` | Travail exploratoire, tâche inconnue, premier lancement. | Très court - convient pour valider un prototype. | Aucune. Le gate de plan est armé : vous approuvez le plan avant son exécution. | Non. |
| `supervised` | Usage quotidien standard. | Modéré - couvre la majorité des tâches courantes. | Une passe de vérification après la fin de l'exécution. | Non. |
| `bounded_autonomous` | Automatisations configurées, pipelines récurrents dont vous connaissez le comportement. | Généreux - adapté aux workflows longs mais bornés. | Une passe de vérification après la fin de l'exécution. | Non. |
| `long_autonomous` | Tâches de fond longue durée, travail de nuit sur données volumineuses. Réservé aux agents éprouvés. | Maximum disponible. | Une passe de vérification après la fin de l'exécution. | Non, pour un agent. |

Deux choses que la table laisserait supposer, et qu'il ne faut pas.

<!-- claim:plan-gate-bypassed-above-supervised -->
**L'approbation n'est pas par étape.** Aucun palier ne met en pause avant chaque
action. Ce que `assisted` et `supervised` arment, c'est le **gate de plan** :
vous voyez le plan et l'approuvez une fois, avant exécution. `bounded_autonomous`
et `long_autonomous` contournent entièrement ce gate, donc le plan s'exécute sans
que vous l'ayez vu, sauf si vous passez `--plan` pour le réarmer sur cette
exécution. Indépendamment, et à tous les paliers, une écriture fichier que le
runtime juge risquée déclenche sa propre demande d'approbation.

<!-- claim:verification-is-one-post-run-pass -->
**La vérification tourne une fois, à la fin.** C'est une passe unique après
l'exécution, pas un contrôle entre les étapes, et `assisted` ne la lance pas.

> La colonne "Injection mémoire" vaut `Non` à tous les paliers **pour une
> exécution d'agent**, et c'est la garantie qui compte : rien n'injecte de
> mémoire dans le prompt d'un agent. L'assistant conversationnel intégré est un
> autre chemin, et au palier `long_autonomous` il reçoit bien un brief de persona
> utilisateur. Un agent ne peut pas atteindre ce code.

## Étapes - Appliquer un palier pour une exécution

Le palier se précise au lancement, avec `--autonomy`. Il s'applique uniquement à cette exécution et ne modifie pas `apollia.toml`.

```
apollia-os run mon-agent "ma tâche" --autonomy supervised
```

Remplacez `supervised` par l'une des quatre valeurs : `assisted`, `supervised`, `bounded_autonomous`, `long_autonomous`.

## Étapes - Modifier le palier de chaque exécution

Il n'y a pas de palier par défaut global à régler. `[autonomy]` ne fait pas
partie des neuf sections que le daemon lit, donc un `default_level` écrit dans
`apollia.toml` ne change rien, et la commande qui l'écrirait est refusée :

```
$ apollia-os config set autonomy.default_level supervised
Error: unknown config key: 'autonomy.default_level'
```

Une exécution lancée sans `--autonomy` tourne au palier `assisted`. Pour
travailler à un autre palier, passez le drapeau à chaque exécution.

## Vérification

Après le lancement, les premières lignes de log de la tâche indiquent le palier actif :

```
autonomy.level=supervised agent=mon-agent "autonomy.activated"
```

Ouvrez les logs depuis le panneau de détail de l'agent ou via `apollia-os agent logs mon-agent --last 20`.

## Si ca ne marche pas

- **Valeur inconnue au lancement :** si vous passez une valeur incorrecte à `--autonomy`, la CLI rejette la commande et liste les quatre valeurs valides (`assisted`, `supervised`, `bounded_autonomous`, `long_autonomous`). Vérifiez l'orthographe, les valeurs sont en `snake_case`.
- **Le palier `--autonomy` est ignoré :** vérifiez que vous utilisez bien `apollia-os run`, pas `apollia-os start`. La commande `start` démarre le daemon sans exécuter de tâche ; `--autonomy` n'y a pas de sens.
- **Un bloc `[autonomy]` n'a aucun effet :** le daemon ne lit pas cette section d'`apollia.toml`, qu'il soit redémarré ou non. Réglez le palier exécution par exécution avec `--autonomy`.

> **Référence technique :** [Référence Apollia](/reference) - StepBudget, ResilienceLayer, comportement de chaque palier d'autonomie.
