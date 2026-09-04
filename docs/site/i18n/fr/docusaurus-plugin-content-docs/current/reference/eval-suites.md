---
sidebar_position: 7
title: Schéma de suite d'évaluation
---

# Schéma de suite d'évaluation

Une suite d'évaluation est un fichier TOML que vous écrivez à la main. Elle
nomme un ensemble de tâches ; chaque tâche porte un prompt, un nombre
d'exécutions, un agent cible optionnel, et une liste d'assertions typées qui
décident du succès ou de l'échec. `apollia-os eval run` la lit, l'exécute, et
se termine avec un code de sortie non nul si une assertion échoue sur une
exécution.

Pour le guide opérateur, avec les étapes de l'interface et comment lire un
rapport, voir [Mesurer un agent avec eval](/operator-help/agents/measure-an-agent-with-eval).

Les tableaux ci-dessous sont générés à partir des types Rust qui analysent le
fichier ; ils ne peuvent donc pas diverger de ce que l'analyseur accepte. Un
champ absent de ces tableaux est un champ que l'analyseur rejette.

<!-- BEGIN GENERATED: eval-schema -->

### La suite

| Clé | Type | Obligatoire | Signification |
| --- | --- | --- | --- |
| `name` | `String` | **obligatoire** | Nom de la suite lisible par un humain, affiché dans les rapports. |
| `tasks` | `Vec<EvalTask>` | optionnel | Tâches à évaluer. Vide par défaut quand le tableau `tasks` est absent. |

### Une tâche

| Clé | Type | Obligatoire | Signification |
| --- | --- | --- | --- |
| `id` | `String` | **obligatoire** | Identifiant stable de la tâche, utilisé comme clé de ligne dans le rapport. |
| `prompt` | `String` | **obligatoire** | L'instruction soumise à l'agent. |
| `runs` | `u32` | optionnel | Nombre de fois où la tâche est exécutée. Vaut 3 par défaut. |
| `agent` | `Option<String>` | optionnel | Identifiant de l'agent cible. `None` laisse le choix à l'appelant. |
| `assertions` | `Vec<Assertion>` | optionnel | Assertions typées de succès/échec évaluées à chaque exécution. |

### Assertions

Chaque entrée sous `[[tasks.assertions]]` porte une clé `type` qui sélectionne
la forme. Les champs listés sont ceux que cette forme accepte, et aucun autre.

| `type` | Ses champs | Ce qu'elle vérifie |
| --- | --- | --- |
| `exit_code` | `equals` | Réussit quand le code de sortie de l'exécution est égal à `equals`. |
| `file_exists` | `path` | Réussit quand un fichier existe à `path` après l'exécution. |
| `regex` | `on`, `pattern` | Réussit quand `pattern` correspond au canal de sortie sélectionné. |
| `llm_judge` | `rubric` | Réussit quand un juge LLM évalue la sortie par rapport à `rubric` comme un succès. |

### `on`, le canal contre lequel une assertion `regex` effectue sa comparaison

| Valeur | Signification |
| --- | --- |
| `stdout` | La sortie standard (stdout) diffusée en flux de l'exécution. |
| `result` | Le texte du résultat final de l'exécution. |
<!-- END GENERATED: eval-schema -->

## Un exemple complet

```toml
name = "my-validation-suite"

[[tasks]]
id        = "summarize-text"
prompt    = "Summarize this text in three sentences: [...]"
runs      = 3

  [[tasks.assertions]]
  type = "exit_code"
  equals = 0

  [[tasks.assertions]]
  type = "file_exists"
  path = "output/summary.txt"

  [[tasks.assertions]]
  type = "regex"
  pattern = "\\b(summary|synthesis)\\b"
  on = "stdout"

  [[tasks.assertions]]
  type = "llm_judge"
  rubric = "The answer must be a coherent three-sentence summary of the source text."
```

## Ce que l'analyseur refuse

Se tromper dans le nom d'un champ fait échouer le chargement plutôt que
d'ignorer silencieusement la vérification, ce qui est le comportement attendu
d'un harnais de test. Si `apollia-os eval run` signale une suite invalide,
comparez d'abord vos clés d'assertion aux tableaux ci-dessus avant de
chercher ailleurs.

`llm_judge` prend un `rubric` et rien d'autre. Il n'y a aucune valeur attendue
à fournir : le `rubric` est le critère entier, et le juge répond en fonction
de lui.

`apollia-os eval run` construit son exécuteur sans juge, si bien que toute
assertion `llm_judge` échoue aujourd'hui, avec la raison « llm judge not
evaluated: this runner has no judge router ». C'est délibéré du côté du harnais,
une assertion que rien n'a vérifiée n'est jamais comptée comme réussie, et ce
n'est pas un problème de backend : en configurer un n'y change rien. Tant que la
commande ne câble pas de juge, écrivez vos suites sur `exit_code`, `file_exists`
et `regex`.
