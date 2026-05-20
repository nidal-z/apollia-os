# Anatomie d'un agent Apollia

Reprenons le code de `hello_agent.py` et disséquons chaque ligne. Cet exercice peut sembler trivial pour un agent aussi simple, mais les mêmes principes s'appliquent à tous les agents — du plus basique au plus complexe.

---

## Les 10 lignes essentielles

```python
# (1)  Un fichier Python ordinaire — pas de framework à importer
class HelloAgent:

    # (2)  manifest() est synchrone — le runtime l'appelle une seule fois au démarrage
    def manifest(self):
        return {
            "name": "hello-agent",          # (3)  Identifiant unique — pas d'espaces
            "version": "1.0.0",             # (4)  Semver — utilisé pour le registre
            "description": "...",           # (5)  Description lisible — affichée dans agent list
            "tools_required": [],           # (6)  Outils à résoudre au démarrage (vide = aucun)
            "max_concurrent_tasks": 1,      # (7)  Capacité : 1 tâche à la fois
        }

    # (8)  run() est async — peut faire des appels I/O sans bloquer le runtime
    async def run(self, task, ctx):
        parts = task["input"]["parts"]      # (9)  L'entrée est une liste de parties typées
        text = parts[0]["text"] if parts else "monde"
        return {
            "task_id": task["task_id"],     # (10) Toujours recopier le task_id dans la réponse
            "status": "completed",
            "output": [{"type": "text", "text": f"Bonjour ! J'ai reçu : {text}"}],
        }

# (11) Une variable agent au niveau module — le runtime cherche exactement ce nom
agent = HelloAgent()
```

Voici ce que chaque annotation signifie dans le détail.

---

## (1) Pas d'import requis

```python
class HelloAgent:
```

Il n'y a rien à importer. Pas de `from apollia import BaseAgent`. Le runtime utilise le **duck typing** : il vérifie simplement que votre objet a les méthodes `manifest()` et `run()`, sans imposer d'héritage ni de décorateur.

C'est une décision de conception délibérée : votre code Python ne dépend d'aucune bibliothèque Apollia. Si demain vous retirez Apollia OS, votre code reste valide Python pur.

> Le chapitre 3 présente un SDK Python optionnel qui fournit des classes de base et des mocks pour les tests. Ce SDK est du sucre syntaxique — pas une obligation.

---

## (2) manifest est synchrone

```python
def manifest(self):
```

`manifest()` est une méthode **synchrone** (pas `async`). Le runtime l'appelle une seule fois, au moment du déploiement (`apollia-os agent start`), avant d'accepter toute tâche.

C'est votre déclaration d'intention : "Voici ce dont j'ai besoin pour fonctionner." Le runtime utilise cette information pour préparer l'environnement d'exécution de l'agent.

---

## (3) name — l'identifiant de routage

```python
"name": "hello-agent",
```

Ce nom est l'identifiant que vous utilisez dans `apollia-os run <name> <input>`. Il doit être unique sur le runtime. Convention : kebab-case, pas d'espaces, pas de caractères spéciaux.

Le nom est aussi la clé de routage utilisée par les autres agents pour vous déléguer des tâches via A2A (voir chapitre 11).

---

## (4) version — semver pour le registre

```python
"version": "1.0.0",
```

Le versioning suit [semver](https://semver.org/). Il est utilisé par le registre d'agents communautaires (chapitre 8) pour gérer les mises à jour. Pour vos agents locaux, il est surtout informatif — affiché dans `apollia-os agent list`.

---

## (5) description — ce qui s'affiche dans la CLI

```python
"description": "Agent de démonstration minimal",
```

Cette chaîne est affichée dans `apollia-os agent list` et dans la découverte A2A. Rédigez-la comme une description d'outil : ce que l'agent fait, pas comment il le fait.

---

## (6) tools_required — le contrat de dépendances

```python
"tools_required": [],
```

C'est la liste des outils que votre agent veut utiliser via `ctx.tools`. Le runtime **résout** (vérifie l'existence) chaque outil de cette liste au démarrage. Si un outil est absent, le déploiement échoue immédiatement.

```python
# Exemple avec des outils déclarés :
"tools_required": ["file_io", "http_get"],
```

Ce mécanisme implémente le principe **fail-fast** : une configuration incomplète est détectée au démarrage, pas en plein milieu d'une exécution. Le chapitre 4 présente les 10 outils natifs disponibles.

---

## (7) max_concurrent_tasks — la capacité de l'agent

```python
"max_concurrent_tasks": 1,
```

Combien de tâches cet agent peut-il traiter en parallèle ? Le runtime crée un sémaphore de cette capacité. Toute tâche soumise au-delà de la capacité est mise en file d'attente.

Pour un agent simple comme `hello-agent`, 1 est correct. Pour un agent qui fait des appels réseau independants, une valeur plus élevée améliore le débit. La règle pratique : commencez à 1, augmentez seulement si vous avez mesuré un besoin de parallélisme.

---

## (8) run est async

```python
async def run(self, task, ctx):
```

`run()` est **toujours async**. Cette contrainte est importante : elle permet au runtime Tokio d'exécuter plusieurs agents et tâches sur le même thread sans les bloquer mutuellement.

En pratique, cela signifie que vous pouvez utiliser `await` librement dans `run()` — appels d'outils, accès mémoire, appels LLM — sans jamais bloquer le runtime.

Les deux paramètres :

- **`task`** — un dictionnaire qui décrit la tâche à exécuter (entrée, identifiant, métadonnées)
- **`ctx`** — le contexte d'exécution : accès aux outils (`ctx.tools`), à la mémoire (`ctx.memory`), au LLM (`ctx.llm`)

---

## (9) task["input"]["parts"] — l'entrée structurée

```python
parts = task["input"]["parts"]
text = parts[0]["text"] if parts else "monde"
```

L'entrée d'une tâche n'est pas une simple chaîne de caractères. C'est une liste de **parties**, chacune ayant un type :

| Type | Contenu | Exemple d'usage |
|---|---|---|
| `"text"` | Texte brut | Instructions, requêtes |
| `"data"` | JSON structuré | Paramètres, données métier |
| `"file"` | Chemin de fichier | Documents, images |

Cette structure permet à un agent de recevoir des entrées mixtes. Pour l'instant, `hello-agent` ne lit que la première partie texte. Le chapitre 3 détaille tous les types de parties.

---

## (10) Retourner un résultat structuré

```python
return {
    "task_id": task["task_id"],     # Toujours recopier !
    "status": "completed",
    "output": [{"type": "text", "text": f"..."}],
}
```

La réponse a trois champs obligatoires :

**`task_id`** — vous devez recopier l'identifiant de la tâche reçue. C'est ce qui permet au runtime de faire correspondre votre réponse à la requête originale, surtout en mode concurrent.

**`status`** — l'état final de l'exécution :

| Valeur | Signification |
|---|---|
| `"completed"` | Succès, l'output contient le résultat |
| `"failed"` | Échec, ajouter un champ `error` avec `code` et `message` |
| `"input_required"` | L'agent attend une intervention humaine (HITL, chapitre 10) |

**`output`** — même format que l'entrée : une liste de parties typées.

---

## (11) La variable agent au niveau module

```python
agent = HelloAgent()
```

Le runtime cherche exactement une variable nommée `agent` au niveau du module Python (pas dans une fonction, pas dans une classe). C'est le point d'entrée que PyO3 utilise pour accéder à votre objet.

Cette convention est simple et sans ambiguïté : un fichier = un agent.

---

## Récapitulatif visuel

```
hello_agent.py
│
├── class HelloAgent
│   ├── manifest()  → dict de configuration (appelé 1 fois, au démarrage)
│   │   ├── name              → identifiant de routage
│   │   ├── version           → semver
│   │   ├── tools_required    → résolution fail-fast
│   │   └── max_concurrent_tasks → capacité du sémaphore
│   │
│   └── run(task, ctx)  → logique métier (appelé à chaque tâche)
│       ├── task["input"]["parts"]  → entrée structurée
│       ├── ctx.tools / ctx.memory / ctx.llm  → services runtime
│       └── return { task_id, status, output }
│
└── agent = HelloAgent()  → point d'entrée du runtime
```

---

## Ce qui vient ensuite

Vous avez maintenant une compréhension solide du contrat minimal qu'Apollia OS attend de tout agent. Dans le chapitre 2, vous allez construire un agent plus réaliste — un assistant qui lit des fichiers, les résume via LLM, et sauvegarde le résultat. Chaque concept introduit ici sera réutilisé et enrichi.
