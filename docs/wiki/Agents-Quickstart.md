# Agents — Quickstart — Apollia OS

> Créer et exécuter votre premier agent Python en 5 minutes chrono.
> Public cible : développeur Python, nouveau sur Apollia OS

---

## Prérequis

- Apollia OS compilé : `cargo build --workspace --release`
- `target/release/apollia-os` dans votre PATH (ou `cargo install --path crates/apollia-cli`)
- Python 3.11+

```bash
apollia-os --version
# apollia-os 0.1.0
```

---

## Étape 1 — Écrire l'agent

Créez un fichier `hello_agent.py` :

```python
# hello_agent.py
class HelloAgent:
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

Deux méthodes suffisent. Pas de classe de base. Pas de package à installer.

---

## Étape 2 — Démarrer le runtime

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

Le runtime tourne en arrière-plan. Vérifier l'état :

```bash
$ apollia-os status
  Runtime    ACTIVE
  Agents     0 actifs
  Tâches     0 en cours
```

---

## Étape 3 — Déployer l'agent

```bash
$ apollia-os agent start ./hello_agent.py
  Chargement de hello_agent.py...
  Résolution des outils... (0 requis)
  ✔ hello-agent [ACTIVE]
```

Lister les agents déployés :

```bash
$ apollia-os agent list
  NAME          STATUS    TASKS    VERSION
  hello-agent   ACTIVE    0/1      1.0.0
```

---

## Étape 4 — Exécuter une tâche

```bash
$ apollia-os run hello-agent "Dupont SA"
  -> Task t-abc123 submitted to hello-agent
  Executing...
  Done in 0.3s (1 step, 0 tool calls)

  RESULT
  Bonjour ! J'ai reçu : Dupont SA
```

---

## Étape 5 — Arrêter proprement

```bash
$ apollia-os stop
  Drain des tâches en cours... (0 actives)
  ✔ Runtime arrêté proprement
```

---

## Alternative — Utiliser le SDK Python

Pour des agents plus structurés avec IDE autocomplete, mocks de test et scaffolding :

```bash
$ pip install -e ./sdk
$ apollia-os agent new mon-agent --type react
```

> Vous pouvez aussi appeler directement le SDK : `python -m apollia new mon-agent --type react`

Le SDK fournit `BaseReActAgent`, `ConversationalAgent`, `OrchestratedAgent` avec type stubs PEP 561 et infrastructure de test complète. Voir [Agents SDK Guide](./Agents-SDK-Guide).

---

## Ce qui vient ensuite

Vous avez un agent fonctionnel. Pour aller plus loin :

- **Python SDK** : [Agents SDK Guide](./Agents-SDK-Guide) — classes de base, mocks, scaffolding
- **Utiliser les outils** : [Agents RuntimeContext Guide](./Agents-RuntimeContext-Guide) — `ctx.tools`, `ctx.memory`
- **Tutoriel détaillé** : [Agents Tutoriel Hello Agent](./Agents-Tutoriel-Hello-Agent) — explications pas à pas
- **Agents complexes** : [Agents Bonnes Pratiques](./Agents-Bonnes-Pratiques) — StepBudget, coûts LLM
- **Adapter un agent existant** : [Agents Adapter Existants](./Agents-Adapter-Existants) — LangGraph, CrewAI
- **AIP complet** : [Briques AIP Specification](./Briques-AIP-Specification) — tous les champs, tous les types

## Voir aussi

- [INSTALL.md](./INSTALL) — installation complète avec PyO3 macOS
- [Briques CLI](./Briques-CLI) — toutes les commandes disponibles
