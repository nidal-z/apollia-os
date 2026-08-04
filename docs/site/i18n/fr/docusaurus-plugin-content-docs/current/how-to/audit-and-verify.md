---
sidebar_position: 3
title: Auditer et vérifier une exécution
---

# Auditer et vérifier une exécution

Ce guide couvre le processus de responsabilisation autour d'une exécution
d'agent : lire le journal d'audit signé et vérifier son intégrité. Il suppose
que vous avez déjà exécuté au moins une tâche ou une session de chat face à un
daemon.

Pour comprendre la logique de ce modèle et son lien avec les exigences
réglementaires, voir [Le modèle de responsabilisation](/explanation/accountability-model).

## Lire le journal d'audit

Chaque action gouvernée qu'un agent effectue est enregistrée dans un journal
en ajout seul, chaîné par hachage, et signé au fur et à mesure de sa
croissance. Lister les événements récents :

```sh
apollia-os audit list --limit 20
apollia-os audit stats
```

Pour lire le journal complet d'une exécution donnée, y compris les
complétions du modèle capturées, résolvez-la par un identifiant d'exécution ou
par un identifiant de tâche qui s'y rattache :

```sh
apollia-os audit show <run-or-task-id>
```

Pour extraire le journal à des fins d'archivage ou de revue externe :

```sh
apollia-os audit export --output audit.json --limit 100000
```

<!-- claim:audit-export-pages-past-the-server-ceiling -->
**Le point de terminaison ne sert au maximum que 500 événements par
requête**, et la commande parcourt les pages jusqu'à ce qu'une page plus
courte revienne, de sorte que `--limit` borne l'export plutôt que
l'historique atteignable. Elle avertit sur stderr lorsque l'export s'est
arrêté sur votre `--limit` plutôt qu'à la fin du journal, ce qui est le
signal pour l'augmenter.

Les mêmes enregistrements sont disponibles via l'API HTTP pour une
intégration hôte ; voir les opérations d'audit dans la
[référence de l'API HTTP](/reference/api/apollia-os-runtime-api).

## Vérifier l'intégrité d'une exécution

Le journal est une chaîne de hachage avec signatures, ce qui rend toute
altération détectable. Vérifiez une exécution pour vous assurer que sa chaîne
et ses signatures sont intactes :

```sh
apollia-os audit verify <run-id>
```

Une vérification réussie indique que la séquence enregistrée n'a pas été
modifiée depuis son écriture. C'est le contrôle à effectuer lorsque vous
devez pouvoir faire confiance à l'authenticité du journal de ce qu'un agent a
fait.

Ajoutez `--json` à `audit list`, `audit show`, `audit stats` et `audit
verify` pour obtenir une sortie exploitable par une machine. `audit export`
écrit toujours du JSON et n'accepte pas `--json`.

## En pratique

Un passage type de responsabilisation consiste à utiliser `audit show` pour
lire ce qu'une exécution a fait, puis `audit verify` pour confirmer que
l'enregistrement est authentique. Les deux commandes sont en lecture seule :
aucune des deux ne modifie quoi que ce soit sur le disque.

## Voir aussi

- [Le modèle de responsabilisation](/explanation/accountability-model) pour
  comprendre comment ces primitives s'articulent et ce qu'elles permettent.
- La [référence CLI](/reference/cli) pour tous les indicateurs de `audit`.
- La [référence de l'API HTTP](/reference/api/apollia-os-runtime-api) pour
  les points de terminaison d'audit utilisés par une intégration hôte.
