---
sidebar_position: 11
title: Empaqueter et distribuer
---

# Empaqueter et distribuer

Un paquet est ce que vous remettez à quelqu'un d'autre : un seul fichier `.apo`
qui contient des agents, des outils et des workflows. Ce guide suit un paquet
d'un dossier vide à une archive installée et signée, avec les quatre commandes
`package` qui le portent : `new`, `build`, `inspect` et `install`.

Il suppose qu'Apollia est installé. L'écriture du contenu du paquet est
couverte par [Écrire un outil](/how-to/write-a-worker) et
[Écrire un workflow](/how-to/write-a-director).

## Ce qu'est un paquet

Un paquet est un dossier qui porte un manifeste `apollia.toml` à sa racine. Le
manifeste déclare les agents, qui sont des données, et décrit les outils et les
workflows Python que le dossier livre. Construit, le dossier devient un fichier
`.apo` : une archive zip avec le manifeste à sa racine, les modules Python et
les ressources à leurs chemins relatifs et, quand vous la signez, une signature
qui couvre toutes les autres entrées.

Chaque table et chaque clé du manifeste figurent dans la
[référence du manifeste de paquet](/reference/package-manifest).

## Créer le dossier

```bash
apollia-os package new invoice-sorter
```

La commande écrit `invoice-sorter/` sous le dossier courant, ou sous
`--output-dir`, avec un `apollia.toml` qui déclare un agent et un outil, et le
module Python de cet outil. Le dossier se construit tel quel. Rien n'est
écrasé : un dossier existant est refusé. Un nom de paquet prend des lettres
minuscules, des chiffres, `-` et `_`.

## Décrire le code

Après avoir écrit ou modifié un outil ou un workflow, écrivez sa description
dans le manifeste :

```bash
python -m apollia describe invoice-sorter
```

La commande réécrit les tables `[[tools]]` et `[[workflows]]` depuis les
décorateurs et conserve le reste du manifeste tel qu'écrit. C'est la seule étape
qui importe votre code, et elle s'exécute sur votre machine : construire,
inspecter et prévisualiser un paquet n'exécutent jamais de Python.

## Construire l'archive

```bash
apollia-os package build invoice-sorter
```

L'archive est écrite sous le nom `<name>-<version>.apo` dans le dossier courant,
ou là où `--output` le dit. La construction valide le manifeste, et refuse un
dossier dont les modules Python définissent des outils ou des workflows que le
manifeste ne décrit pas. Un refus nomme ce qui ne va pas : une clé inconnue avec
son chemin complet, un identifiant hors de `[a-z0-9_-]+`, une version qui n'est
pas semver.

## La signer

```bash
apollia-os package build invoice-sorter --sign
```

`--sign` signe l'archive avec votre clé d'auteur. La clé est lue dans le coffre
de secrets sous le libellé que donne `--key`, `default` par défaut ; elle y est
générée et rangée la première fois. L'archive porte alors la signature et la
clé publique qui l'a produite.

Un paquet non signé s'installe, avec un avis. Une signature qui ne correspond
pas au contenu est refusée.

## Inspecter avant de partager

```bash
apollia-os package inspect invoice-sorter-0.1.0.apo
```

`inspect` montre le manifeste, les entrées de l'archive et l'état de la
signature : `unsigned`, ou `signed` avec l'éditeur déclaré et l'empreinte de la
clé. Il n'a pas besoin du runtime, et sert donc aussi sur une machine où vous
voulez seulement lire ce qu'un fichier contient.

## Installer

```bash
apollia-os package preview invoice-sorter-0.1.0.apo
apollia-os package install invoice-sorter-0.1.0.apo --confirm-deps
```

Les deux ont besoin du runtime. `preview` n'écrit rien : il rapporte les agents,
les workflows et les outils, les accès que demande chaque agent, l'éditeur et la
signature, les connexions que le paquet exige et leur présence, et les serveurs
MCP qu'il apporte. `install` installe le paquet ; `--confirm-deps` accepte les
dépendances Python que liste sa table `[pip]`, qui vont dans un environnement
du paquet.

Dans l'application de bureau, ouvrir un fichier `.apo` montre le même aperçu, et
le paquet ne s'installe que depuis celui-ci.

## Mettre à jour

Installer une version plus récente d'un paquet installé le met à jour. Les
surcharges qu'un utilisateur a posées sur ses agents sont conservées.

```bash
apollia-os package install invoice-sorter-0.2.0.apo --confirm-changes
```

`--confirm-changes` accepte ce que la mise à jour ajoute : accès, connexions
exigées, serveurs MCP. Lisez d'abord `package preview`, qui les liste. Une
version plus ancienne que celle installée est refusée, sauf avec
`--allow-downgrade`.

## Faire confiance à un éditeur

Un destinataire qui fait confiance à votre clé voit vos paquets sous le nom
qu'il vous a choisi :

```bash
apollia-os package trust import --label "Example Accounting" <key>
apollia-os package trust list
```

`<key>` est la clé publique, en 64 caractères hexadécimaux ou dans un fichier
qui les contient. `package trust remove <fingerprint>` cesse de lui faire
confiance.

## Gérer les paquets installés

```bash
apollia-os agent package list
apollia-os agent package show invoice-sorter
apollia-os agent package uninstall invoice-sorter --confirm
```

La désinstallation retire les agents du paquet avec leurs surcharges, ses
workflows et ses outils, son environnement, ses fichiers et les serveurs MCP
qu'il a apportés. Les connexions qu'il exigeait restent, et la mémoire et
l'historique de ses agents sont conservés sauf avec `--clear-memory` ou
`--clear-history`.

## Partager un agent créé dans l'interface

Un agent créé dans l'application, sans code, s'exporte lui aussi en paquet :

```bash
apollia-os agent export my-agent --sign
```

Ses valeurs courantes deviennent les défauts du paquet. Aucun secret, aucune
mémoire et aucun historique ne sont exportés.

## Distribuer

Il n'y a ni index ni registre : un `.apo` est un fichier, et vous le distribuez
comme n'importe quel fichier. Le destinataire le prévisualise, puis l'installe.

## Voir aussi

- La [référence du manifeste de paquet](/reference/package-manifest).
- Chaque sous-commande et chaque option de `package` figurent dans la
  [référence CLI](/reference/cli).
