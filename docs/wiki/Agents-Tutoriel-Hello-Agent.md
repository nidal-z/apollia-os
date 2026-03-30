# Agents — Tutoriel Hello Agent — Apollia OS

> Tutoriel pas à pas complet : de zéro à un agent fonctionnel avec explications détaillées de chaque concept.
> Public cible : développeur Python débutant sur Apollia OS

---

## Ce que vous allez construire

Un agent `hello-agent` qui :
1. Reçoit du texte en entrée
2. Le salue personnellement
3. Tourne localement sans aucune dépendance cloud

Durée estimée : 15-20 minutes (en incluant la compilation).

---

## Prérequis

```bash
rustc --version   # >= 1.75
python3 --version # >= 3.11
cargo build --workspace  # doit compiler sans erreur
```

Si la compilation échoue, voir [INSTALL.md](./INSTALL).

---

## Partie 1 — Comprendre la structure d'un agent

Un agent Apollia OS est **un objet Python avec deux méthodes** :

```python
class MonAgent:
    def manifest(self):  # méthode synchrone
        ...

    async def run(self, task, ctx):  # méthode asynchrone
        ...

agent = MonAgent()  # instance au niveau module — obligatoire
```

Pas de classe de base. Pas de décorateur. Pas de `__init__` requis. Le runtime utilise le **duck typing** : il vérifie que votre objet a les méthodes `manifest()` et `run()`, sans imposer d'héritage (voir [ADR-003](../decisions/adr-003-duck-typing-aip) et [Glossaire](../glossary)).

La ligne `agent = MonAgent()` est importante : le runtime cherche une variable `agent` au niveau module quand il charge le fichier Python.

---

## Partie 2 — Le manifest

Le manifest décrit ce dont l'agent a besoin. Le runtime le lit à `INITIALIZING` pour :
- Vérifier que les outils requis sont disponibles
- Ouvrir le namespace mémoire si demandé
- Configurer le StepBudget

```python
def manifest(self):
    return {
        "name": "hello-agent",       # identifiant unique — pas d'espaces
        "version": "1.0.0",          # semver
        "description": "Agent de démonstration minimal",
        "tools_required": [],        # pas d'outils pour ce premier agent
    }
```

`tools_required` vide signifie que l'agent peut fonctionner sans aucun outil. Au démarrage, le runtime **résout** (vérifie l'existence dans le Tool Registry) chaque outil déclaré ici. Si un outil requis est absent, l'agent refuse de démarrer. C'est parfait pour commencer sans dépendance.

---

## Partie 3 — La méthode run

```python
async def run(self, task, ctx):
    # task est un dict avec les données de la tâche
    # ctx donne accès aux services du runtime
    ...
```

### Accéder à l'entrée

```python
async def run(self, task, ctx):
    # task["input"]["parts"] est une liste de parties
    # Chaque partie a un "type" ("text", "data", "file")
    parts = task["input"]["parts"]

    # Récupérer le texte de la première partie
    text = parts[0]["text"] if parts else "monde"
    # Si aucune partie, on utilise "monde" comme valeur par défaut
```

### Retourner un résultat

```python
    return {
        "task_id": task["task_id"],  # toujours recopier le task_id
        "status": "completed",       # ou "failed", "input_required"
        "output": [
            {"type": "text", "text": f"Bonjour ! J'ai reçu : {text}"}
        ],
    }
```

Le champ `output` est une liste de parties — le même format que l'entrée.

---

## Partie 4 — L'agent complet

```python
# hello_agent.py


class HelloAgent:
    """Agent de démonstration minimal — salue l'entrée reçue."""

    def manifest(self):
        return {
            "name": "hello-agent",
            "version": "1.0.0",
            "description": "Agent de démonstration minimal",
            "tools_required": [],
            "max_concurrent_tasks": 1,
        }

    async def run(self, task, ctx):
        parts = task["input"]["parts"]
        text = parts[0]["text"] if parts else "monde"
        return {
            "task_id": task["task_id"],
            "status": "completed",
            "output": [{"type": "text", "text": f"Bonjour ! J'ai reçu : {text}"}],
        }


agent = HelloAgent()
```

Sauvegardez ce fichier sous `hello_agent.py`.

---

## Partie 5 — Démarrer et tester

### Étape 1 : Démarrer le runtime

```bash
$ apollia-os start
  ✔ Runtime prêt en 0.8s
```

### Étape 2 : Déployer l'agent

```bash
$ apollia-os agent start ./hello_agent.py
  Chargement de hello_agent.py...
  Validation AIP...
    ✔ manifest() — OK
    ✔ run() async — OK
    ✔ tools_required : 0 outils (aucun à résoudre)
  ✔ hello-agent [ACTIVE]
```

Que se passe-t-il en coulisse ?
1. Le runtime charge le module Python via PyO3
2. Il appelle `agent.manifest()` et valide le dict retourné
3. Il résout les `tools_required` (aucun ici)
4. Il crée un `ExecutionCoordinator` avec un semaphore de capacité 1
5. Il passe l'agent en état `ACTIVE`

### Étape 3 : Envoyer une tâche

```bash
$ apollia-os run hello-agent "Dupont SA"
  -> Task t-abc123 submitted to hello-agent     # ID unique de la tâche
  Executing...
  Done in 0.3s (1 step, 0 tool calls)           # 1 step = 1 cycle run(), 0 outil appelé

  RESULT
  Bonjour ! J'ai reçu : Dupont SA
```

> **Comprendre l'output :** `t-abc123` est l'identifiant unique de la tâche. "1 step" signifie que l'agent a effectué un cycle de raisonnement (son `run()`). "0 tool calls" confirme qu'aucun outil (`ctx.tools`) n'a été appelé.

### Étape 4 : Observer l'état

```bash
$ apollia-os agent info hello-agent
  ProcessState : ACTIVE
  Tâches       : 0 / 1 (idle)
  Outils       : (aucun)

$ apollia-os task list
  TASK_ID     AGENT         STATUS      DURATION
  t-abc123    hello-agent   completed   0.3s
```

### Étape 5 : Arrêter proprement

```bash
$ apollia-os stop
  ✔ Runtime arrêté proprement
```

---

## Partie 6 — Gérer les erreurs

Modifiez `run()` pour démontrer la gestion d'erreur :

```python
async def run(self, task, ctx):
    parts = task["input"]["parts"]

    if not parts:
        return {
            "task_id": task["task_id"],
            "status": "failed",
            "error": {
                "code": "MISSING_INPUT",
                "message": "Au moins une partie est requise"
            }
        }

    text = parts[0]["text"]

    if len(text) > 1000:
        return {
            "task_id": task["task_id"],
            "status": "failed",
            "error": {
                "code": "INPUT_TOO_LONG",
                "message": f"Maximum 1000 caractères, reçu {len(text)}"
            }
        }

    return {
        "task_id": task["task_id"],
        "status": "completed",
        "output": [{"type": "text", "text": f"Bonjour ! J'ai reçu : {text}"}],
    }
```

---

## Partie 7 — Ce qui vient ensuite

Vous avez un agent AIP-compatible fonctionnel. Prochaines étapes :

**Utiliser le SDK Python** : pour des agents plus structurés avec IDE autocomplete, classes de base et mocks de test, installez le SDK (`pip install -e ./sdk`) et utilisez `BaseReActAgent`, `ConversationalAgent` ou `OrchestratedAgent`. Voir [Agents SDK Guide](./Agents-SDK-Guide).

**Ajouter des outils** : modifiez le manifest pour déclarer `"tools_required": ["file_io"]`, puis utilisez `await ctx.tools.call("file_io", {...})` dans `run()`. Voir [Agents RuntimeContext Guide](./Agents-RuntimeContext-Guide).

**Ajouter de la mémoire** : ajoutez `"memory_namespace": "hello-memory"` au manifest. Le runtime ouvrira automatiquement un namespace SQLite dédié. Voir [Briques Memory Engine](./Briques-Memory-Engine).

**Adapter un agent existant** : si vous avez déjà un agent LangGraph ou CrewAI, voir [Agents Adapter Existants](./Agents-Adapter-Existants).

## Voir aussi

- [Agents SDK Guide](./Agents-SDK-Guide) — SDK Python complet (classes de base, mocks, scaffolding)
- [Agents Quickstart](./Agents-Quickstart) — version condensée (5 min)
- [Briques AIP Specification](./Briques-AIP-Specification) — référence complète AIP
- [Agents RuntimeContext Guide](./Agents-RuntimeContext-Guide) — tous les services `ctx.*`
