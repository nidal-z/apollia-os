# Agents — Troubleshooting — Apollia OS

> Guide de diagnostic par symptôme. Chaque section commence par le message d'erreur exact ou le comportement observé.
> Public cible : tous les développeurs d'agents

---

## Symptôme : `error: no Python interpreter found` au build

**Cause :** PyO3 ne trouve pas l'interpréteur Python sur macOS ou dans un environnement non-standard.

**Solution :**
```bash
export PYO3_PYTHON=/opt/homebrew/bin/python3.13
cargo build --workspace
```

**Vérification :**
```bash
cargo build --workspace 2>&1 | grep -i python
# Ne doit plus afficher d'erreur
```

---

## Symptôme : `address already in use` au démarrage du runtime

**Cause :** Un processus `apollia-os` précédent est toujours en cours, ou le socket `/tmp/apollia.sock` est orphelin.

**Solution :**
```bash
# Arrêt propre d'abord
apollia-os stop

# Si ça ne répond pas, forcer
pkill apollia-os
rm -f /tmp/apollia.sock
```

**Vérification :**
```bash
apollia-os start
apollia-os status
```

---

## Symptôme : agent bloqué en `INITIALIZING`

**Cause :** Un `tools_required` est introuvable dans le registre, ou `on_start()` bloque indéfiniment.

**Diagnostic :**
```bash
apollia-os agent info mon-agent
# Vérifier la colonne STATUS et les logs

RUST_LOG=debug apollia-os start
apollia-os agent start ./mon_agent.py 2>&1 | grep -i "tool\|error\|init"
```

**Solutions courantes :**
- Outil déclaré dans `tools_required` mais pas dans la liste des outils disponibles → utiliser `apollia-os tools list` pour vérifier
- `on_start()` appelle une API externe qui timeout → ajouter un timeout explicite dans `on_start()`

**Vérification :**
```bash
apollia-os tools list
# Vérifier que tous les tools_required sont présents
```

---

## Symptôme : tâche échoue avec `BudgetExceeded`

**Cause :** Le StepBudget a été atteint (max_steps, max_tool_calls ou wall_clock_timeout).

**Diagnostic :**
```bash
apollia-os task status t-abc123
# Vérifier Steps et ToolCalls vs leur maximum
```

**Solutions :**
1. Augmenter les limites dans le manifest :
```python
"step_budget": {
    "max_steps": 30,
    "max_tool_calls": 60,
    "wall_clock_timeout_secs": 600
}
```

2. Ajouter une vérification dans `run()` :
```python
if ctx.step_budget.steps_remaining < 3:
    return {"task_id": task["task_id"], "status": "completed",
            "output": [{"type": "text", "text": "Résultat partiel"}]}
```

**Vérification :**
```bash
apollia-os run mon-agent "test" --json | jq '.status'
# "completed"
```

---

## Symptôme : agent passe en `DEGRADED` au lieu de `ACTIVE`

**Cause :** Un outil déclaré dans `tools_optional` est introuvable. L'agent démarre mais en mode dégradé.

**Diagnostic :**
```bash
apollia-os agent info mon-agent
# La colonne Outils montrera les outils manquants avec ✗
```

**Solution :** Vérifier que l'outil optionnel est bien enregistré dans le Tool Registry. Si l'outil n'est jamais disponible, le retirer de `tools_optional`.

**Note :** un agent `DEGRADED` peut tout de même traiter des tâches. Si les tâches ne nécessitent pas l'outil manquant, c'est un comportement acceptable.

**Cas particulier — préfixe `a2a:`** : les dépendances `tools_required` / `tools_optional` commençant par `a2a:` (skills inter-agents) ne sont **pas** vérifiées dans le `ToolRegistry` au boot — elles sont résolues d'office par le resolver et leur résolution réelle a lieu à l'invocation via le `ToolProxy` + `A2AInvoker`. Un agent qui ne déclare que des dépendances `a2a:` ne doit donc jamais passer en `DEGRADED` pour cause d'outil manquant.

---

## Symptôme : `ToolNotAllowed` lors d'un appel ctx.tools.call

**Cause :** L'agent tente d'utiliser un outil qu'il n'a pas déclaré dans `tools_required` ou `tools_optional`.

**Solution :**
```python
def manifest(self):
    return {
        # Ajouter l'outil manquant ici
        "tools_required": ["file_io", "bash_executor"],
    }
```

**Vérification :**
```bash
apollia-os agent info mon-agent
# La colonne Outils doit lister bash_executor
```

---

## Symptôme : ctx.memory est None alors que je l'attends

**Cause :** `memory_namespace` n'est pas défini dans le manifest.

**Solution :**
```python
def manifest(self):
    return {
        "memory_namespace": "mon-agent-memory",  # ajouter cette ligne
        ...
    }
```

Redéployer l'agent (`apollia-os agent stop` + `apollia-os agent start`).

---

## Symptôme : exception Python dans run non diagnostiquable

**Cause :** L'exception n'est pas catchée et le message est tronqué dans les logs.

**Solution :** Logger l'exception dans `run()` avant de retourner un résultat d'erreur :

```python
async def run(self, task, ctx):
    try:
        # ... code de l'agent
    except Exception as e:
        import traceback
        ctx.log.error("unexpected_error",
                      error=str(e),
                      traceback=traceback.format_exc()[:500])
        return {
            "task_id": task["task_id"],
            "status": "failed",
            "error": {"code": "UNEXPECTED_ERROR", "message": str(e)}
        }
```

**Diagnostic complet :**
```bash
RUST_LOG=debug apollia-os start 2>&1 | tee /tmp/apollia-debug.log
apollia-os run mon-agent "test input"
grep "unexpected_error\|ERROR" /tmp/apollia-debug.log
```

---

## Symptôme : `apollia-os run` timeout sans résultat

**Cause :** L'agent est `ACTIVE` mais la tâche ne termine pas dans le délai wall_clock.

**Diagnostic :**
```bash
apollia-os task list
# Vérifier le statut : working ou input_required ?

apollia-os task status <task_id>
# Vérifier Elapsed vs le max
```

**Solutions :**
- Si `input_required` : l'agent attend une entrée → `apollia-os task resume <id> --input "..."`
- Si `working` bloqué : vérifier que `run()` n'est pas bloqué sur une opération synchrone sans `await`
- Augmenter `wall_clock_timeout_secs` dans le manifest

---

## Symptôme : `dylibError: Library not loaded: libpython3.XX.dylib` (macOS)

**Cause :** Python n'a pas été compilé avec `--enable-shared`, ou la dylib est introuvable au runtime.

**Solution :**
```bash
brew reinstall python@3.13
export PYO3_PYTHON=/opt/homebrew/bin/python3.13
cargo build --workspace
```

**Si le problème persiste :**
```bash
export DYLD_LIBRARY_PATH=/opt/homebrew/lib:$DYLD_LIBRARY_PATH
apollia-os start
```

---

## Voir aussi

- [INSTALL.md](./INSTALL) — installation complète avec configuration PyO3
- [Briques CLI](./Briques-CLI) — toutes les commandes de diagnostic
- [Ops Exploitation et Debug](./Ops-Exploitation-et-Debug) — monitoring en production
