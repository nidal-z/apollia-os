---
title: Configurer le routage hybride local + frontier
slug: /operator-help/installation/configure-hybrid-routing
sidebar_position: 7
---

# Configurer le routage hybride local + frontier

> Pour tout operator qui veut combiner un modèle local rapide pour les étapes simples et un modèle cloud puissant pour les étapes complexes, avec un plafond de coût automatique.

## Prérequis

- Au moins un backend pour le côté par défaut, un modèle `.gguf` servi par le moteur embarqué `llama-server`. Voir [Télécharger des modèles locaux](telecharger-des-modeles-locaux.md). Un serveur Ollama n'a pas sa place dans ce rôle : Apollia l'atteint en HTTP comme n'importe quel point de terminaison compatible OpenAI, c'est donc un backend distant même s'il tourne sur votre propre réseau.
- Au moins un backend cloud (frontier) déclaré. Voir [Connecter un modèle distant](connecter-un-modele-distant.md).
- Le nom exact de chaque backend tel que vous l'avez nommé lors de la configuration.

## Comportement du routage hybride

Le routeur hybride achemine chaque étape de raisonnement de l'agent selon sa complexité estimée :

1. Les étapes simples (récupération d'information, formatage, appels d'outils directs) sont traitées par le backend par défaut.
2. Les étapes complexes (raisonnement multi-sauts, synthèse longue, jugement incertain) sont escaladées vers le backend frontier.
3. Quand le cumul de coût cloud atteint `cost_ceiling_usd`, toutes les étapes restantes sont traitées par le backend par défaut, quel que soit leur niveau de complexité.

Le routeur ne garantit pas une coupure nette à l'euro près : les étapes déjà en cours au moment du dépassement se terminent sur le backend qui les a commencées.

## Étapes - Activer le routage hybride

`[llm.routing.hybrid]` est une sous-section de `[llm.routing]`, qui est
elle-même obligatoire et porte deux clés requises. Écrire la table hybride seule
crée la table parente sans elles, et le fichier ne se charge plus. Copiez le bloc
entier, pas les trois dernières lignes :

```toml
[llm.routing]
precise = "local-qwen3-8b"
fast    = "local-qwen3-4b"

[llm.routing.hybrid]
frontier          = "claude-anthropic"
cost_ceiling_usd  = 2.00
ceiling_action    = "stay_local"
```

Le même fichier doit déjà porter une section `[llm]` avec une clé `default` et au
moins une entrée `[[llm.backends]]` ; les deux sont requises également.

- `precise` et `fast` : noms des backends utilisés pour le raisonnement profond et pour l'extraction légère. Les deux sont requis, et les deux doivent correspondre à un backend déclaré.
- `frontier` : nom exact du backend cloud déclaré dans `[llm.backends]`. La valeur ne peut pas être vide.
- `cost_ceiling_usd` : plafond en dollars US par session de routing. Mettez une valeur strictement positive.
<!-- claim:hybrid-ceiling-action-decides-the-outcome -->
- `ceiling_action` : ce qui se passe au franchissement du plafond. `stay_local`, la valeur par défaut, poursuit l'exécution sur le backend local, silencieusement dégradée. `hard_stop` termine proprement l'exécution avec une erreur structurée. Choisissez `hard_stop` quand une réponse locale non signalée serait pire que pas de réponse.

Redémarrez le daemon après modification.

## Vérification

Le daemon ne valide pas cette section au démarrage, et il n'affiche rien qui
nomme votre frontier ou votre plafond. La seule ligne de routage qu'il émet au
démarrage porte les deux backends de `[llm.routing]`, et rien d'autre :

```
precise="local-qwen3-8b" fast="local-qwen3-4b" llm.routing.propagated
```

Le routage hybride se voit plus tard, au moment où une étape est escaladée. Deux
lignes racontent toute l'histoire, et les deux portent le nom du frontier :

```
frontier="claude-anthropic" session_cost_usd=0.31 ceiling_usd=2 llm.hybrid.escalation.routed
frontier="claude-anthropic" reason="the session cost ceiling is reached" llm.hybrid.escalation.blocked
```

Lancez une tâche délibérément complexe et cherchez l'une des deux. Pour surveiller l'escalade et le cumul de coût en temps réel, consultez la page d'observabilité. Voir [Surveiller les coûts LLM](../observabilite/monitor-ai-costs.md).

## Si ca ne marche pas

- **Plus rien ne se charge après la modification :** la table `[llm.routing]` est incomplète. `precise` et `fast` sont requises dès que la table existe, et écrire `[llm.routing.hybrid]` seule suffit à la créer.
- **Aucune escalade n'a jamais lieu, et aucune erreur nulle part :** un nom de `frontier` erroné n'est pas détecté au démarrage du daemon. Il se manifeste à la première escalade par `llm.hybrid.escalation.blocked` avec `reason="the frontier backend is absent from the router"`. Vérifiez le nom exact dans **Réglages - Backends LLM**, la correspondance est sensible à la casse.
- **Le plafond est atteint immédiatement :** la ligne bloquée porte `reason="the session cost ceiling is reached"`. Augmentez `cost_ceiling_usd` ou découpez vos tâches en sessions plus courtes.
- **Toutes les étapes passent en local alors que vous attendez de l'escalade :** le routeur n'escalade que sur une étape qu'il juge complexe. Testez avec une tâche explicitement complexe avant de conclure que la configuration est fausse.

> **Référence technique :** [Configuration](/reference/configuration) - la section `[llm]`, ses backends et ses clés de routage.
