---
sidebar_position: 11
title: Empaqueter et distribuer un agent
---

# Empaqueter et distribuer un agent

Un agent peut être livré sous la forme d'un unique fichier Python ou d'un bundle
multi-agents décrit par un `agent.toml`, et se distribue en l'installant depuis un
chemin local ou une URL Git. Ce guide couvre la génération du squelette de départ,
le format du bundle, la validation au moment de l'installation, et la distribution
via Git.

Ce guide suppose que vous avez déjà écrit un agent ; si ce n'est pas le cas,
consultez [Écrire un worker](/how-to/write-a-worker).

## Générer un squelette de départ

`agent create` génère un agent de démarrage et son fichier de test correspondant à
partir d'un template :

```sh
apollia-os agent create my-agent --type react
```

`--type` vaut `react` (valeur par défaut), `conversational`, ou `orchestrated`.
Cette commande écrit un unique module agent plus son test ; elle ne crée pas de
bundle. Rédigez vous-même l'`agent.toml` lorsque vous avez besoin de plusieurs
agents dans un même package.

## Fichier unique ou bundle

Apollia reconnaît deux formes au moment de l'installation :

- **Fichier unique.** Un module `.py` qui se termine par un `agent = ...` au
  niveau du module. Il s'installe comme un agent unique.
- **Bundle.** Un répertoire contenant un `agent.toml` à sa racine, décrivant un
  ou plusieurs agents (avec, en option, une configuration partagée). Il s'installe
  comme un package.

Un répertoire sans `agent.toml` n'est pas un package valide et est rejeté.

## Le format de bundle `agent.toml`

```toml
[package]
name = "sales-suite"
version = "0.1.0"
description = "A worker and a director for sales prep."
author = "you"

[[agents]]
name = "crm-lookup"
entry = "crm_lookup.py"
role = "worker"
packages = ["httpx>=0.27"]

[[agents]]
name = "sales-director"
entry = "director.py"
role = "director"
packages = []

[tools.web]
enabled = true
ssrf_guard = true

[pip]
packages = ["python-dateutil"]
```

- `[package]` porte les champs `name`, `version`, `description`, et `author`.
- Chaque entrée `[[agents]]` a un `name`, un module `entry`, un `role` (`worker`,
  `director`, ou `assistant`), et ses propres `packages`.
- `[tools.web]` active ou désactive la surface d'outil web et sa protection SSRF.
- `[pip]` liste les dépendances Python valables pour tout le package. Des
  déclencheurs peuvent également être déclarés dans le bundle.

## Installation et sa validation

Installer depuis un chemin local :

```sh
# Un fichier unique
apollia-os agent install ./my_agent.py

# Un répertoire de bundle
apollia-os agent install ./sales-suite/
```

L'installation exécute ces vérifications, dans l'ordre :

1. La source existe.
2. L'agent se charge et satisfait le contrat (un `manifest()` et un `run()`
   asynchrone), validé via le pont Python.
3. Si un manifeste déclare `dangerous_tools_allowed`, l'installeur émet un
   avertissement et poursuit ; il ne bloque pas et ne demande pas de
   confirmation.
4. Les packages Python déclarés sont installés.
5. Si l'agent embarque un répertoire `tests/`, ses tests s'exécutent sous
   `pytest`. Un échec bloque l'installation. Cette étape peut être sautée avec
   `--skip-tests` (non recommandé).

## Distribuer via Git

Tout dépôt Git dont la racine contient le fichier de l'agent (ou un bundle
`agent.toml`) peut être installé directement. Pointez `agent install` vers l'URL
de clonage, en épinglant éventuellement un tag ou une branche via un suffixe
`#` :

```sh
apollia-os agent install https://github.com/you/my-agent.git
apollia-os agent install https://github.com/you/my-agent.git#v1.2.0
```

Le runtime délègue à `git` (via un appel shell) le clonage du dépôt (un clone
superficiel, sur la référence épinglée si elle est donnée), puis le valide et
l'installe exactement comme une source locale. `git` doit être présent sur la
machine ; il n'existe aucun mécanisme de repli en son absence. Il n'existe ni
index de découverte ni recherche intégrés : vous distribuez l'URL, l'installeur
prend le relais à partir de là.

## Gérer les agents et packages installés

```sh
# Agents individuels
apollia-os agent list
apollia-os agent uninstall my-agent --confirm
apollia-os agent update my-agent ./my_agent.py      # remplace par un nouveau module local

# Packages (bundles)
apollia-os agent package list
apollia-os agent package show sales-suite
apollia-os agent package uninstall sales-suite --confirm   # supprime ses agents et déclencheurs
```

`agent update` remplace un agent installé par un nouveau chemin de module local ;
elle ne re-clone pas une source Git. Pour mettre à jour depuis Git, réinstallez
depuis l'URL.

## Voir aussi

- [Écrire un worker](/how-to/write-a-worker) pour le contrat de skill qu'expose
  un worker distribué.
- Chaque sous-commande et option de `agent` figure dans la
  [référence CLI](/reference/cli).
