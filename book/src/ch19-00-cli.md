# La CLI complète

La CLI `apollia-os` suit le pattern `<noun> <verb>` — cohérent avec `docker container run` ou `kubectl get pods`. Une fois le pattern appris, toutes les commandes sont prévisibles.

> **Référence technique :** [Briques-CLI](https://github.com/nidal-z/apollia-os/wiki/Briques-CLI) — toutes les commandes, sous-commandes, flags, et exemples de sortie.

---

## Trois niveaux de profondeur

```
Niveau 1 (admin, usage quotidien) : start · stop · status · run
Niveau 2 (développeur)            : agent · task · tools · permissions · memory · audit · llm · pipeline · trigger · notify · stt
Niveau 3 (debug)                  : --verbose · --debug · --raw
```

Le Niveau 1 suffit pour opérer un système en production. Le Niveau 2 sert au développement et au diagnostic. Le Niveau 3 expose l'état interne des acteurs Tokio.

---

## Exemples emblématiques

### `apollia-os start`

```bash
$ apollia-os start
  Apollia OS v0.1.0 démarrage...
  ✔ EventBus         prêt
  ✔ AgentRegistry    prêt
  ✔ Tool Registry    6 outils chargés
  ✔ Memory Engine    prêt (FTS5, embedding désactivé)
  ✔ LlmRouter        2 backends prêts (local · anthropic)
  ✔ TaskRouter       prêt
  ✔ APIServer        écoute sur /tmp/apollia.sock · localhost:7771
  ─────────────────────────────────────────────────
  ✔ Runtime prêt en 1.2s
```

Si aucun backend LLM n'est configuré, le runtime démarre quand même avec un avertissement — `ctx.llm` sera `None` dans les agents.

### `apollia-os run`

```bash
# Mode Direct — résultat direct
$ apollia-os run devis-generator "Génère un devis pour Dupont SA, 5 jours, 850€/jour"
  → Tâche t-009 soumise
  ⠿ Exécution en cours...
  ✔ Terminé en 3.1s (4 étapes, 3 appels outils)

  RÉSULTAT
  Devis #043 généré : /workspace/devis/devis-043.json
  Montant : 4 250 € HT · 5 100 € TTC

# Mode Orchestré — affiche le plan et la progression step par step
$ apollia-os run analyse-contrat "Analyse ce contrat et extrait les clauses clés"

  Plan généré (3 étapes) :
  ├── [s1] Lire le fichier contrat  → file_io
  ├── [s2] Extraire les clauses  → llm  (attend s1)
  └── [s3] Produire le rapport  → llm  (attend s2)

  ● [1/3] Lire le fichier contrat...
  ✔ [1/3] (complété)  0.1s
  ● [2/3] Extraire les clauses...
  ✔ [2/3] (complété)  1.8s
  ● [3/3] Produire le rapport...
  ✔ [3/3] (complété)  2.1s

  ✔ Tâche complétée en 4.1s
```

Flags utiles : `--stream` (streaming token par token), `--no-wait` (fire and forget), `--timeout 60`.

### `apollia-os status`

```bash
$ apollia-os status

  Apollia OS v0.1.0  ●  Running depuis 2h14m  ·  PID 12345

  AGENTS (3 actifs)
  ───────────────────────────────────────────────────────────
  NOM                  ÉTAT       TÂCHES   DERNIÈRE ACTIVITÉ
  devis-generator      ● actif    47       il y a 3min
  crm-qualifier        ● actif    12       il y a 1h
  rapport-hebdo        ○ dégradé  5        il y a 30min
    └ ⚠ mcp_erp_acme indisponible (circuit ouvert)

  SANTÉ OUTILS
  ───────────────────────────────────────────────────────────
  bash_executor    ✔  python_executor  ✔  file_io       ✔
  http_client      ✔  mcp_erp_acme    ✗  (retry dans 18s)
```

### `apollia-os permissions` — règles persistées

Le moteur de permissions enregistre automatiquement vos choix "Toujours autoriser" dans `governance.db`. La sous-commande `permissions` vous permet de les inspecter et de les révoquer.

```bash
# Voir toutes les règles persistées (project + global)
$ apollia-os permissions list
  ID    OUTIL             PORTÉE    ARGUMENT               EXPIRATION     CRÉÉ LE
  1     file_write        project   /tmp/ @ /mon/projet    permanente     2026-04-25
  2     web_search        global    (tous)                 permanente     2026-04-22
  (les règles 'session' vivent en mémoire du runtime — non listables depuis la CLI)

# Révoquer une règle par son identifiant
$ apollia-os permissions revoke 1
  Confirmer la révocation de la règle #1 (file_write) ? [o/N] o
  ✔ Règle #1 révoquée

# Consulter l'historique des décisions automatiques
$ apollia-os permissions audit --tool web_search --limit 10
```

La sous-commande opère directement sur `governance.db` — **pas besoin d'un runtime démarré**.

> **Référence technique :** [Briques-CLI §permissions](https://github.com/nidal-z/apollia-os/wiki/Briques-CLI) — tous les flags `list()`, `revoke`, `audit`, portées, et sorties JSON.

### `apollia-os task resume` — HITL

```bash
# Voir les tâches en attente d'approbation humaine
$ apollia-os task list --pending-approval
  ID        AGENT              DEPUIS   PROMPT
  t-042     devis-generator    14min    Confirmer l'envoi du devis à dupont@sa.fr ?
  t-038     crm-qualifier      2h       Autoriser la mise à jour du CRM ?

# Approuver ou rejeter
$ apollia-os task resume t-042 --approve
  ✔ Tâche t-042 reprise (approuvée)

$ apollia-os task resume t-042 --reject --reason "Budget insuffisant"
  ✔ Tâche t-042 terminée (rejetée : Budget insuffisant)
```

---

## Flags globaux

| Flag | Description |
|---|---|
| `--json` | Sortie JSON sur stdout — désactive couleurs et progress bars |
| `-q, --quiet` | Succès/erreur seulement, aucun détail |
| `-v, --verbose` | Détails supplémentaires |
| `--debug` | Logs internes + traces ORIA |
| `--no-color` | Désactive les couleurs (TTY auto-détecté si absent) |
| `--socket PATH` | Socket Unix alternatif (défaut: `/tmp/apollia.sock`) |

Les sorties humaines (tableaux, progress bars, couleurs) sont activées uniquement si stdout est un terminal — désactivées automatiquement dans les pipes et les scripts CI.

---

## Codes de sortie

```
0   Succès
1   Erreur générale (usage, input invalide)
2   Erreur runtime (runtime non démarré, connexion refusée)
3   Tâche échouée (run --wait avec tâche en échec)
4   Timeout (--timeout dépassé)
5   Annulé par l'utilisateur (Ctrl+C)
```

Usage en script bash :

```bash
apollia-os run devis-generator "..." --wait || {
  echo "Tâche échouée (code: $?)"
  apollia-os audit --last 1
}
```

---

## Aide intégrée

`apollia-os` sans arguments affiche un résumé des commandes disponibles — pas besoin de mémoriser `--help` pour commencer :

```
Apollia OS v0.1.0 — Runtime d'agents IA autonomes souverains

DÉMARRAGE RAPIDE
  apollia-os start                      Démarrer le runtime
  apollia-os agent start <agent.py>     Déployer un agent
  apollia-os run <agent> "<tâche>"      Lancer une tâche
  apollia-os status                     Vue d'ensemble

TOUTES LES COMMANDES
  start · stop · restart · status · run · health · onboard
  agent    list | start | stop | restart | info | logs | validate | new
  task     list | status | result | cancel | retry | resume | inspect
  pipeline    list | run | runs | status
  tools       list | enable | disable | config | reload | credentials | describe
  permissions list | revoke | audit
  memory      inspect | search | get | forget | purge | export | import
  audit       [list] | stats | export
  notify      test | list | logs
  stt         status | transcribe | transcriptions list | model list | model download

FLAGS GLOBAUX : --json · -q/--quiet · -v/--verbose · --debug · --no-color

→ apollia-os <commande> --help   aide détaillée
```

---

> **Référence complète :** [Briques-CLI](https://github.com/nidal-z/apollia-os/wiki/Briques-CLI) — toutes les sous-commandes `agent`, `task`, `tools`, `permissions`, `memory`, `audit`, `llm`, `pipeline`, `trigger`, `notify`, `stt` avec leurs flags, sorties, et exemples détaillés.
