# La CLI complète

La CLI `apollia` suit le pattern `<noun> <verb>`, cohérent avec `docker container run` ou `kubectl get pods`. Une fois le pattern appris, toutes les commandes sont prévisibles.

Le binaire principal est `apollia`. Quand le SDK Python est installé, vous avez aussi accès à `python -m apollia` qui propose un sous-ensemble (`inspect`, `new`) sans avoir besoin du runtime démarré.

> **Référence technique :** la liste exhaustive des sous-commandes, leurs flags et leurs sorties JSON sera dans la page wiki `Briques-CLI` *(wiki disponible prochainement)*.

---

## Trois niveaux de profondeur

```
Niveau 1 (admin, usage quotidien) : start, stop, status, run
Niveau 2 (développeur)            : agent, task, tools, permissions, memory,
                                    audit, llm, trigger, notify, stt, secrets
Niveau 3 (debug)                  : --verbose, --debug, --raw
```

Le Niveau 1 suffit pour opérer un système en production. Le Niveau 2 sert au développement et au diagnostic. Le Niveau 3 expose l'état interne des acteurs Tokio.

---

## Commandes essentielles

### `apollia-os start`

```bash
$ apollia-os start
```

Démarre le runtime en foreground. Les logs sortent sur stderr (`tracing` configurable via `RUST_LOG`). Si aucun backend LLM n'est configuré, le runtime démarre quand même avec un avertissement et `ctx.llm` lèvera une erreur à la première utilisation.

Pour stopper proprement : `Ctrl+C` ou `apollia-os stop` depuis un autre shell.

### `apollia-os run`

```bash
$ apollia-os run research-director "Analyse le rapport /tmp/q3.pdf"
```

Soumet une tâche one-shot à l'agent et attend le résultat. Flags utiles : `--json` (sortie machine), `--verbose` (durée et compteurs de steps), `--debug` (traces ORIA sur stderr).

### `apollia-os status`

```bash
$ apollia-os status
  Runtime  ACTIVE

  AGENTS (3 active)
  NAME                   STATE
  research-director      * active
  pdf-worker             * active
  veille-ia                stopped
```

Vue rapide de l'état du runtime + liste des agents enregistrés avec leur état (`* active`, ` stopped`, `! degraded`).

### `python -m apollia inspect <agent.py>`

Validation statique d'un fichier agent, sans démarrer le runtime (cf. [chapitre 27](../part-vii-tooling/27-apollia-inspect.md)). Disponible aussi via le binaire `apollia inspect` quand le SDK Python est installé dans le PATH.

### `python -m apollia new <name> --type <type>`

Scaffold d'un nouvel agent (cf. [chapitre 28](../part-vii-tooling/28-apollia-new-scaffolding.md)).

### `apollia-os agent install`

```bash
$ apollia-os agent install ./my_worker.py
Agent 'my-worker' v0.1.0 installed successfully
```

Une ligne en cas de succès. En cas d'erreur, le message indique précisément la cause (`manifest invalid`, `venv setup failed`, etc.).

Pour activer l'agent au prochain boot (et le rendre invocable immédiatement) :

```bash
$ apollia-os agent enable my-worker
```

### `apollia-os agent list`

```bash
$ apollia-os agent list
  NAME                     VERSION    STATUS       AUTO-LOAD  SOURCE
  apollia-guide            0.2.0      active       yes        installed
  pdf-worker               0.1.0      active       yes        installed
  veille-ia                0.2.0      active       yes        installed
```

Cinq colonnes : nom, version, statut runtime, auto-load au boot (oui/non), source d'installation (`installed`, `bundled`, `dev`).

Pour la liste des skills A2A exposées par les workers actifs : `apollia-os a2a skills`.

### `apollia-os a2a invoke <skill_id> --args '<JSON>'`

```bash
$ apollia-os a2a invoke pdf.read_text --args '{"path": "/tmp/report.pdf"}'
  Skill     : pdf.read_text
  Worker    : pdf-worker
  Status    : completed
  Duration  : 84 ms
  Output    :
    [
      {
        "type": "data",
        "data": { "text": "...", "page_count": 12 }
      }
    ]
```

Le worker cible est résolu automatiquement à partir du `skill_id` (pas besoin de le nommer). `--args '<JSON>'` passe le payload, `--args-file` accepte un chemin (ou `-` pour stdin), `--json` retourne la sortie machine complète.

### `apollia-os permissions list`

Le moteur de permissions enregistre vos choix "Toujours autoriser" dans `governance.db`. La sous-commande `permissions` les inspecte et révoque.

```bash
$ apollia-os permissions list
  ID    OUTIL          PORTÉE    ARGUMENT             EXPIRATION   CRÉÉ LE
  1     file_write     project   /tmp/ @ /mon/proj    permanente   2026-04-25
  2     web_search     global    (tous)               permanente   2026-04-22

$ apollia-os permissions revoke 1
  ✔ Règle #1 révoquée
```

### `apollia-os task resume <id>`

```bash
$ apollia-os task list --pending-approval
  ID        AGENT              DEPUIS   PROMPT
  t-042     invoice-router     14min    Où ranger la facture Acme Corp ?

$ apollia-os task resume t-042 --approve --input "frais-bureau"
  ✔ Tâche t-042 reprise
```

---

## Flags globaux

| Flag | Description |
|---|---|
| `--json` | Sortie JSON sur stdout, désactive couleurs et progress bars |
| `--socket PATH` | Socket Unix alternatif (défaut `/tmp/apollia.sock`) |
| `-q` / `--quiet` | Succès ou erreur uniquement, aucun détail |
| `-v` / `--verbose` | Détails supplémentaires (durées, steps count) |
| `--debug` | Logs internes + traces ORIA sur stderr |
| `--no-color` | Désactive les couleurs ANSI |

Les sorties humaines (tableaux, progress bars, couleurs) sont activées uniquement si stdout est un terminal, désactivées automatiquement dans les pipes et les scripts CI.

---

## Codes de sortie

```
0   Succès
1   Erreur générale (usage, input invalide)
2   Erreur runtime (runtime non démarré, connexion refusée)
3   Tâche échouée (run --wait avec tâche en échec)
4   Timeout
5   Interrompu (Ctrl+C / SIGINT, shutdown gracieux puis exit)
```

Usage en script bash :

```bash
apollia-os run research-director "..." --wait || {
  echo "Tâche échouée (code: $?)"
  apollia-os audit list --limit 1
}
```

---

## Aide intégrée

`apollia-os --help` liste les commandes disponibles. Extrait :

```
Usage: apollia-os [OPTIONS] <COMMAND>

Commands:
  start        Start the runtime in foreground
  stop         Stop a running runtime
  status       Display runtime and agent status
  run          Submit a task to an agent and wait for the result
  auth         OAuth2 PKCE authentication management (login, status, logout)
  agent        Agent management (list, start, stop, info, install, uninstall,
               enable, disable, update, new, package, logs, validate, repair)
  a2a          Agent-to-Agent skill discovery and direct invocation
  task         Task management (list, status, cancel, inspect, resume,
               approvals)
  tools        Native tool governance (list, enable, disable, config, reload,
               credentials, describe, approvals)
  audit        Audit trail (list, stats, export)
  memory       Memory management
  llm          LLM backend diagnostics (status, ping, chat)
  model        Local model file management
  trigger      Trigger management (list, status, fire, enable, disable, logs,
               reload, create, update, delete)
  notify       Notification channel management (test, list, logs)
  stt          Speech-to-Text management
  permissions  Permission rule management (list, revoke, audit)
  chat         Interactive chat session with an LLM
  mcp          MCP server management
```

Côté SDK Python (sans runtime démarré) :

```
python -m apollia inspect <agent.py>
python -m apollia new <name> --type <worker|conversational|react|orchestrated>
```

---

## ADRs

- `ADR-004` : Pattern noun-verb pour la CLI
- `ADR-004` : Bootstrap CLI sans Supervisor

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
