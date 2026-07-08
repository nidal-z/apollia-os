# Configurer le routage hybride local + frontier

> Pour tout operator qui veut combiner un modèle local rapide pour les étapes simples et un modèle cloud puissant pour les étapes complexes, avec un plafond de coût automatique.

## Prérequis

- Au moins un backend local déclaré dans `apollia.toml` (fichier `.gguf` ou serveur Ollama). Voir [Télécharger des modèles locaux](telecharger-des-modeles-locaux.md).
- Au moins un backend cloud (frontier) déclaré. Voir [Connecter un modèle distant](connecter-un-modele-distant.md).
- Le nom exact de chaque backend tel que vous l'avez nommé lors de la configuration.

## Comportement du routage hybride

Le routeur hybride achemine chaque étape de raisonnement de l'agent selon sa complexité estimée :

1. Les étapes simples (récupération d'information, formatage, appels d'outils directs) sont traitées par le modèle local.
2. Les étapes complexes (raisonnement multi-sauts, synthèse longue, jugement incertain) sont escaladées vers le backend frontier.
3. Quand le cumul de coût cloud atteint `cost_ceiling_usd`, toutes les étapes restantes sont traitées en local, quel que soit leur niveau de complexité.

Le routeur ne garantit pas une coupure nette à l'euro près : les étapes déjà en cours au moment du dépassement se terminent sur le backend qui les a commencées.

## Étapes - Activer le routage hybride

Éditez `apollia.toml` et ajoutez la section suivante :

```toml
[llm.routing.hybrid]
frontier          = "claude-anthropic"
cost_ceiling_usd  = 2.00
```

- `frontier` : nom exact du backend cloud déclaré dans `[llm.backends]`. La valeur ne peut pas être vide.
- `cost_ceiling_usd` : plafond en dollars US par session de routing (doit être strictement positif). Toute valeur nulle ou négative est rejetée au démarrage.

Redémarrez le daemon après modification.

## Vérification

Les valeurs invalides sont détectées au démarrage du daemon. Si la config est correcte, les logs indiquent :

```
llm.routing=hybrid frontier=claude-anthropic ceiling_usd=2.00 "routing.activated"
```

Pour surveiller l'escalade et le cumul de coût en temps réel, consultez la page d'observabilité. Voir [Surveiller les coûts LLM](../observabilite/surveiller-les-couts-llm.md).

## Si ca ne marche pas

- **"backend frontier inconnu" au démarrage :** la valeur de `frontier` ne correspond à aucun backend déclaré dans `[llm.backends]`. Vérifiez le nom exact, la correspondance est sensible à la casse.
- **"plafond invalide" au démarrage :** `cost_ceiling_usd` est à zéro, négatif ou absent. Mettez une valeur strictement positive (exemple : `0.50`).
- **Le plafond est atteint immédiatement :** votre plafond est trop bas par rapport aux tâches lancées. Augmentez `cost_ceiling_usd` ou découpez vos tâches en sessions plus courtes.
- **Toutes les étapes passent en local alors que vous attendez de l'escalade :** votre modèle local est peut-être évalué comme suffisant pour vos tâches. Réduisez `cost_ceiling_usd` ou testez avec une tâche explicitement complexe pour confirmer que le routeur fonctionne.

> **Référence technique :** [Référence Apollia](../../reference/index.md) - paramètres de routing multi-backend, politique de fallback, calcul du coût par étape.
