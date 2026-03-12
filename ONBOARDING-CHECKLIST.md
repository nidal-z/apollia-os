# Apollia OS — Feuille de test manuel

Checklist complète pour valider qu'`apollia-os` fonctionne correctement de bout en bout.
Cocher chaque case au fur et à mesure. Un `[ ]` non coché en fin de session = bug à ouvrir.

**Durée estimée :** 45–60 min (sans LLM) · 90–110 min (avec LLM, inclus Bloc 16)
**Prérequis :** avoir lu `ONBOARDING.md` sections 2 et 3 avant de commencer.

---

## Légende

| Symbole | Signification |
|---|---|
| `[ ]` | À tester |
| `[x]` | Validé |
| `[~]` | Partiel / comportement inhabituel à noter |
| `[!]` | Échec — reporter en bas dans "Anomalies" |
| `⌨` | Commande à saisir dans le terminal |
| `→` | Résultat attendu |
| `📝` | Zone de note libre |

---

## Bloc 0 — Prérequis système

> Effectuer avant tout. Si un item échoue, ne pas continuer.

```
[x]  Rust installé et à jour
    ⌨  rustc --version
    →  rustc 1.75.0 ou supérieur

[x] Python 3.11+ disponible
    ⌨  python3 --version
    →  Python 3.11.x ou supérieur

[x] macOS uniquement : PYO3_PYTHON exporté
    ⌨  echo $PYO3_PYTHON
    →  /opt/homebrew/bin/python3.13 (ou chemin équivalent)
    →  Si vide : export PYO3_PYTHON=/opt/homebrew/bin/python3.13

[x] SQLite disponible
    ⌨  sqlite3 --version
    →  3.35.0 ou supérieur

[x] Git disponible (requis par apollia-reviewer)
    ⌨  git --version
    →  git version 2.x.x
```

---

## Bloc 1 — Build

```
[x] Build workspace sans erreur
    ⌨  cargo build --workspace --release 2>&1 | tail -5
    →  Finished release profile [optimized] target(s) in ...
    →  Aucune ligne ERROR

[x] Binaire apollia-os produit
    ⌨  ls -lh target/release/apollia-os
    →  Fichier présent, taille > 5 MB

[x] apollia-os répond à --version
    ⌨  ./target/release/apollia-os --version
    →  apollia-os 0.1.0

[x] apollia-os dans le PATH
    ⌨  which apollia-os
    →  Chemin vers target/release/apollia-os
    →  Si absent : export PATH="$(pwd)/target/release:$PATH"

📝 Durée de build : 8 min 11 sec
📝 Warnings clippy éventuels : Aucun
```

---

## Bloc 2 — Démarrage du runtime

> Ouvrir **deux terminaux** : Terminal A (runtime) et Terminal B (commandes).

### Terminal A — lancer le runtime

```
[x] Runtime démarre sans erreur
    ⌨  apollia-os start
    →  Affiche le résumé en 9 lignes puis la séparation "---"
    →  Dernière ligne : "* Runtime ready in X.Xs"
    →  Pas de ligne ERROR au démarrage

[x] Le résumé de démarrage est complet et correct
    →  * EventBus            ready
    →  * AgentRegistry       ready
    →  * ToolRegistry        ready (3 native tools)
    →  * LlmRouter           disabled  ← ou "backend X" si [llm] dans apollia.toml
    →  * TaskRouter          ready
    →  * TriggerEngine       ready (0 trigger(s))
    →  * PipelineEngine      disabled (no [[pipelines]] defined)  ← ou "N pipeline(s)"
    →  * APIServer           listening on /tmp/apollia.sock + localhost:7771
    →  * NotificationEngine  disabled  ← ou "N channel(s)" si [notifications] configuré
    →  -------------------------------------------------
    →  * Runtime ready in X.Xs

    ⚠  MemoryEngine n'apparaît PAS ici — ce n'est pas un acteur Tokio,
       il s'instancie à la demande pour chaque agent. C'est normal.
    ⚠  PipelineEngine "disabled" sans [[pipelines]] dans apollia.toml = comportement
       attendu, pas une erreur.

[x] Socket Unix créé
    ⌨  ls -la /tmp/apollia.sock
    →  srwxr-xr-x  ...  /tmp/apollia.sock

📝 Temps de démarrage constaté : 2.9 s
📝 Warnings au démarrage : Aucun
```

### Terminal B — vérifications post-démarrage

```
[x] Status runtime actif
    ⌨  apollia-os status
    →  Runtime  ACTIVE
    →  (ligne vide)
    →  AGENTS (0 active)
    →  NOM                  STATE
    →  (no agents registered)

[x] Status en JSON
    ⌨  apollia-os status --json
    →  JSON valide avec champ "status": "running"

[x] Endpoint health répond
    ⌨  curl -s --unix-socket /tmp/apollia.sock http://localhost/api/v1/health
    →  {"status":"ok"}

[x] Port TCP répond
    ⌨  curl -s http://localhost:7771/api/v1/health
    →  {"status":"ok"}

[x] Dashboard web accessible
    ⌨  Ouvrir http://localhost:7771/ dans un navigateur
    →  Page HTML avec interface Apollia OS
    →  Pas de page blanche, pas d'erreur 404

📝 Temps de réponse HTTP : 251 ms
```

---

## Bloc 3 — Gestion des agents

```
[x] Liste agents vide au démarrage
    ⌨  apollia-os agent list
    →  Aucun agent ou liste vide

[x] Déploiement apollia-reviewer
    ⌨  apollia-os agent start agents/apollia-reviewer.py
    →  Agent "apollia-reviewer" registered
    →  Pas d'erreur Python

[x] Agent visible dans la liste
    ⌨  apollia-os agent list
    →  apollia-reviewer   active

[x] Info agent complète
    ⌨  apollia-os agent info apollia-reviewer
    →  name: apollia-reviewer
    →  version: 1.0.0
    →  state: active
    →  tools: bash_executor, file_io
    →  execution_mode: direct
    →  memory_namespace: apollia-reviewer

[x] Info agent en JSON
    ⌨  apollia-os agent info apollia-reviewer --json
    →  JSON valide avec "agent_id", "state": "active"

[x] Arrêt agent
    ⌨  apollia-os agent stop apollia-reviewer
    →  Agent stopped

[x] Agent absent après arrêt
    ⌨  apollia-os agent list
    →  Liste vide (ou agent absent)

[x] Redéploiement après arrêt
    ⌨  apollia-os agent start agents/apollia-reviewer.py
    →  Agent redéployé sans erreur
    →  apollia-os agent list → apollia-reviewer   active

📝 Temps de déploiement : _____ s
📝 Anomalies observées : _______________________________
```

---

## Bloc 4 — Exécution de tâches (apollia-reviewer)

> Le repo courant doit avoir au moins 1 commit pour que git diff HEAD~1 retourne quelque chose.
> ⌨  git log --oneline -3  (vérifier qu'il y a au moins 2 commits)

### 4.1 Exécution synchrone

```
[x] Tâche synchrone complète
    ⌨  apollia-os run apollia-reviewer "$(pwd)"
    →  Ligne "  -> Task <uuid> submitted to apollia-reviewer"
    →  Rapport Markdown affiché commençant par "# Review —"
    →  Section "## Static analysis" présente
    →  Section "## LLM analysis" présente (même si "static only")
    →  Ligne finale "*Generated by apollia-reviewer*"
    →  Ligne "  * Completed in X.Xs"
    →  Exit code 0

[x] Fichier rapport créé
    ⌨  ls -la .apollia/reviews/review-latest.md
    →  Fichier présent, taille > 0
    ⌨  head -5 .apollia/reviews/review-latest.md
    →  Commence par "# Review —"

[x] Exit code succès
    ⌨  apollia-os run apollia-reviewer "$(pwd)"; echo "Completed in $s"
    →  Completed in 10.5s

📝 Durée d'exécution : _____ s
```

### 4.2 Exécution avec streaming

```
[x] Streaming temps réel
    ⌨  apollia-os run apollia-reviewer "$(pwd)" --stream
    →  Ligne "  -> Task <uuid> submitted to apollia-reviewer"
    →  Immédiatement après : "  ~ Running on apollia-reviewer..."
       (confirme que le stream est actif et que la tâche est prise en charge)
    →  Quelques secondes plus tard : rapport Markdown affiché dès completion
    →  Exit code 0

    ℹ  apollia-reviewer fonctionne en mode direct (exécution Python opaque).
       En mode direct, seuls les événements "started" et "completed" sont émis —
       pas d'events intermédiaires par appel outil. Les appels outils en mode
       orchestré (step_started / step_completed) apparaîtront une fois le
       ToolProxy câblé (story à venir).

    ℹ  Différence visible vs sans --stream :
       - Sans --stream  : poll toutes les 200 ms → terminal gelé → rapport d'un coup
       - Avec --stream  : "Running on apollia-reviewer..." s'affiche immédiatement,
         puis le rapport apparaît sans délai de poll dès que la tâche se termine.

📝 Délai visible entre "submitted" et "Running on..." : < 1 s
📝 Rapport affiché dès completion (pas de délai de poll) : oui / non
```

### 4.3 Exécution asynchrone (detach)

```
[x] Soumission non-bloquante
    ⌨  apollia-os run apollia-reviewer "$(pwd)" --detach
    →  Retourne immédiatement un task-id (ex: t-abc123)
    →  Prompt revient sans attendre

[x] Suivi par task-id
    ⌨  apollia-os task status <task-id-copié>
    →  status: running  (si encore en cours)
    →  puis status: completed  (après quelques secondes)

[x] Liste des tâches
    ⌨  apollia-os task list
    →  Tâche visible avec son statut

[x] Tâche en JSON
    ⌨  apollia-os task status <task-id> --json
    →  JSON valide avec "status", "task_id"

📝 task-id obtenu : _______________________________
```

### 4.4 Annulation de tâche

```
[x] Annulation d'une tâche en cours
    ⌨  apollia-os run apollia-reviewer "$(pwd)" --detach
    →  Récupérer le task-id
    ⌨  apollia-os task cancel <task-id>
    →  Tâche annulée
    ⌨  apollia-os task status <task-id>
    →  status: canceled

[x] Exit code annulation
    ⌨  apollia-os task cancel <task-id-inexistant>; echo "Exit: $?"
    →  Exit: 2 ou message d'erreur cohérent (pas de panic)
```

### 4.5 Input invalide

```
[x] Agent avec chemin invalide
    ⌨  apollia-os run apollia-reviewer "/chemin/inexistant"
    →  Exit code != 0  (3 = tâche échouée ou 2 = erreur)
    →  Message d'erreur lisible, pas de panic Rust

[x] Agent inexistant
    ⌨  apollia-os run agent-inexistant "input"
    →  Erreur "agent not found" ou similaire
    →  Exit code != 0

📝 Messages d'erreur observés : _______________________________
```

---

## Bloc 5 — Outils natifs

### 5.1 Liste et descriptions

```
[ ] Lister les outils
    ⌨  apollia-os tools list
    →  3 outils : file_io, bash_executor, python_executor

[ ] Décrire file_io
    ⌨  apollia-os tools describe file_io
    →  Description + schéma d'entrée (actions: read/write/delete/list/glob)

[ ] Décrire bash_executor
    ⌨  apollia-os tools describe bash_executor
    →  Description + paramètres (command, timeout_seconds)

[ ] Lister en JSON
    ⌨  apollia-os tools list --json
    →  JSON valide, tableau de 3 outils
```

### 5.2 Audit trail (trace des appels outils)

```
[ ] Audit peuplé après exécution
    ⌨  apollia-os audit list --limit 10
    →  Entrées présentes (issues des git + bash calls de apollia-reviewer)
    →  Champs : outil, agent, timestamp, résultat

[ ] Stats d'audit
    ⌨  apollia-os audit stats
    →  Compteurs par outil

[ ] Audit en JSON
    ⌨  apollia-os audit list --json
    →  JSON valide

📝 Nombre d'entrées audit : _____
📝 Outils les plus appelés : _______________________________
```

---

## Bloc 6 — Mémoire

```
[ ] Mémoire enregistrée par apollia-reviewer
    ⌨  apollia-os memory inspect apollia-reviewer
    →  Entrées présentes (review enregistrée en épisodique)
    →  Namespace isolé "apollia-reviewer"

[ ] Fichier SQLite créé
    ⌨  ls -lh ~/.apollia/memory.db
    →  Fichier présent, taille > 0

📝 Nombre d'entrées mémoire : _____
```

---

## Bloc 7 — API HTTP directe

> Valider que l'API REST répond correctement (indépendamment de la CLI).

```
[ ] GET /api/v1/health
    ⌨  curl -s http://localhost:7771/api/v1/health
    →  {"status":"ok"}

[ ] GET /api/v1/agents
    ⌨  curl -s http://localhost:7771/api/v1/agents
    →  JSON valide, liste les agents actifs

[ ] POST /api/v1/tasks — champ agent_id correct
    ⌨  curl -s -X POST http://localhost:7771/api/v1/tasks \
         -H "Content-Type: application/json" \
         -d '{"agent_id":"apollia-reviewer","input":{"parts":[{"type":"text","text":"'"$(pwd)"'"}]}}'
    →  202 Accepted
    →  JSON avec "task_id"
    →  PAS de 422 Unprocessable Entity

[ ] GET /api/v1/tasks/:id
    ⌨  Utiliser le task-id récupéré ci-dessus
    ⌨  curl -s http://localhost:7771/api/v1/tasks/<task-id>
    →  JSON avec "status" (running ou completed)

[ ] GET /api/v1/tools
    ⌨  curl -s http://localhost:7771/api/v1/tools
    →  JSON valide, 3 outils

[ ] Format d'erreur (requête invalide)
    ⌨  curl -s -X POST http://localhost:7771/api/v1/tasks \
         -H "Content-Type: application/json" \
         -d '{"champ_invalide":"valeur"}'
    →  4xx avec {"error":"..."} — UN SEUL champ "error" (pas "message")
    →  Pas de 500

📝 task-id API obtenu : _______________________________
```

---

## Bloc 8 — LLM (optionnel — passer si non configuré)

> Prérequis : avoir une clé ANTHROPIC_API_KEY ou OPENAI_API_KEY, et apollia.toml configuré (voir ONBOARDING.md §7).

```
[ ] Status LLM (sans backend)
    ⌨  apollia-os llm status
    →  0 backend(s) configuré(s)  OU  liste des backends si configuré

--- Effectuer les tests suivants UNIQUEMENT si un backend est configuré ---

[ ] Ping LLM
    ⌨  apollia-os llm ping
    →  Backend répond, latence affichée

[ ] Chat direct
    ⌨  apollia-os llm chat "Réponds en une phrase : qu'est-ce qu'un acteur Tokio ?"
    →  Réponse textuelle cohérente

[ ] Chat en JSON
    ⌨  apollia-os llm chat "hello" --json
    →  JSON valide avec champ "content"

[ ] apollia-reviewer Tier 1 (avec LLM)
    ⌨  apollia-os run apollia-reviewer "$(pwd)"
    →  Section "## LLM analysis" contient une vraie analyse (pas "static only")
    →  Ligne finale : "*Generated by apollia-reviewer (Tier 1 — LLM enabled)*"

📝 Backend utilisé : _______________________________
📝 Latence ping : _____ ms
📝 Qualité de l'analyse LLM (1-5) : _____
```

---

## Bloc 9 — Triggers (optionnel — passer si non configuré)

> Prérequis : avoir une section `[[triggers]]` dans apollia.toml.

```
[ ] Liste des triggers
    ⌨  apollia-os trigger list
    →  Triggers configurés visibles avec état enabled/disabled

[ ] Statut d'un trigger
    ⌨  apollia-os trigger status <id-trigger>
    →  Informations du trigger + dernière exécution

[ ] Déclenchement manuel
    ⌨  apollia-os trigger fire <id-trigger>
    →  Trigger déclenché
    ⌨  apollia-os task list
    →  Nouvelle tâche créée par le trigger

[ ] Hot-reload
    ⌨  Modifier apollia.toml (ex: changer le schedule)
    ⌨  apollia-os trigger reload
    →  Rechargement sans redémarrer le runtime
    ⌨  apollia-os trigger status <id>
    →  Nouveau schedule reflété

📝 Trigger testé : _______________________________
```

---

## Bloc 10 — Pipelines (optionnel — passer si non configuré)

> Prérequis : avoir une section `[[pipelines]]` dans apollia.toml avec au moins apollia-reviewer comme step.

```
[ ] Liste des pipelines
    ⌨  apollia-os pipeline list
    →  Pipelines configurés visibles

[ ] Exécution synchrone
    ⌨  apollia-os pipeline run <id-pipeline> "$(pwd)"
    →  Exécution step par step
    →  Résultat final affiché

[ ] Exécution asynchrone
    ⌨  apollia-os pipeline run <id-pipeline> "$(pwd)" --detach
    →  run-id retourné immédiatement

[ ] Statut run
    ⌨  apollia-os pipeline status <run-id>
    →  Statut du run + état de chaque step

[ ] Historique des runs
    ⌨  apollia-os pipeline runs <id-pipeline>
    →  Liste des runs passés

[ ] Fichier SQLite pipelines
    ⌨  ls -lh ~/.apollia/pipelines.db
    →  Fichier présent après un run

📝 Pipeline testé : _______________________________
📝 Durée d'exécution : _____ s
```

---

## Bloc 11 — HITL (optionnel)

> Prérequis : un agent avec `tools_requiring_approval` configuré ou agent qui appelle `ctx.tools.call` sur un outil requérant approbation.

```
[ ] Tâche en attente d'approbation
    ⌨  apollia-os task list --pending-approval
    →  Tâches en état "input_required" visibles

[ ] Approbation depuis CLI
    ⌨  apollia-os task resume <task-id> --approve
    →  Tâche reprend
    ⌨  apollia-os task status <task-id>
    →  status: completed

[ ] Rejet depuis CLI
    ⌨  apollia-os task resume <task-id> --reject
    →  Tâche reprend avec rejet
    ⌨  apollia-os task status <task-id>
    →  status: completed ou failed selon logique agent

[ ] HITL visible dans le dashboard
    ⌨  Ouvrir http://localhost:7771/
    →  Section "Approbations" affiche la tâche en attente
    →  Boutons Approuver/Rejeter fonctionnels
```

---

## Bloc 12 — Notifications (optionnel)

> Prérequis : avoir une section `[notifications]` dans apollia.toml.

```
[ ] Liste des canaux
    ⌨  apollia-os notify list
    →  Canaux configurés avec leur état

[ ] Test des canaux
    ⌨  apollia-os notify test
    →  Envoi de notification test
    →  Notification reçue sur le canal configuré (desktop/webhook)

[ ] Logs de notifications
    ⌨  apollia-os notify logs --last 10
    →  Entrées de log visibles

📝 Canal testé : _______________________________
📝 Notification bien reçue : oui / non
```

---

## Bloc 13 — Arrêt graceful

```
[ ] Arrêt propre sans tâche en cours
    ⌨  apollia-os stop  (dans Terminal B)
    →  "Draining tasks..."
    →  "Runtime stopped."
    →  Terminal A (apollia-os start) se termine proprement
    →  Exit code 0

[ ] Socket supprimé après arrêt
    ⌨  ls /tmp/apollia.sock 2>&1
    →  "No such file or directory"

[ ] Arrêt avec tâche en cours (drain)
    ⌨  Relancer : apollia-os start
    ⌨  Soumettre une tâche longue : apollia-os run apollia-reviewer "$(pwd)" --detach
    ⌨  Immédiatement : apollia-os stop
    →  "Draining tasks... (1 active)"
    →  Runtime attend que la tâche se termine (max 30s)
    →  Puis : "Runtime stopped."

[ ] Double Ctrl+C force l'arrêt
    ⌨  Relancer : apollia-os start
    ⌨  Dans Terminal A : Ctrl+C une fois → "Graceful shutdown initiated"
    ⌨  Ctrl+C une deuxième fois → arrêt immédiat (exit 1)

📝 Temps de drain observé : _____ s
```

---

## Bloc 14 — Résilience (tests de robustesse)

```
[ ] Runtime résiste à un agent inexistant
    ⌨  apollia-os agent start /chemin/inexistant.py
    →  Erreur claire "file not found" ou similaire
    →  Le runtime continue de tourner (apollia-os status → ACTIVE)

[ ] Runtime résiste à un agent Python malformé
    ⌨  echo "syntaxe( invalide" > /tmp/bad_agent.py
    ⌨  apollia-os agent start /tmp/bad_agent.py
    →  Erreur de chargement Python
    →  Le runtime continue de tourner

[ ] Runtime résiste à une tâche vers agent arrêté
    ⌨  apollia-os agent stop apollia-reviewer
    ⌨  apollia-os run apollia-reviewer "$(pwd)"
    →  Erreur "agent not found" ou "agent not active"
    →  Exit code != 0 (2 ou 3)
    →  Pas de panic Rust

[ ] CLI répond sans runtime
    ⌨  apollia-os stop  (si runtime tourne encore)
    ⌨  apollia-os status
    →  Message d'erreur "connection refused" ou "runtime not running"
    →  Exit code != 0
    →  Pas de panic Rust, pas de stack trace

📝 Comportements inattendus observés : _______________________________
```

---

## Bloc 15 — Commandes just

```
[ ] just build
    ⌨  just build
    →  cargo build --workspace terminé sans erreur

[ ] just lint
    ⌨  just lint
    →  cargo fmt --check + cargo clippy sans erreur

[ ] just test
    ⌨  just test
    →  cargo test --workspace — tous les tests passent
    →  Nombre de tests : _____ / _____ passés

[ ] just test-python (macOS : PYO3_PYTHON requis)
    ⌨  just test-python
    →  Tests avec features python-tests passent
    →  Nombre de tests Python : _____ / _____ passés

📝 Tests échoués (noms) : _______________________________
```

---

## Bloc 16 — Agents ReAct et mode orchestré *(optionnel — LLM requis)*

> Ce bloc nécessite un backend LLM configuré (voir Bloc 8).
> Sans LLM : seul le test de dégradation (16.3) est possible.

```
[ ] Prérequis : LLM backend configuré et pingable
    ⌨  apollia-os llm ping
    →  Pong ! Latence : X ms
    →  Si échec, configurer un backend dans apollia.toml (voir Bloc 8)

[ ] Déployer react-agent
    ⌨  apollia-os agent start agents/react_agent.py
    →  Agent registered: react-agent
    →  Status: Running

[ ] Vérifier le manifest de react-agent
    ⌨  apollia-os agent info react-agent
    →  name: react-agent
    →  execution_mode: direct
    →  tools_required: bash_executor, file_io

[ ] 16.1 — Exécution ReAct synchrone (LLM requis)
    ⌨  apollia-os run react-agent "How many .rs files are in $(pwd)/crates?"
    →  Rapport commence par "# ReAct —"
    →  Section "## Loop (Thought / Action / Observe)" présente
    →  Au moins un "Thought:" et un "Action:" visibles
    →  Section "## Answer" présente avec une réponse
    →  Termine par "*Generated by react-agent (Tier 1 — LLM enabled)*"
    →  Exit code 0

[ ] 16.2 — Streaming ReAct (--stream)
    ⌨  apollia-os run react-agent "List top-level directories in $(pwd)" --stream
    →  Première ligne : "~ Running on react-agent..."  (apparaît immédiatement)
    →  Events SSE arrivent progressivement (pas d'un seul coup)
    →  Rapport final identique à 16.1

[ ] 16.3 — Dégradation sans LLM
    ⌨  (temporairement, commenter le backend LLM dans apollia.toml ou utiliser un agent isolé)
    ⌨  apollia-os run react-agent "echo hello"
    →  Rapport contient "⚠ LLM backend not configured"
    →  Contient "static fallback only"
    →  Exit code 0 (dégradation gracieuse, pas d'erreur fatale)

    📝 LLM disponible lors du test : Oui / Non
    📝 Modèle utilisé : _______________________________

[ ] Déployer orchestrated-agent
    ⌨  apollia-os agent start agents/orchestrated_agent.py
    →  Agent registered: orchestrated-agent
    →  Status: Running

[ ] Vérifier le manifest de orchestrated-agent
    ⌨  apollia-os agent info orchestrated-agent
    →  name: orchestrated-agent
    →  execution_mode: orchestrated
    →  system_prompt présent dans le manifest

[ ] 16.4 — Exécution mode orchestré (LLM requis)
    ⌨  apollia-os run orchestrated-agent "Summarise the crate structure of $(pwd)/crates"
    →  ORIA génère un plan automatiquement
    →  Steps s'exécutent (bash_executor appelé pour chaque step)
    →  Rapport final retourné avec statut "completed"
    →  Exit code 0

[ ] 16.5 — Inspecter le plan ORIA généré
    ⌨  TASK_ID=$(apollia-os run orchestrated-agent "count .rs files in $(pwd)" --json | jq -r '.task_id')
    ⌨  apollia-os task inspect $TASK_ID
    →  Plan JSON visible avec les steps
    →  Chaque step a un statut (Completed / Failed)
    →  Steps exécutés dans l'ordre topologique

[ ] 16.6 — Streaming mode orchestré
    ⌨  apollia-os run orchestrated-agent "Count lines in the largest .rs file in $(pwd)" --stream
    →  "~ Running on orchestrated-agent..."  (immédiat)
    →  Events plan_generated / step_started / step_completed apparaissent
    →  Rapport final avec réponse
    →  Exit code 0

[ ] Les deux agents coexistent sans interférence
    ⌨  apollia-os agent list
    →  react-agent        Running
    →  orchestrated-agent Running
    →  (+ autres agents déployés précédemment)

[ ] Arrêt propre des deux agents
    ⌨  apollia-os agent stop react-agent
    ⌨  apollia-os agent stop orchestrated-agent
    →  Status: Stopped (ou Stopping puis Stopped)

📝 Nombre de steps générés par ORIA : _______
📝 Modèle utilisé : _______________________________
📝 Comportements inattendus : _______________________________
```

---

## Récapitulatif

| Bloc | Libellé | Statut | Notes |
|------|---------|--------|-------|
| 0 | Prérequis système | | |
| 1 | Build | | |
| 2 | Démarrage runtime | | |
| 3 | Gestion des agents | | |
| 4 | Exécution de tâches | | |
| 5 | Outils natifs | | |
| 6 | Mémoire | | |
| 7 | API HTTP directe | | |
| 8 | LLM *(optionnel)* | | |
| 9 | Triggers *(optionnel)* | | |
| 10 | Pipelines *(optionnel)* | | |
| 11 | HITL *(optionnel)* | | |
| 12 | Notifications *(optionnel)* | | |
| 13 | Arrêt graceful | | |
| 14 | Résilience | | |
| 15 | Commandes just | | |
| 16 | Agents ReAct / orchestré *(optionnel)* | | |

**Score :** _____ / _____ items validés
**Date du test :** _______________
**Testeur :** _______________
**Version :** `apollia-os --version` → _______________
**OS :** _______________

---

## Anomalies détectées

> Reporter ici tout comportement inattendu. Une ligne par anomalie.

| # | Bloc | Commande | Résultat obtenu | Résultat attendu |
|---|------|---------|----------------|-----------------|
| 1 | | | | |
| 2 | | | | |
| 3 | | | | |

---

## Notes libres

```
_________________________________________________________________
_________________________________________________________________
_________________________________________________________________
_________________________________________________________________
```
