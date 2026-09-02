---
title: Mesurer les performances d'un agent avec apollia-os eval
slug: /operator-help/agents/measure-an-agent-with-eval
sidebar_position: 5
---

# Mesurer les performances d'un agent avec apollia-os eval

> Pour tout operator qui veut quantifier la fiabilité d'un agent sur un ensemble de tâches reproductibles avant de l'utiliser en production.

## Prérequis

- Apollia lancé, daemon actif.
- L'agent à évaluer installé et démarrable.
- Un fichier `suite.toml` à créer (voir ci-dessous).

## Créer une suite d'évaluation

Créez un fichier `suite.toml` dans le répertoire de votre choix. Structure minimale :

```toml
name = "ma-suite-validation"

[[tasks]]
id        = "resumer-texte"
prompt    = "Résume ce texte en trois phrases : [...]"
runs      = 3

  [[tasks.assertions]]
  type = "exit_code"
  equals = 0

  [[tasks.assertions]]
  type = "file_exists"
  path = "output/resume.txt"

  [[tasks.assertions]]
  type = "regex"
  pattern = "\\b(résumé|synthèse)\\b"
  on = "stdout"

  [[tasks.assertions]]
  type = "llm_judge"
  rubric = "La réponse doit être une synthèse cohérente du texte source, en trois phrases."
```

Détail des champs :

- `name` : identifiant lisible de la suite, apparaît dans le rapport.
- `tasks[].id` : identifiant de la tâche, unique dans la suite.
- `tasks[].prompt` : texte de la tâche envoyée à l'agent.
- `tasks[].runs` : nombre d'exécutions indépendantes par tâche (défaut : `3`). Utilisez au moins 3 pour détecter les réponses non déterministes.
- `tasks[].assertions` : liste de vérifications appliquées à chaque exécution.

Quatre types d'assertions existent : `exit_code`, `file_exists`, `regex` et
`llm_judge`. Chacun prend son propre jeu de champs et refuse les autres, si bien
qu'une clé mal orthographiée fait échouer le chargement au lieu d'ignorer le
contrôle.

Les champs exacts par type sont dans le [schéma des suites d'évaluation](/reference/eval-suites),
généré depuis le parseur lui-même. Deux méritent d'être connus avant d'écrire
votre première suite : un `regex` cible `stdout` ou `result`, jamais un fichier,
et un `llm_judge` prend une `rubric` et aucune valeur attendue.

## Étapes - Lancer l'évaluation

```
apollia-os eval run ma-suite.toml
```

La commande affiche un tableau de résultats en temps réel pendant l'exécution. À la fin, elle écrit un fichier `.results.jsonl` dans le même répertoire que la suite.

Pour obtenir une sortie JSON machine (intégration CI, scripting) :

```
apollia-os eval run ma-suite.toml --json
```

## Étapes - Lire le rapport

```
apollia-os eval report ma-suite.results.jsonl
```

Affiche un résumé par tâche : taux de réussite, temps médian, assertions échouées. Utilisez `--json` pour obtenir le rapport en JSON.

## Vérification

- Le fichier `ma-suite.results.jsonl` est créé dans le même répertoire que `suite.toml`.
- La commande `apollia-os eval run` se termine avec le code de sortie `0` si toutes les assertions passent sur toutes les exécutions.
- La commande `apollia-os eval report` affiche le taux de réussite global.

## Si ca ne marche pas

- **"runtime non joignable" au lancement :** le daemon Apollia n'est pas démarré. Lancez `apollia-os start` puis relancez l'évaluation.
- **"suite invalide" :** vérifiez la syntaxe TOML de votre fichier (parenthèses, guillemets, nom de clé) et assurez-vous que le champ `type` de chaque assertion est l'une des quatre valeurs reconnues.
- **Les assertions `llm_judge` échouent systématiquement :** vérifiez que le backend LLM par défaut est configuré et joignable. Le juge LLM utilise le même backend que l'agent évalué.
- **Le fichier `.results.jsonl` n'est pas créé :** l'évaluation a échoué avant de produire des résultats. Relancez avec `--json` pour voir l'erreur brute.

> **Référence technique :** [Référence Apollia](/reference) - commandes `eval run` et `eval report`, format `.results.jsonl`, intégration CI.
