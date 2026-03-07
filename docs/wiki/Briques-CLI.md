# Apollia CLI — Interface d'Administration et de Debug

> *La CLI est la première impression d'Apollia OS. Elle doit être utilisable par un admin PME non-développeur et scriptable par un ingénieur DevOps.*

---

## 1. Principes de design

### 1.1 Pattern de commandes

**Pattern retenu : `apollia-os <noun> <verb>`** — cohérent avec `docker container create`, `kubectl get pods`. Choix structurel maintenu sans exception dans toute la CLI.

**Règles issues des meilleures pratiques CLI 2025 :**
- Sorties humaines par défaut (tableaux colorés), machine avec `--json` sur toutes les commandes
- TTY auto-détecté : couleurs et progress bars uniquement si stdout est un terminal
- Flags courts (un caractère) réservés aux opérations fréquentes uniquement
- Précédence : flags > variables d'environnement > `apollia.toml` > défauts

### 1.2 Niveaux de profondeur

```
Niveau 1 (admin PME) : start · stop · status · run
Niveau 2 (développeur) : agent · task · tools · memory · audit
Niveau 3 (debug) : --verbose · --debug · --raw
```

---

## 2. Niveau 1 — Commandes quotidiennes

### `apollia-os start`

```bash
$ apollia-os start
  Apollia OS v0.1.0 démarrage...
  ✔ EventBus         prêt
  ✔ AgentRegistry    prêt
  ✔ Tool Registry    6 outils chargés
  ✔ Memory Engine    prêt (FTS5, embedding désactivé)
  ✔ TaskRouter       prêt
  ✔ APIServer        écoute sur /tmp/apollia.sock · localhost:7771
  ─────────────────────────────────────────────────
  ✔ Runtime prêt en 0.8s

$ apollia-os start --foreground      # Logs en direct, pas de daemonisation
$ apollia-os start --config ./dev.toml
$ apollia-os start --log-level debug

# Si déjà démarré :
  ⚠ Apollia OS tourne déjà (PID 12345)
  → apollia-os status pour voir l'état
```

### `apollia-os stop`

```bash
$ apollia-os stop
  Apollia OS arrêt en cours...
  → Drain des tâches en cours (timeout: 30s)
    ✔ agent "devis-generator"  — tâche t-001 terminée (2.1s)
    ✔ agent "crm-qualifier"    — aucune tâche active
  → Fermeture connexions MCP · Flush SQLite
  ✔ Arrêt propre en 3.2s

$ apollia-os stop --force            # Annule les tâches en cours immédiatement
$ apollia-os restart
```

### `apollia-os status`

La commande la plus utilisée au quotidien.

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

  TÂCHES EN COURS (1)
  ───────────────────────────────────────────────────────────
  ID          AGENT             DURÉE    ÉTAPE
  t-008       devis-generator   14s      3/10

  SANTÉ OUTILS
  ───────────────────────────────────────────────────────────
  bash_executor    ✔  python_executor  ✔  file_io       ✔
  http_client      ✔  mcp_erp_acme    ✗  (retry dans 18s)

  → apollia-os agent logs rapport-hebdo  pour diagnostiquer

$ apollia-os status --json
```

### `apollia-os run`

```bash
$ apollia-os run devis-generator "Génère un devis pour Dupont SA, 5 jours, 850€/jour"
  → Tâche t-009 soumise à devis-generator
  ⠿ Exécution en cours...
  ✔ Terminé en 3.1s (4 étapes, 3 appels outils)

  RÉSULTAT
  Devis #043 généré : /workspace/devis/devis-043.json
  Montant : 4 250 € HT · 5 100 € TTC

$ apollia-os run devis-generator --input ./tâche.json
$ apollia-os run devis-generator "..." --stream    # Streaming en temps réel
$ apollia-os run devis-generator "..." --no-wait   # Fire and forget
  → Tâche t-010 soumise. Suivi : apollia-os task status t-010
$ apollia-os run devis-generator "..." --timeout 60 --wait
```

---

## 3. Niveau 2 — Gestion complète

### `apollia-os agent <verb>`

```bash
# Lister
$ apollia-os agent list

# Démarrer
$ apollia-os agent start ./agents/mon-agent.py
  → Validation du manifest...
  ✔ Manifest valide (outils : file_io, python_executor)
  → Résolution des outils...
  ✔ Agent "mon-agent" démarré

# Arrêter / Redémarrer
$ apollia-os agent stop devis-generator
$ apollia-os agent restart devis-generator
  ✔ Agent "devis-generator" redémarré en 0.4s

# Détail complet
$ apollia-os agent info devis-generator
  Nom         : devis-generator
  Fichier     : ./agents/devis.py
  Version     : 1.2.0
  État        : actif depuis 2h14m
  Namespace   : crm-dupont
  Outils      : file_io, python_executor, http_client
  Concurrence : 1 tâche max
  Step budget : 10 steps, 20 tool_calls, 5min timeout
  Tâches      : 47 terminées, 0 échouées
  Mémoire     : 847 épisodes, 234 clés sémantiques (2.3MB)

# Logs
$ apollia-os agent logs devis-generator
$ apollia-os agent logs devis-generator --last 50
$ apollia-os agent logs devis-generator --follow

# Valider sans démarrer
$ apollia-os agent validate ./agents/mon-agent.py
  ✔ Manifest valide
  ✔ Outils requis disponibles : file_io, python_executor
  ⚠ Outil optionnel absent : mcp_erp_acme (démarrera en DEGRADED)
  ✔ Namespace "crm-dupont" accessible
```

### `apollia-os task <verb>`

```bash
# Lister
$ apollia-os task list
  ID          AGENT              ÉTAT        DURÉE    IL Y A
  t-009       devis-generator    ✔ terminé   3.1s     2min
  t-007       crm-qualifier      ✗ échoué    1.0s     12min

$ apollia-os task list --agent devis-generator --status failed --last 10

# Statut
$ apollia-os task status t-009
$ apollia-os task result t-009 --json

# Annuler / Relancer
$ apollia-os task cancel t-008
  ⚠ Confirmer annulation de t-008 ? [o/N] o
$ apollia-os task retry t-007
  → Nouvelle tâche t-011 soumise (retry de t-007)

# Reprendre une tâche input_required
$ apollia-os task resume t-012 --input "Approuvé, procéder à l'envoi"
```

### `apollia-os tools <verb>`

```bash
$ apollia-os tools list
$ apollia-os tools describe bash_executor
$ apollia-os tools register ./tools/erp_connector.py
$ apollia-os tools unregister erp_acme
$ apollia-os tools test bash_executor --input '{"command": "echo hello"}'
  stdout: hello  ·  exit_code: 0  ·  duration: 12ms
$ apollia-os tools reset-circuit mcp_erp_acme
  ✔ Circuit breaker réinitialisé (→ CLOSED)
```

### `apollia-os memory <verb>`

```bash
$ apollia-os memory inspect crm-dupont
$ apollia-os memory search crm-dupont "devis refusé client"
$ apollia-os memory get crm-dupont client.dupont_sa.preferences
$ apollia-os memory forget crm-dupont client.dupont_sa.old_email
$ apollia-os memory purge crm-dupont
$ apollia-os memory export crm-dupont --format json > backup.json
$ apollia-os memory import crm-dupont backup.json
```

### `apollia-os audit`

```bash
$ apollia-os audit
  HEURE          AGENT          TÂCHE   OUTIL              DURÉE   RÉSULTAT
  10:00:03       devis-gen      t-009   file_io            12ms    ✔
  09:48:05       crm-qual       t-007   http_client        1000ms  ✗ TIMEOUT

$ apollia-os audit --agent devis-generator --since "2026-03-05 09:00"
$ apollia-os audit --json > audit-export.json

$ apollia-os audit stats
  Période    : dernières 24h
  Tâches     : 89 terminées, 3 échouées (96.7% succès)
  Temps moy  : 2.8s
  Outil +    : python_executor (34 appels)
  Outil —    : http_client (2 timeouts)
```

---

## 4. Niveau 3 — Debug

```bash
# Debug complet d'une exécution
$ apollia-os run devis-generator "..." --debug
  [DEBUG] ContextBundle construit en 45ms
  [DEBUG] Classification → MODE_DIRECT (3 tools, max_steps=10)
  [DEBUG] StepBudget: 10 steps, 20 tool_calls, 300s wall_clock
  [DEBUG] Step 1: tool_call file_io {"path": "clients/dupont.json"}
  [DEBUG] Tool response: 12ms, exit_code=0, 247 bytes
  ...

# État interne du runtime
$ apollia-os status --debug
  [DEBUG] Acteurs: EventBus(healthy) AgentRegistry(healthy) TaskRouter(healthy)
  [DEBUG] APIServer: 0 connexions actives, 127 requêtes totales
  [DEBUG] Circuit breakers: bash_executor(CLOSED,0/5) http_client(CLOSED,2/5)
  [DEBUG] Memory SQLite WAL: 0 transactions en attente

# Tester la connexion
$ apollia-os health
  ✔ Runtime actif (réponse en 2ms)
  ✔ Unix socket: /tmp/apollia.sock
  ✔ HTTP: localhost:7771
```

---

## 5. Flags globaux

```
--json          Sortie JSON sur stdout (désactive couleurs et progress)
-q, --quiet     Succès/erreur seulement — aucun détail
-v, --verbose   Détails supplémentaires
--debug         Logs internes + traces ORIA
--no-color      Désactive les couleurs (TTY auto-détecté si absent)
--socket PATH   Socket Unix alternatif (défaut: /tmp/apollia.sock)
```

---

## 6. Codes de sortie (standard Unix)

```
0   Succès
1   Erreur générale (usage, input invalide)
2   Erreur runtime (runtime non démarré, connexion refusée)
3   Tâche échouée (run --wait avec tâche en échec)
4   Timeout (--timeout dépassé)
5   Annulé par l'utilisateur (Ctrl+C)
```

Usage en script :
```bash
apollia-os run devis-generator "..." --wait || {
  echo "Tâche échouée (code: $?)"
  apollia-os audit --last 1
}
```

---

## 7. Onboarding — Premier lancement

```bash
$ apollia-os
  Apollia OS v0.1.0 — Runtime d'agents IA autonomes souverains

  DÉMARRAGE RAPIDE
    apollia-os start                      Démarrer le runtime
    apollia-os agent start <agent.py>     Déployer un agent
    apollia-os run <agent> "<tâche>"      Lancer une tâche
    apollia-os status                     Vue d'ensemble

  TOUTES LES COMMANDES
    start · stop · restart · status · run · health
    agent   list | start | stop | restart | info | logs | validate
    task    list | status | result | cancel | retry | resume
    tools   list | describe | register | unregister | test | reset-circuit
    memory  inspect | search | get | forget | purge | export | import
    audit   [list] | stats | export

  FLAGS GLOBAUX : --json · -q/--quiet · -v/--verbose · --debug · --no-color

  → apollia-os <commande> --help   aide détaillée
  → https://docs.apollia.io        documentation complète
```

---

## 8. Implémentation — Stack Rust

```toml
# Cargo.toml (apollia-cli)
[dependencies]
clap = { version = "4", features = ["derive", "color"] }
comfy-table = "7"        # Tableaux formatés
indicatif = "0.17"       # Progress bars + spinners
atty = "0.2"             # TTY detection
reqwest = { version = "0.12", features = ["json", "unix-socket"] }
tokio = { version = "1", features = ["full"] }
serde_json = "1"
colored = "2"
```

---

## 9. Décisions architecturales clés

| Décision | Justification |
|---|---|
| Pattern `noun verb` homogène | Cohérent avec docker/kubectl — pas d'apprentissage supplémentaire |
| 4 commandes de niveau 1 | Admin PME peut opérer sans connaître l'architecture |
| `--json` global | Scriptabilité totale sans compromis sur la lisibilité humaine |
| TTY auto-détection | Couleurs en terminal, texte brut dans les pipes |
| Codes de sortie standards | Intégration bash/CI sans traitement spécial |
| `apollia-os agent validate` | Fail fast avant démarrage — économise du temps de debug |
| `apollia-os memory export` | Souveraineté — l'admin peut extraire toute la mémoire |
| Onboarding sans `--help` obligatoire | `apollia-os` seul explique les commandes |
