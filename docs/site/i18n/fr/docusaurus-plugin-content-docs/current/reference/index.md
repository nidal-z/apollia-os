---
sidebar_position: 0
title: Référence
description: "Les faits vérifiés par la machine : API HTTP, CLI, contrat du SDK, clés de configuration, outils natifs, événements et suites d'évaluation."
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

Les trois sont reconstruites depuis l'arbre qu'elles documentent, si bien
qu'une page ne peut pas s'en écarter. On modifie la source, jamais la page.

## Références complémentaires

- **[Configuration (apollia.toml)](/reference/configuration)** les sections du
  fichier de configuration et leurs champs.
- **[Manifeste de paquet (apollia.toml)](/reference/package-manifest)** chaque
  table et chaque clé du manifeste placé à la racine d'un paquet, et les règles
  qu'il doit tenir.
- **[Catalogue d'outils natifs](/reference/native-tools)** les outils que le
  runtime expose nativement aux agents.
- **[Variables d'environnement](/reference/environment-variables)** ce que le
  runtime lit dans son environnement : le moteur local, le stockage des secrets,
  les clients OAuth des connecteurs, les diagnostics.
- **[Paramètres d'échantillonnage par défaut](/reference/sampling-defaults)** les
  champs d'échantillonnage qu'une requête envoie à un modèle, et les niveaux de
  valeurs par défaut qui les remplissent.
- **[Schéma des suites d'évaluation](/reference/eval-suites)** le TOML qu'accepte
  une suite `apollia-os eval run`, champ par champ et assertion par assertion.
- **[Points d'accès compatibles](/reference/compatible-endpoints)** le second
  écouteur que les autres applications appellent aux formats d'OpenAI et
  d'Anthropic, son port, son interrupteur, et les chemins qu'il sert.
