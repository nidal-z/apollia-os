# Bonjour, Agent !

Maintenant que le runtime est installé, construisons le plus simple des agents possibles. L'objectif n'est pas de faire quelque chose d'impressionnant — c'est de comprendre le flux complet : écrire un fichier Python, le déployer, envoyer une tâche, lire le résultat.

Ce que vous allez construire : un agent `hello-agent` qui reçoit du texte et répond avec une salutation. Pas de LLM. Pas d'outils. Juste le contrat minimal qu'Apollia OS attend de tout agent.

---

## Le fichier complet

Créez un fichier `hello_agent.py` dans le répertoire de votre choix :

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

Deux méthodes. Pas de classe de base. Pas de package à installer. C'est tout ce qu'Apollia OS demande. La section suivante ([Anatomie d'un agent](./ch01-03-anatomy.md)) explique chaque ligne en détail.

---

## Étape 1 — Démarrer le runtime

```bash
$ apollia-os start
  Apollia OS v0.1.0 démarrage...
  ✔ EventBus         prêt
  ✔ AgentRegistry    prêt
  ✔ Tool Registry    prêt
  ✔ TaskRouter       prêt
  ✔ APIServer        écoute sur /tmp/apollia.sock · localhost:7771
  ✔ Runtime prêt en 0.8s
```

Le runtime démarre en arrière-plan. Il expose deux interfaces : un socket Unix (`/tmp/apollia.sock`) pour les appels locaux rapides, et TCP sur le port 7771 pour les appels réseau. La CLI utilise le socket Unix par défaut.

Vérifiez que le runtime est actif :

```bash
$ apollia-os status
  Runtime    ACTIVE
  Agents     0 actifs
  Tâches     0 en cours
```

---

## Étape 2 — Déployer l'agent

```bash
$ apollia-os agent start ./hello_agent.py
  Chargement de hello_agent.py...
  Validation AIP...
    ✔ manifest() — OK
    ✔ run() async — OK
    ✔ tools_required : 0 outils (aucun à résoudre)
  ✔ hello-agent [ACTIVE]
```

Que se passe-t-il en coulisse lors du déploiement ?

1. Le runtime charge le module Python via PyO3 (le pont Rust ↔ Python)
2. Il appelle `agent.manifest` et valide le dictionnaire retourné
3. Il résout chaque outil listé dans `tools_required` (zéro ici)
4. Il crée un coordinateur d'exécution avec une capacité de 1 tâche concurrente
5. Il fait passer l'agent en état `ACTIVE`

Si votre agent avait déclaré un outil inexistant dans `tools_required`, le runtime aurait refusé de démarrer à l'étape 3. Ce comportement **fail-fast** est intentionnel : mieux vaut découvrir un problème de configuration au démarrage qu'en cours d'exécution.

Listez les agents déployés :

```bash
$ apollia-os agent list
  NAME          STATUS    TASKS    VERSION
  hello-agent   ACTIVE    0/1      1.0.0
```

---

## Étape 3 — Envoyer une tâche

```bash
$ apollia-os run hello-agent "Dupont SA"
  -> Task t-abc123 submitted to hello-agent
  Executing...
  Done in 0.3s (1 step, 0 tool calls)

  RESULT
  Bonjour ! J'ai reçu : Dupont SA
```

Décortiquons l'output :

- **`t-abc123`** — identifiant unique de cette tâche, généré par le runtime
- **`1 step`** — l'agent a effectué un seul cycle de raisonnement (un appel à `run()`)
- **`0 tool calls`** — aucun outil n'a été invoqué via `ctx.tools`
- **`RESULT`** — la section qui suit contient l'`output` retourné par l'agent

Envoyez une deuxième tâche pour voir que l'agent reste actif :

```bash
$ apollia-os run hello-agent "Acme Corp"
  Done in 0.2s (1 step, 0 tool calls)

  RESULT
  Bonjour ! J'ai reçu : Acme Corp
```

---

## Étape 4 — Observer l'état

Consultez l'état de l'agent et l'historique des tâches :

```bash
$ apollia-os agent info hello-agent
  ProcessState : ACTIVE
  Tâches       : 0 / 1 (idle)
  Outils       : (aucun)

$ apollia-os task list
  TASK_ID     AGENT         STATUS      DURATION
  t-def456    hello-agent   completed   0.2s
  t-abc123    hello-agent   completed   0.3s
```

`ProcessState: ACTIVE` signifie que l'agent est prêt à recevoir de nouvelles tâches. `0 / 1 (idle)` indique qu'aucune tâche n'est en cours sur la capacité de 1 tâche concurrente déclarée dans le manifest.

---

## Étape 5 — Arrêter proprement

```bash
$ apollia-os stop
  Drain des tâches en cours... (0 actives)
  ✔ Runtime arrêté proprement
```

Le runtime attend toujours que les tâches en cours se terminent avant de s'arrêter. C'est le shutdown gracieux — vous ne perdez pas de travail en cours.

---

## Gérer les erreurs dans run

Un agent bien écrit gère explicitement les cas d'erreur au lieu de laisser le runtime gérer les exceptions Python. Voici un `run()` plus robuste :

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

`status: "failed"` avec un champ `error` structuré permet à l'appelant (un orchestrateur, un pipeline, ou l'utilisateur via CLI) de comprendre précisément ce qui a échoué et pourquoi.

---

## Ce que vous avez appris

- Un agent Apollia OS est un objet Python avec deux méthodes : `manifest()` et `run()`
- Le runtime valide l'agent au démarrage (fail-fast) avant de l'activer
- `run()` reçoit une tâche structurée et retourne un résultat structuré
- Le runtime gère le cycle de vie, la concurrence, et le shutdown propre

La section suivante décompose chaque élément de ce code pour comprendre ce que chaque ligne signifie et pourquoi elle est là.
