# Configuration

Apollia OS sépare deux types de configuration : la configuration **structurelle** dans `apollia.toml` (immuable en cours d'exécution), et la configuration **opérationnelle** en SQLite (triggers, pipelines, notifications — modifiables à chaud via API).

> **Référence technique :** [Config-apollia-toml](https://github.com/nidal-z/apollia-os/wiki/Config-apollia-toml) — toutes les sections TOML avec leurs clés, valeurs par défaut, et descriptions.

---

## apollia.toml — configuration structurelle

Le fichier `~/.apollia/apollia.toml` est lu au démarrage. Il couvre neuf sections : `[runtime]`, `[oria]`, `[memory]`, `[tools]`, `[logging]`, `[a2a]`, `[chat]`, `[observability]`, `[stt]`, et les backends LLM via `[[llm.backends]]`.

```toml
# Exemple minimal — un backend LLM Anthropic
[[llm.backends]]
name        = "anthropic"
type        = "anthropic"
model       = "claude-haiku-4-5"
api_key_env = "ANTHROPIC_API_KEY"
default     = true
```

Si aucun backend n'est configuré, `ctx.llm` sera `None` dans les agents. Le runtime démarre avec un warning.

Pour la liste complète des clés disponibles par section, voir [Config-apollia-toml](https://github.com/nidal-z/apollia-os/wiki/Config-apollia-toml).

---

## Précédence de configuration

```
flags CLI  >  variables d'environnement  >  apollia.toml  >  valeurs par défaut
```

```bash
# Surcharger le port via flag
apollia-os start --api-port 7772

# Surcharger via variable d'environnement
APOLLIA_API_PORT=7772 apollia-os start

# Utiliser un fichier de config alternatif
apollia-os start --config ./dev.toml
```

---

## Configuration opérationnelle — SQLite

Les triggers, pipelines et canaux de notification ne sont **pas** dans `apollia.toml`. Ils sont gérés via l'API REST et persistés en SQLite :

| Données | Base SQLite | Géré via |
|---|---|---|
| Triggers | `~/.apollia/triggers_def.db` | API REST + Desktop |
| Pipelines | `~/.apollia/pipelines_def.db` | API REST + Desktop |
| Notifications | `~/.apollia/notifications.db` | API REST + Desktop |
| Sessions chat | `~/.apollia/chat.db` | API REST |
| Tâches | `~/.apollia/tasks.db` | Interne |
| Mémoire | `~/.apollia/memory/*.db` | Interne + CLI |
| Permissions & outils | `~/.apollia/governance.db` | Desktop + CLI |

La raison de cette séparation (ADR-033) : `apollia.toml` est lu au démarrage et ne change pas. Les triggers et pipelines doivent être modifiables à chaud via l'UI sans redémarrer le runtime. SQLite avec hot reload est la seule solution compatible.

---

## Trouver son fichier de config

```bash
# Localisation par défaut
cat ~/.apollia/apollia.toml

# Ouvrir dans l'éditeur par défaut
apollia-os config edit

# Afficher la config effective (avec défauts appliqués)
apollia-os config show --json
```

---

> **Référence complète :** [Config-apollia-toml](https://github.com/nidal-z/apollia-os/wiki/Config-apollia-toml) — toutes les sections `[runtime]`, `[oria]`, `[memory]`, `[tools]`, `[logging]`, `[a2a]`, `[chat]`, `[observability]`, `[stt]`, et `[[llm.backends]]` avec leurs valeurs par défaut.

---

## Lire les métriques de session depuis votre code

Le runtime maintient pour chaque exécution une `SessionMetrics` agrégée — steps consommés, appels d'outils par catégorie, latence cumulée, tokens LLM si applicable. Vous pouvez les lire depuis `ctx` à n'importe quel moment de `run()`, par exemple pour les logger en fin de tâche ou les exporter vers votre observabilité maison.

```python
async def run(self, task, ctx):
    # ... logique de l'agent ...

    # En fin de tâche : récupérer les métriques accumulées
    metrics = await ctx.session.metrics()

    # Forme : dict avec steps_used, tool_calls, llm_calls, llm_tokens_in,
    # llm_tokens_out, elapsed_secs, errors_count
    ctx.log.info(
        "session_done",
        steps=metrics["steps_used"],
        tools=metrics["tool_calls"],
        tokens=metrics["llm_tokens_in"] + metrics["llm_tokens_out"],
        elapsed=metrics["elapsed_secs"],
    )

    # Exporter vers une API maison (asynchrone, fire-and-forget)
    if metrics["llm_tokens_out"] > 5000:
        await ctx.tools.call("http_fetch", {
            "url":    "https://obs.example.com/ingest",
            "method": "POST",
            "body":   {"agent": "file-assistant", "metrics": metrics},
        })

    return {"task_id": task["task_id"], "status": "completed", "output": [...]}
```

Les métriques sont également émises automatiquement sur l'EventBus en fin de tâche (`RuntimeEvent::TaskCompleted`) — utilisez `ctx.session.metrics` uniquement si vous avez besoin de la valeur pendant l'exécution.
