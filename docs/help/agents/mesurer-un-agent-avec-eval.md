# Mesurer les performances d'un agent avec apollia eval

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
  value = 0

  [[tasks.assertions]]
  type = "file_exists"
  path = "output/resume.txt"

  [[tasks.assertions]]
  type = "regex"
  pattern = "\\b(résumé|synthèse)\\b"
  target = "stdout"

  [[tasks.assertions]]
  type = "llm_judge"
  prompt = "La réponse est-elle une synthèse cohérente de trois phrases ? Réponds par OUI ou NON."
  pass_if = "OUI"
```

Détail des champs :

- `name` : identifiant lisible de la suite, apparaît dans le rapport.
- `tasks[].id` : identifiant de la tâche, unique dans la suite.
- `tasks[].prompt` : texte de la tâche envoyée à l'agent.
- `tasks[].runs` : nombre d'exécutions indépendantes par tâche (défaut : `3`). Utilisez au moins 3 pour détecter les réponses non déterministes.
- `tasks[].assertions` : liste de vérifications appliquées à chaque exécution.

Les quatre types d'assertions :

| Type | Ce qu'il vérifie |
|---|---|
| `exit_code` | Le code de sortie de l'exécution (0 = succès). |
| `file_exists` | Un fichier produit par l'agent existe au chemin indiqué. |
| `regex` | Une expression régulière correspond dans `stdout` ou dans un fichier. |
| `llm_judge` | Un second LLM évalue la sortie à partir d'un prompt et d'une valeur attendue. |

## Étapes - Lancer l'évaluation

```
apollia eval run ma-suite.toml
```

La commande affiche un tableau de résultats en temps réel pendant l'exécution. À la fin, elle écrit un fichier `.results.jsonl` dans le même répertoire que la suite.

Pour obtenir une sortie JSON machine (intégration CI, scripting) :

```
apollia eval run ma-suite.toml --json
```

## Étapes - Lire le rapport

```
apollia eval report ma-suite.results.jsonl
```

Affiche un résumé par tâche : taux de réussite, temps médian, assertions échouées. Utilisez `--json` pour obtenir le rapport en JSON.

## Vérification

- Le fichier `ma-suite.results.jsonl` est créé dans le même répertoire que `suite.toml`.
- La commande `apollia eval run` se termine avec le code de sortie `0` si toutes les assertions passent sur toutes les exécutions.
- La commande `apollia eval report` affiche le taux de réussite global.

## Si ca ne marche pas

- **"runtime non joignable" au lancement :** le daemon Apollia n'est pas démarré. Lancez `apollia start` puis relancez l'évaluation.
- **"suite invalide" :** vérifiez la syntaxe TOML de votre fichier (parenthèses, guillemets, nom de clé) et assurez-vous que le champ `type` de chaque assertion est l'une des quatre valeurs reconnues.
- **Les assertions `llm_judge` échouent systématiquement :** vérifiez que le backend LLM par défaut est configuré et joignable. Le juge LLM utilise le même backend que l'agent évalué.
- **Le fichier `.results.jsonl` n'est pas créé :** l'évaluation a échoué avant de produire des résultats. Relancez avec `--json` pour voir l'erreur brute.

> **Référence technique :** [Briques-CLI](https://github.com/Apollia-OS/apollia-os/wiki/Briques-CLI) - commandes `eval run` et `eval report`, format `.results.jsonl`, intégration CI.
