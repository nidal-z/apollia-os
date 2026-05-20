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

### `apollia start`

```bash
$ apollia start
  Apollia OS v0.1.0 démarrage...
  ✔ EventBus         prêt
  ✔ AgentRegistry    prêt
  ✔ Tool Registry    16 outils chargés
  ✔ Memory Engine    prêt (FTS5)
  ✔ LlmRouter        2 backends (local, anthropic)
  ✔ TaskRouter       prêt
  ✔ APIServer        /tmp/apollia.sock, localhost:7771
  ─────────────────────────────────────────────────
  ✔ Runtime prêt en 1.2s
```

Si aucun backend LLM n'est configuré, le runtime démarre quand même avec un avertissement.

### `apollia run`

```bash
$ apollia run research-director "Analyse le rapport /tmp/q3.pdf"
  → Tâche t-009 soumise
  ⠿ Exécution en cours...
  ✔ Terminé en 3.1s (4 étapes, 3 appels outils)

  RÉSULTAT
  Le rapport présente trois axes : ...
```

Flags utiles : `--stream` (streaming token par token), `--json` (sortie machine), `--detach` (fire and forget).

### `apollia status`

```bash
$ apollia status

  Apollia OS v0.1.0  ●  Running depuis 2h14m  ·  PID 12345

  AGENTS (3 actifs)
  ────────────────────────────────────────────────────────────
  NOM                  ÉTAT       TÂCHES   DERNIÈRE ACTIVITÉ
  research-director    ● actif    47       il y a 3min
  pdf-worker           ● actif    12       il y a 1h
  veille-ia            ○ dégradé   5       il y a 30min
    └ ⚠ mcp_external indisponible (circuit ouvert)

  SANTÉ OUTILS
  ────────────────────────────────────────────────────────────
  bash_executor    ✔  file_read     ✔  file_write    ✔
  web_read         ✔  web_search    ✔  mcp_external  ✗  (retry dans 18s)
```

### `apollia inspect <agent.py>`

Validation statique d'un fichier agent, sans démarrer le runtime (cf. [chapitre 27](../part-vii-tooling/27-apollia-inspect.md)).

### `apollia new <name> --type <type>`

Scaffold d'un nouvel agent (cf. [chapitre 28](../part-vii-tooling/28-apollia-new-scaffolding.md)).

### `apollia agent install`

```bash
$ apollia agent install ./my_worker.py
  ✔ Manifest validé (worker, 2 skills)
  ✔ Venv créé (pypdf>=4 installé)
  ✔ Agent enregistré (état: ACTIVE)
  ✔ Skills indexées (my.skill_a, my.skill_b)
```

### `apollia invoke <agent> <skill> [args...]`

```bash
$ apollia invoke pdf-worker pdf.read_text path=/tmp/report.pdf
  {"text": "...", "page_count": 12}
```

Convention : `key=value` à plat ou `--json` pour un payload JSON complet.

### `apollia permissions list`

Le moteur de permissions enregistre vos choix "Toujours autoriser" dans `governance.db`. La sous-commande `permissions` les inspecte et révoque.

```bash
$ apollia permissions list
  ID    OUTIL          PORTÉE    ARGUMENT             EXPIRATION   CRÉÉ LE
  1     file_write     project   /tmp/ @ /mon/proj    permanente   2026-04-25
  2     web_search     global    (tous)               permanente   2026-04-22

$ apollia permissions revoke 1
  ✔ Règle #1 révoquée
```

### `apollia task resume <id>`

```bash
$ apollia task list --pending-approval
  ID        AGENT              DEPUIS   PROMPT
  t-042     invoice-router     14min    Où ranger la facture Acme Corp ?

$ apollia task resume t-042 --approve --input "frais-bureau"
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
apollia run research-director "..." --wait || {
  echo "Tâche échouée (code: $?)"
  apollia audit --last 1
}
```

---

## Aide intégrée

`apollia` sans arguments affiche un résumé. Pas besoin de mémoriser `--help` pour commencer :

```
Apollia OS v0.1.0, runtime d'agents IA autonomes souverains

DÉMARRAGE RAPIDE
  apollia start                       Démarrer le runtime
  apollia agent install <agent.py>    Déployer un agent
  apollia run <agent> "<tâche>"       Lancer une tâche
  apollia status                      Vue d'ensemble

TOUTES LES COMMANDES
  start, stop, restart, status, run, health
  agent       list | install | uninstall | info | logs | validate
  task        list | status | result | cancel | retry | resume
  tools       list | enable | disable | config | reload | describe
  permissions list | revoke | audit
  memory      inspect | list | clear | export | import
  audit       list | stats | export
  secrets     list | set | unset
  llm         list | add | remove | set-default
  trigger     list | add | enable | disable | fire
  notify      test | list
  stt         transcribe | status

DÉVELOPPEMENT (sous-module Python)
  python -m apollia inspect <agent.py>
  python -m apollia new <name> --type <worker|conversational|react|orchestrated>

FLAGS GLOBAUX : --json, -q/--quiet, -v/--verbose, --debug, --no-color
```

---

## ADRs

- `ADR-008` : Pattern noun-verb pour la CLI
- `ADR-018` : Bootstrap CLI sans Supervisor

*(ADRs disponibles prochainement, cf. l'encadré "ADRs et wiki" en introduction.)*
