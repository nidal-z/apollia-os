---
sidebar_position: 6
title: Faire planifier un agent d'abord
---

# Faire planifier un agent d'abord

Un agent peut travailler de deux façons. Pas à pas, il choisit chaque outil au
fil de l'eau. En planifiant d'abord, il écrit la liste des étapes de la tâche
avant d'agir, puis les exécute. Planifier convient à une tâche qui demande
plusieurs appels d'outils dont l'ordre compte ; cela coûte un appel au modèle de
plus avant que quoi que ce soit ne se passe. Ce guide règle le mode de
planification d'un agent, dans son paquet et sur votre machine.

Vous n'écrivez aucun flux de contrôle pour cela : l'agent est une donnée, et la
planification est l'un de ses réglages. Quand vous connaissez vous-même les
étapes, écrivez-les : [Écrire un workflow](/how-to/write-a-director).

## Ce que fait « planifier d'abord »

L'agent écrit un plan : une liste d'étapes, chacune un appel d'outil ou un appel
au modèle, avec les étapes dont elle dépend. Le runtime, ensuite :

1. vérifie le plan : étapes uniques, dépendances connues, aucun cycle, et pas
   plus d'étapes que le budget de l'exécution n'en permet ;
2. l'exécute niveau par niveau, les étapes indépendantes en lecture seule d'un
   même niveau ensemble ;
3. replanifie une étape qui a échoué, ainsi qu'un résultat que la vérification
   refuse, aux paliers d'autonomie qui vérifient ;
4. garde le plan dans le cache de plans, si bien que la même tâche du même agent
   s'exécute de nouveau sans planifier.

Les étapes appellent les outils comme le fait la boucle pas à pas : uniquement
les outils accordés à l'agent, par les mêmes approbations. Chaque appel au modèle
d'une étape va au modèle de l'agent.

## Les trois modes

| `planning` | L'agent planifie |
| --- | --- |
| `never` | Jamais. Il agit pas à pas. |
| `auto` | Quand la tâche se lit comme une tâche à plusieurs étapes. C'est le défaut. |
| `always` | Chaque tâche. |

En `auto`, la décision ne fait aucun appel au modèle : la demande et l'agent sont
notés, et la tâche est planifiée quand la note la lit comme plusieurs étapes.

## Le régler dans le paquet

L'auteur d'un paquet recommande un mode dans les valeurs par défaut de l'agent :

```toml
[[agents]]
id = "briefing"
name = "Briefing"
mission = """
Given a topic, build a one-page briefing: the context in two or three
sentences, five to seven key facts with their source, and three open
questions. If you cannot verify a fact, say so in the briefing.
"""
examples = ["Give me a briefing on the new accounting rules."]

[agents.defaults]
access = ["web_search", "web_read"]
planning = "always"
autonomy = "supervised"
```

`access` liste les outils que le plan peut utiliser. `web_search` et `web_read`
sont des outils natifs ; l'ensemble figure dans la
[référence des outils](/reference/native-tools). Les autres clés de la table
sont dans la [référence du manifeste de paquet](/reference/package-manifest).

## Le changer sur votre machine

Une valeur du paquet est un défaut. Surchargez-la pour un agent, et une mise à
jour du paquet conserve votre choix :

```bash
apollia-os agent set briefing planning always
apollia-os agent show briefing
apollia-os agent reset briefing --field planning
```

`agent show` affiche la définition telle qu'elle est résolue : les défauts du
paquet, puis vos surcharges. `agent reset --field planning` revient au défaut du
paquet.

## L'exécuter

```bash
apollia-os run briefing "Give me a briefing on the new accounting rules."
```

L'agent planifie, exécute le plan et répond comme il le ferait pas à pas. Un
plan illisible, une étape qui échoue encore après ses replanifications, ou un
budget épuisé font échouer l'exécution.

## Le budget borne le plan

Un plan ne peut pas contenir plus d'étapes que le budget de l'exécution, et le
runtime fait respecter ce budget quoi que fasse l'agent. Le budget vient du
palier d'autonomie de l'agent, la clé `autonomy` ci-dessus ou
`apollia-os agent set briefing autonomy supervised`. Voir
[Paliers d'autonomie](/explanation/autonomy-tiers) pour ce que chaque palier
permet.

## Approuver un plan

« Planifier d'abord » n'a pas de porte d'approbation qui lui soit propre : le
plan s'exécute une fois vérifié. Dans une conversation, relire un plan avant
qu'il ne s'exécute est le mode plan de la conversation, décrit dans
[Approuver ou refuser une action](/operator-help/control/approve-or-reject-an-action).
Quand une conversation est en mode plan, l'agent ne planifie pas d'abord
par-dessus.

## Le cache de plans

Un plan est mis en cache sous l'agent, la version de sa définition, sa mission
et la tâche. Une nouvelle mission, ou une nouvelle version du paquet, fait donc
planifier de nouveau. Pour inspecter ou vider le cache :

```bash
apollia-os plan cache stats
apollia-os plan cache evict --max-age-days 7
apollia-os plan cache clear
```

## Voir aussi

- [Le plan et le modèle d'exécution orchestrée](/explanation/the-plan-model)
  pour le modèle derrière un plan.
- [Écrire un workflow](/how-to/write-a-director) quand les étapes sont à vous
  d'écrire.
- Les commandes `agent` et `plan` dans la [référence CLI](/reference/cli).
