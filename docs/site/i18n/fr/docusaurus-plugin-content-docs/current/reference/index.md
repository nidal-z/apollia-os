---
sidebar_position: 0
title: Référence
---

# Référence

Orientée information, précise, générée quand c'est possible.

## Sections générées (source de vérité, ne pas éditer à la main)

- **[Référence CLI](/reference/cli)** générée depuis l'arbre de commandes clap de
  `apollia-os`.
- **[Référence API HTTP](/reference/api/apollia-os-runtime-api)** générée depuis la
  spec OpenAPI livrée (`clients/openapi.json`).
- **[Contrat SDK / ctx](/reference/sdk)** généré depuis `sdk/apollia/types.py` et les
  protocoles `Ctx` par service.

Régénère les trois avec `bash regen.sh`.

## Références complémentaires

- **[Configuration (apollia.toml)](/reference/configuration)** les sections du
  fichier de configuration et leurs champs.
- **[Catalogue d'outils natifs](/reference/native-tools)** les outils que le
  runtime expose nativement aux agents.
- **[Variables d'environnement](/reference/environment-variables)** ce que le
  runtime lit dans son environnement : le moteur local, le stockage des secrets,
  les clients OAuth des connecteurs, les diagnostics.
- **[Paramètres d'échantillonnage par défaut](/reference/sampling-defaults)** quel
  paramètre d'échantillonnage atteint effectivement un modèle, et ce qui est
  écrit mais non appliqué.
- **[Schéma des suites d'évaluation](/reference/eval-suites)** le TOML qu'accepte
  une suite `apollia-os eval run`, champ par champ et assertion par assertion.
