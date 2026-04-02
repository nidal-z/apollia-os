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
