---
sidebar_position: 0
title: Architecture
slug: /architecture
---

# Architecture

Ceci est la cartographie système publique d'Apollia OS, rédigée selon une
structure [arc42](https://arc42.org) avec un modèle [C4](https://c4model.com). C'est
la carte que lit un contributeur, un intégrateur ou un évaluateur technique pour
comprendre comment le runtime est construit et pourquoi.

## Ce que cette section est, et n'est pas

Elle décrit la forme du système : ses objectifs, ses contraintes, ses parties,
la façon dont elles s'assemblent, et où se trouve la dette. Elle ne répète pas
le matériel de référence. Chaque fois qu'un fait relève de l'API, du CLI ou du
SDK, cette section renvoie vers la référence générée plutôt que de le
reformuler, afin qu'il n'existe qu'une seule source par fait :

- [Référence de l'API HTTP](/reference/api/apollia-os-runtime-api) pour le contrat de pilotage côté hôte.
- [Référence CLI](/reference/cli) pour la surface de commandes.
- [Contrat SDK / `ctx`](/reference/sdk) pour ce qu'un auteur d'agent appelle.
- [Configuration](/reference/configuration) et le
  [catalogue d'outils natifs](/reference/native-tools).

Les concepts qui méritent leur propre récit vivent sous
[Explication](/explanation), par exemple
[le modèle de responsabilité](/explanation/accountability-model). Cette section
renvoie vers eux plutôt que de les dupliquer.

## Politique d'honnêteté

Chaque affirmation ici est ancrée dans ce que le code fait réellement, pas dans
ce que d'anciennes notes de conception espéraient qu'il ferait. Là où une
capacité est partielle ou absente, cela est indiqué clairement dans
[Risques et dette technique](/architecture/risks-and-technical-debt).
Une cartographie qui cache ses lacunes n'est pas une carte, c'est une brochure.

## Ordre de lecture

1. [Introduction et objectifs](/architecture/introduction-and-goals)
2. [Contraintes](/architecture/constraints)
3. [Contexte et périmètre](/architecture/context-and-scope)
4. [Stratégie de solution](/architecture/solution-strategy)
5. [Vue de construction](/architecture/building-blocks)
6. [Vue d'exécution](/architecture/runtime-view)
7. [Concepts transversaux](/architecture/crosscutting-concepts)
8. [Décisions d'architecture](/architecture/decisions)
9. [Risques et dette technique](/architecture/risks-and-technical-debt)
10. [Glossaire](/architecture/glossary)
