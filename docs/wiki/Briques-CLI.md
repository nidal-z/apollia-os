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
  ✔ LlmRouter        2 backends prêts (local · anthropic)
  ✔ TaskRouter       prêt
  ✔ APIServer        écoute sur /tmp/apollia.sock · localhost:7771
  ─────────────────────────────────────────────────
  ✔ Runtime prêt en 1.2s

# Sans LLM configuré :
$ apollia-os start
  Apollia OS v0.1.0 démarrage...
  ✔ EventBus         prêt
  ✔ AgentRegistry    prêt
  ✔ Tool Registry    6 outils chargés
  ✔ Memory Engine    prêt (FTS5, embedding désactivé)
  ⚠ LlmRouter        aucun backend configuré — ctx.llm sera None pour tous les agents
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

En Mode Orchestré, `apollia-os run` affiche le plan généré, la progression step par step en temps réel, et les notices de replanification.

```bash
# Mode Direct (comportement inchangé)
$ apollia-os run devis-generator "Génère un devis pour Dupont SA, 5 jours, 850€/jour"

# Mode Orchestré — affichage plan + steps temps réel
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

# Créer un agent depuis un template SDK (Sprint 21)
$ apollia-os agent new mon-agent --type react
  ✔ SDK disponible (apollia 0.1.0)
  ✔ Nom disponible
  → Création de l'agent dans ~/.apollia/agents/mon-agent/
  ✔ Agent créé :
    - mon_agent_agent.py
    - test_mon_agent_agent.py

$ apollia-os agent new assistant --type conversational
$ apollia-os agent new analyseur --type orchestrated

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

# Lister les tâches en attente d'approbation humaine (HITL)
$ apollia-os task list --pending-approval
  ID        AGENT              DEPUIS   PROMPT
  t-042     devis-generator    14min    Confirmer l'envoi du devis à dupont@sa.fr ?
  t-038     crm-qualifier      2h       Autoriser la mise à jour du CRM ?

# Approuver une tâche suspendue → reprend l'exécution
$ apollia-os task resume t-042 --approve
  ✔ Tâche t-042 reprise (approuvée)

# Rejeter une tâche suspendue → AIPResult::failed("REJECTED")
$ apollia-os task resume t-042 --reject --reason "Budget insuffisant"
  ✔ Tâche t-042 terminée (rejetée : Budget insuffisant)

# Inspecter le plan d'exécution d'une tâche orchestrée (Sprint 10)
# Lit directement ~/.apollia/plans.db — ne nécessite pas un runtime démarré
$ apollia-os task inspect t-abc123

  Tâche       : t-abc123
  Agent       : analyse-contrat
  Mode        : orchestré
  Statut      : completed
  Créé        : 2026-03-09T14:32:00Z
  Replanif.   : 0/2

  Plan d'exécution :
  ✔ [s1]  Lire le fichier contrat  → file_io
  ✔ [s2]  Extraire les clauses  → llm
  ✔ [s3]  Produire le rapport  → llm

$ apollia-os task inspect t-abc123 --json
# → JSON complet avec outputs par step, durées, erreurs
```

> Pour les tâches Mode Direct (pas de plan SQLite), `task inspect` répond :
> `La tâche t-xxx n'a pas de plan d'exécution (mode direct ou plan non persisté).`

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

Inspection et gestion de la mémoire des agents. Les commandes `inspect`, `list`, et `clear` opèrent directement sur SQLite sans nécessiter un runtime démarré.

```bash
# Inspecter l'état d'un namespace mémoire
$ apollia-os memory inspect crm-dupont
Namespace   : crm-dupont
Fichier     : ~/.apollia/memory/crm-dupont.db (1.2 MB)
Episodes    : 42
Semantique  : 18 clés
Procedures  : 3

# Lister tous les namespaces mémoire présents sur le disque *(Sprint 28)*
$ apollia-os memory list
NAMESPACE         EPISODIC  SEMANTIC  PROCEDURAL      SIZE
agent-crm               42        18           3   1.2 MB
agent-mail               7         0           0  48.0 KB

$ apollia-os memory list --agent agent-crm   # filtrer par agent
$ apollia-os memory list --json              # sortie JSON

# Vider la mémoire d'un agent *(Sprint 28)*
$ apollia-os memory clear --agent agent-crm              # prompt interactif
$ apollia-os memory clear --agent agent-crm --confirm    # sans prompt
$ apollia-os memory clear --agent agent-crm --type episodic --confirm  # type: episodic | semantic | procedural | all
3 entree(s) supprimee(s) (episodic).

# Commandes héritées (lecture fine)
$ apollia-os memory search crm-dupont "devis refusé client"
$ apollia-os memory get crm-dupont client.dupont_sa.preferences
$ apollia-os memory forget crm-dupont client.dupont_sa.old_email
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

### `apollia-os llm <verb>`

Diagnostiquer et tester les backends LLM. Nécessite un runtime démarré.

```bash
# État de tous les backends configurés
$ apollia-os llm status
  BACKENDS LLM
  ────────────────────────────────────────────────────
  NOM           TYPE        MODÈLE                   ÉTAT
  local         embedded    llama3.2-3B-q4_K_M.gguf  ✔ prêt
  anthropic     cloud       claude-haiku-4-5          ✔ prêt
  gpt-4o-mini   cloud       gpt-4o-mini               ✔ prêt (défaut)

$ apollia-os llm status --json

# Mesurer la latence d'un backend
$ apollia-os llm ping
  ✔ gpt-4o-mini (défaut) — 234ms
$ apollia-os llm ping anthropic
  ✔ anthropic — 187ms
$ apollia-os llm ping local
  ✔ local (embedded) — 1 243ms

# Si la clé API est absente :
$ apollia-os llm ping anthropic
  ✗ anthropic — ANTHROPIC_API_KEY absent (exit code 2)

# Envoyer un prompt direct et afficher la réponse
$ apollia-os llm chat "Résume les avantages du local-first en 3 points"
  1. Pas de latence réseau — réponse instantanée
  2. Confidentialité totale des données
  3. Fonctionnement hors ligne garanti

$ apollia-os llm chat "test" --backend anthropic
$ apollia-os llm chat "test" --json
  {"content": "...", "usage": {"prompt_tokens": 12, "completion_tokens": 42}, "latency_ms": 187}
```

### `apollia-os model <verb>`

Gestion des fichiers modèles locaux `.gguf`. Ne nécessite **pas** un runtime démarré — lecture directe du filesystem.

```bash
# Lister les modèles disponibles dans ~/.apollia/models/
$ apollia-os model list
  MODÈLES LOCAUX (~/.apollia/models/)
  ────────────────────────────────────────────
  NOM                              TAILLE
  llama3.2-3B-q4_K_M.gguf         2.0 GB
  mistral-7b-instruct-q4.gguf     4.1 GB

$ apollia-os model list --json
  {"models": [{"name": "llama3.2-3B-q4_K_M.gguf", "size_bytes": 2097152000}]}

# Si le répertoire n'existe pas encore :
$ apollia-os model list
  Aucun modèle trouvé dans ~/.apollia/models/
  → Téléchargez un modèle .gguf et placez-le dans ~/.apollia/models/
```

### `apollia-os onboard [--topic <topic>]`

Lance l'onboarding conversationnel ou re-déclenche un onboarding partiel sur un domaine spécifique. Voir le [Guide Onboarding](Agents-Onboarding-Guide.md) pour le détail complet.

**Fichier** : `crates/apollia-cli/src/commands/onboard.rs`

```bash
# Onboarding complet — conversation naturelle sur les 5 domaines
$ apollia-os onboard
  -> Onboarding task abc123 submitted
  ... conversation ...
  * Onboarding completed in 45.2s

# Re-onboarding ciblé sur un domaine
$ apollia-os onboard --topic preferences
  -> Onboarding task def456 submitted (topic: preferences)

# Sortie JSON
$ apollia-os onboard --topic tools --json
```

**Topics valides** : `identity`, `preferences`, `tools`, `domain`, `agents`.

Un topic invalide retourne une erreur :
```
Error: invalid topic 'invalid', valid topics: identity, preferences, tools, domain, agents
```

### `apollia-os stt <verb>`

Moteur Speech-to-Text embarqué. Nécessite un runtime démarré (sauf `model list`).

```bash
# Statut du moteur STT
$ apollia-os stt status
  STT ENGINE
  ────────────────────────────────────────────
  Statut      : ✔ actif
  Backend     : whisper-cpp
  Modèle      : whisper-large-v3-fr-q5_0.bin
  Metal       : ✔ activé
  Langue      : fr

$ apollia-os stt status --json

# Transcrire un fichier audio
$ apollia-os stt transcribe fichier.wav
  Transcription (3.2s audio, 1.1s traitement) :
  Bonjour, je voudrais un devis pour cinq jours de prestation.

$ apollia-os stt transcribe fichier.wav --output résultat.txt
$ apollia-os stt transcribe fichier.wav --json

# Historique des transcriptions
$ apollia-os stt transcriptions list
  ID               SOURCE   LANGUE   DURÉE    DATE
  a1b2c3d4e5f6     🎙️ hotkey fr       3.2s     il y a 5min
  f6e5d4c3b2a1     📁 file   fr       12.1s    il y a 1h

$ apollia-os stt transcriptions list --limit 5 --json

# Lister les modèles disponibles
$ apollia-os stt model list
  MODÈLES STT (~/.apollia/models/)
  ────────────────────────────────────────────
  NOM                                    TAILLE
  whisper-large-v3-fr-q5_0.bin           956 MB

# Télécharger un modèle depuis HuggingFace
$ apollia-os stt model download bofenghuang/whisper-large-v3-french
  Téléchargement : whisper-large-v3-fr-q5_0.bin
  ████████████████████████████████ 956.2 MB / 956.2 MB (100%)
  ✔ Modèle enregistré dans ~/.apollia/models/whisper-large-v3-fr-q5_0.bin

# Si STT désactivé :
$ apollia-os stt status
  ✗ STT désactivé (stt.enabled = false dans apollia.toml)
```

### `apollia-os notify <verb>`

Gérer les notifications et tester les canaux configurés. Nécessite un runtime démarré.

```bash
$ apollia-os notify test
  CANAUX DE NOTIFICATION
  ────────────────────────────────────────────────────
  NOM              TYPE       ÉTAT
  desktop          desktop    ✔ envoyé (42ms)
  slack-webhook    webhook    ✔ envoyé (187ms)

# Si un canal est indisponible
$ apollia-os notify test
  desktop          desktop    ✗ indisponible (libnotify absent)
  slack-webhook    webhook    ✗ erreur (connection refused)

# Lister les canaux configurés
$ apollia-os notify list
  CANAUX CONFIGURÉS
  desktop          desktop    activé
  slack-webhook    webhook    activé — events: task.input_required, task.failed

# Historique des 20 dernières notifications
$ apollia-os notify logs
$ apollia-os notify logs --last 50
  HEURE              EVENT                TÂCHE      CANAUX
  14:32:01           task.input_required  t-042      desktop
  14:28:15           task.failed          t-038      desktop, slack-webhook
```

### `apollia-os pipeline <verb>`

Orchestration multi-agent via pipelines déclaratifs. Nécessite un runtime démarré.

```bash
# Lister les pipelines déclarés dans apollia.toml
$ apollia-os pipeline list
  PIPELINES CONFIGURÉS
  ──────────────────────────────────────────────────────
  ID                      STEPS   DESCRIPTION
  traitement-facture      4       OCR → validation → comptabilisation → archivage
  rapport-hebdomadaire    2       Rapport PME automatique

# Déclencher un pipeline manuellement
$ apollia-os pipeline run traitement-facture --input "facture-acme.pdf"
  ✔ Pipeline run démarré : r-3f7a2b9c

# Déclencher et suivre la progression (polling par défaut)
$ apollia-os pipeline run traitement-facture --input "facture-acme.pdf"
  ⟿ [ocr] running
  ✔ [ocr] completed
  ⟿ [validation] running
  ✔ [validation] completed
  ⟿ [comptabilite] running
  ⏸ [comptabilite] waiting_approval

  ✔ Pipeline terminé en 1m23s — 4/4 steps (0 skipped)

# Déclencher sans attendre la fin (fire-and-forget)
$ apollia-os pipeline run traitement-facture --input "facture-acme.pdf" --detach
  ● traitement-facture › démarré (run r-3f7a2b9c)

# Voir l'historique des runs d'un pipeline
$ apollia-os pipeline runs traitement-facture
  RUN ID       STATUT        DÉMARRÉ                DURÉE
  r-3f7a2b9c   Completed     2026-03-10 10:01:32    1m23s
  r-2e6b1a8b   Failed        2026-03-09 14:32:01    0m08s   (step: validation)

# Inspecter l'état détaillé d'un run
$ apollia-os pipeline status r-3f7a2b9c
  Pipeline : traitement-facture
  Run      : r-3f7a2b9c
  Statut   : Completed — 2026-03-10 10:02:55

  STEP              STATUT      DURÉE   TÂCHE
  ocr               Completed   13.2s   t-0021
  validation        Completed    1.8s   t-0022
  comptabilisation  Completed   13.0s   t-0023
  archivage         Completed    2.3s   t-0024

# Format JSON pour scripts
$ apollia-os pipeline status r-3f7a2b9c --json
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
    start · stop · restart · status · run · health · onboard
    agent    list | start | stop | restart | info | logs | validate | new
    task     list | status | result | cancel | retry | resume | inspect
    pipeline list | run | runs | status
    tools    list | describe | register | unregister | test | reset-circuit
    memory   inspect | search | get | forget | purge | export | import
    audit    [list] | stats | export
    notify   test | list | logs
    stt      status | transcribe | transcriptions list | model list | model download

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
