---
sidebar_position: 1
title: 1. Introduction et objectifs
---

# 1. Introduction et objectifs

## Ce qu'est Apollia

Apollia OS est un runtime souverain pour agents IA autonomes. Un agent est une
fiche : une mission, un modèle, des outils, des sources et des droits, déclarés
dans le manifeste d'un paquet ou créés dans l'application. Le moteur intégré au
runtime l'exécute dans une boucle ReAct, localement, sous la gouvernance imposée
par le runtime, et sans dépendance au cloud. Un paquet peut aussi apporter du
Python : des outils qu'un agent appelle, et des workflows dont l'auteur fixe les
étapes.

Il est conçu pour être **embarqué**. Son principal consommateur est un produit
hôte qui pilote une instance Apollia pour effectuer du travail d'agent sur ses
propres données, sans jamais faire transiter ces données par un bac à sable
cloud. Le runtime expose pour cela un contrat machine stable (une API HTTP et
des SDK hôtes générés), et peut aussi s'exécuter comme démon local derrière
une CLI et une application desktop opérateur.

## Objectifs de qualité

L'architecture est optimisée, dans cet ordre, pour trois propriétés.

| Objectif | Ce que cela signifie | Pourquoi c'est prioritaire |
|---|---|---|
| **Souveraineté** | Aucune donnée utilisateur ne quitte la machine sans action explicite. L'inférence peut s'exécuter entièrement en local. | Le runtime cible des contextes réglementés et soumis à des contraintes de confidentialité où un bac à sable cloud est rédhibitoire. |
| **Redevabilité** | Toute action gouvernée est enregistrée dans un journal signé, inviolable, qui peut être vérifié. Annuler une modification du système de fichiers n'est pas une capacité de cette version. | L'autonomie n'est déléguable que si l'on peut répondre, après coup, à ce qui s'est passé. |
| **Contrôle** | Un humain détermine jusqu'où un agent peut agir seul, approuve les actions à conséquences directement dans le flux d'exécution, et le runtime impose des budgets stricts qu'un agent ne peut contourner. | L'autonomie bornée fait la différence entre un outil et un risque. |

La performance, la portabilité et l'ergonomie pour les développeurs comptent,
mais elles sont façonnées par ces trois priorités. En cas d'arbitrage, la
souveraineté et la redevabilité l'emportent.

## Parties prenantes

| Partie prenante | Ce qu'elle attend d'Apollia | Où elle se documente |
|---|---|---|
| **Intégrateur hôte** (tête de pont) | Piloter et embarquer le runtime depuis son propre produit sans avoir à en faire de la rétro-ingénierie | [Guide pratique du contrat pilote](/how-to/integrate-via-driving-contract), [Référence de l'API HTTP](/reference/api/apollia-os-runtime-api) |
| **Auteur de paquet** (Python) | Déclarer des agents dans un manifeste, et écrire des outils et des workflows typés contre un contrat stable | [Référence SDK / `ctx`](/reference/sdk) |
| **Opérateur** | Exécuter et superviser les agents au quotidien | [Aide opérateur](/operator-help) |
| **Contributeur** | Faire évoluer le runtime sans en rompre les invariants | Cette section, ainsi que le corpus de règles pour agents versionné dans le dépôt |

## Périmètre de ce document

Cette section cartographie le runtime et ses surfaces. Elle découle du code,
pas d'une ambition affichée. Les formes précises des commandes, des points de
terminaison et des services vivent dans la
[référence](/reference/api/apollia-os-runtime-api) ; cette section explique
comment les parties s'articulent entre elles et ce qui est, ou n'est pas,
effectivement câblé aujourd'hui.
