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

# ⚠️ Non implémentés (prévu) : --foreground, --config <path>, --log-level <level>

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

# ⚠️ Non implémentés (prévu) : stop --force, restart
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
$ apollia-os run devis-generator "..." --detach     # Fire and forget (flag réel : --detach)
  → Tâche t-010 soumise. Suivi : apollia-os task status t-010
# ⚠️ Non implémenté (prévu) : --timeout 60
```

---

## 3. Niveau 2 — Gestion complète

### `apollia-os agent <verb>`

```bash
# Lister tous les agents
$ apollia-os agent list

# Lister uniquement les agents A2A
$ apollia-os agent list --supports-a2a
  A2A-capable agents (2):
  excel-worker   v0.1.0  active  skills: read-excel, write-excel, list-sheets
  csv-worker     v0.1.0  active  skills: read-csv, analyze-csv

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

$ apollia-os agent logs rapport-hebdo
  2026-04-26T10:00:01Z [INFO] Task t-0042 started
  2026-04-26T10:00:02Z [INFO] Step 1/3 — file_io read
  2026-04-26T10:00:04Z [INFO] Step 2/3 — llm summarize
  2026-04-26T10:00:06Z [INFO] Task t-0042 completed

$ apollia-os agent logs rapport-hebdo --last 20
$ apollia-os agent logs rapport-hebdo --follow    # live stream SSE jusqu'à Ctrl+C
$ apollia-os --json agent logs rapport-hebdo       # JSON: { "logs": ["..."] }

# Créer un agent depuis un template SDK
$ apollia-os agent new mon-agent --type react
  ✔ SDK disponible (apollia 0.1.0)
  ✔ Nom disponible
  → Création de l'agent dans ~/.apollia/agents/mon-agent/
  ✔ Agent créé :
    - mon_agent_agent.py
    - test_mon_agent_agent.py

$ apollia-os agent new assistant --type conversational
$ apollia-os agent new analyseur --type orchestrated

$ apollia-os agent validate ./mon-agent.py
  ✔ Manifest valide
  Name        : mon-agent
  Version     : 0.1.0
  Required tools : file_io, python_executor
  Optional tools : http_client
  ⚠ Optional tools not checked — agent may start in DEGRADED mode if absent

# Manifest invalide → exit 1 avec erreur précise
$ apollia-os agent validate ./broken-agent.py
  Error: manifest invalid: missing required field 'name'

# JSON mode
$ apollia-os --json agent validate ./mon-agent.py
  { "valid": true, "name": "mon-agent", "version": "0.1.0", "tools_required": [...], ... }

# Installer un agent communautaire
$ apollia-os agent install agents/community/sql-worker.py
  → Validation du manifest...
  ✔ Manifest valide (name: sql-worker, version: 0.1.0)
  → Scan dangerous_tools_allowed...
  ✔ Aucun outil dangereux déclaré
  → Exécution des tests...
  ✔ 4 tests passent
  ✔ Agent "sql-worker" installé

# Installer sans exécuter les tests
$ apollia-os agent install agents/community/git-worker.py --skip-tests
  → Validation du manifest...
  ✔ Manifest valide (name: git-worker, version: 0.1.0)
  ⚠ Tests ignorés (--skip-tests)
  ✔ Agent "git-worker" installé

# Agent avec dangerous_tools_allowed → warning
$ apollia-os agent install agents/community/my-admin-agent.py
  → Validation du manifest...
  ✔ Manifest valide
  ⚠ L'agent déclare dangerous_tools_allowed: true — approbation requise
  Confirmer l'installation ? [o/N]
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

# Inspecter le plan d'exécution d'une tâche orchestrée
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

Gouvernance locale des outils natifs. Les commandes `list()`, `enable`, `disable`, `config`, `reload()` et `credentials` opèrent directement sur `governance.db` et `apollia.toml` — sans nécessiter un runtime démarré. `describe()` seule interroge le runtime via `GET /api/v1/tools/<name>`.

```bash
# État de chaque outil (actif, backend configuré, credentials)
$ apollia-os tools list
  NOM              ACTIF   BACKEND                CREDENTIALS
  web_search       ✓       DuckDuckGo (auto)      —
  web_read         ✓       dom_smoothie           —
  bash_executor    ✓       —                      —
  python_executor  ✗       —                      —

$ apollia-os tools list --json

# Activer / désactiver un outil (écrit dans governance.db)
$ apollia-os tools enable python_executor
  ✔ python_executor activé
$ apollia-os tools disable bash_executor
  ✔ bash_executor désactivé

# Lire / modifier la config d'un outil dans apollia.toml
$ apollia-os tools config get web_search
$ apollia-os tools config set web_search.backend brave
  ✔ apollia.toml mis à jour (tools.web_search.backend = "brave")
$ apollia-os tools config set web_search.brave.timeout_secs 10

# Recharger le snapshot de gouvernance et afficher l'état effectif
$ apollia-os tools reload
  ✔ Snapshot rechargé (3 outils actifs)

# Credentials chiffrées associées à un outil
$ apollia-os tools credentials list
$ apollia-os tools credentials list web_search
$ apollia-os tools credentials set web_search brave.api_key
  Valeur (masquée) : ****
  ✔ Credential stockée
$ apollia-os tools credentials delete web_search brave.api_key
  ✔ Credential supprimée
$ apollia-os tools credentials test web_search
  ✔ brave.api_key — valide (réponse 187ms)

# Descripteur d'un outil enregistré dans le runtime (nécessite runtime démarré)
$ apollia-os tools describe bash_executor
```

### `apollia-os permissions <verb>`

Gestion des règles de permissions persistées. Opère directement sur `governance.db` — **pas besoin d'un runtime démarré**. Les règles de portée `session` vivent uniquement en mémoire du daemon ; elles ne sont pas listables ni révocables depuis cette commande.

```bash
# Lister toutes les règles persistées (project + global)
$ apollia-os permissions list
  ID    OUTIL             PORTÉE    ARGUMENT               EXPIRATION     CRÉÉ LE
  1     file_write        project   /tmp/ @ /mon/projet    permanente     2026-04-25
  2     web_search        global    (tous)                 permanente     2026-04-22
  (les règles 'session' vivent en mémoire du runtime — non listables depuis la CLI)

# Filtres disponibles
$ apollia-os permissions list --scope global
$ apollia-os permissions list --scope project
$ apollia-os permissions list --tool web_search

# Sortie JSON
$ apollia-os permissions list --json
  {
    "rules": [...],
    "session_rules_visible": false,
    "note": "session rules live in the runtime memory and are not visible from the CLI"
  }

# Révoquer une règle par identifiant
$ apollia-os permissions revoke 1
  Confirmer la révocation de la règle #1 (file_write) ? [o/N] o
  ✔ Règle #1 révoquée

# Révoquer sans confirmation interactive (scripts CI)
$ apollia-os permissions revoke 1 --yes
  ✔ Règle #1 révoquée

# Révoquer toutes les règles d'une portée
$ apollia-os permissions revoke --all --scope global
  Révoquer toutes les règles global (2 règles) ? [o/N] o
  ✔ 2 règles globales révoquées

$ apollia-os permissions revoke --all --scope project --yes
  ✔ 3 règles projet révoquées

# Règle de session passée à revoke → message explicatif
$ apollia-os permissions revoke s42
  Erreur : les règles de session (préfixe 's') vivent en mémoire du runtime.
  Elles disparaissent au prochain redémarrage du daemon, ou utilisez l'app desktop pour les gérer.

# Consulter l'historique des décisions de permissions
$ apollia-os permissions audit
  HEURE            OUTIL           ARGUMENT          DÉCISION
  2026-04-25 14:32 web_search      apollia           AutoAllowedSafeList
  2026-04-25 14:28 file_write      /tmp/output.txt   NeedsApproval

$ apollia-os permissions audit --tool file_write
$ apollia-os permissions audit --limit 100
$ apollia-os permissions audit --json
```

**Portées :**

| Portée | Persistance | Révocable depuis CLI |
|---|---|---|
| `global` | `governance.db` (table `permission_rules`) | ✅ |
| `project` | `governance.db` — lié à un `project_path` | ✅ |
| `session` | Mémoire du daemon uniquement | ❌ (redémarrage ou app desktop) |

### `apollia-os memory <verb>`

Inspection et gestion de la mémoire des agents. Les commandes `inspect`, `list()`, et `clear()` opèrent directement sur SQLite sans nécessiter un runtime démarré.

```bash
# Inspecter l'état d'un namespace mémoire
$ apollia-os memory inspect crm-dupont
Namespace   : crm-dupont
Fichier     : ~/.apollia/memory/crm-dupont.db (1.2 MB)
Episodes    : 42
Semantique  : 18 clés
Procedures  : 3

# Lister tous les namespaces mémoire présents sur le disque **
$ apollia-os memory list
NAMESPACE         EPISODIC  SEMANTIC  PROCEDURAL      SIZE
agent-crm               42        18           3   1.2 MB
agent-mail               7         0           0  48.0 KB

$ apollia-os memory list --agent agent-crm   # filtrer par agent
$ apollia-os memory list --json              # sortie JSON

# Vider la mémoire d'un agent **
$ apollia-os memory clear --agent agent-crm              # prompt interactif
$ apollia-os memory clear --agent agent-crm --confirm    # sans prompt
$ apollia-os memory clear --agent agent-crm --type episodic --confirm  # type: episodic | semantic | procedural | all
3 entree(s) supprimee(s) (episodic).

# Enregistrer une procédure
$ apollia-os memory learn-procedure --namespace agent-x \
    --trigger "analyser un rapport financier" \
    --steps "1. Ouvrir le PDF, 2. Extraire le CA, 3. Générer le résumé"

# Exporter la mémoire d'un namespace
$ apollia-os memory export --namespace agent-x --output ./backup.apollia-memory

# Importer (fusionner ou remplacer)
$ apollia-os memory import --namespace agent-x --input ./backup.apollia-memory --replace

# ⚠️ Non implémentés (prévu) :
# apollia-os memory search <namespace> <query>
# apollia-os memory get <namespace> <key>
# apollia-os memory forget <namespace> <key>
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
--socket PATH   Socket Unix alternatif (défaut: /tmp/apollia.sock)
-q / --quiet    Affiche uniquement succès/erreur — aucun détail (--json prioritaire)
-v / --verbose  Affiche les détails supplémentaires (durées, steps count)
--debug         Logs internes + traces ORIA sur stderr (équivalent RUST_LOG=debug)
--no-color      Désactive les couleurs ANSI même si stdout est un TTY
```

---

## 6. Codes de sortie (standard Unix)

```
0   Succès
1   Erreur générale (usage, input invalide)
2   Erreur runtime (runtime non démarré, connexion refusée)
3   Tâche échouée (run --wait avec tâche en échec)
4   Timeout (--timeout dépassé)
5   Interrompu (Ctrl+C / SIGINT — le shutdown gracieux s'exécute, puis exit 5)
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
    tools       list | enable | disable | config | reload | credentials | describe
    permissions list | revoke | audit
    memory      inspect | list | clear | purge | learn-procedure | export | import
    audit       [list] | stats | export
    notify      test | list | logs
    stt         status | transcribe | transcriptions list | model list | model download

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

---

## 10. Nouvelles commandes et fonctionnalités

### `apollia auth`

Authentification OAuth2 PKCE auprès des providers LLM cloud avec stockage dans le keyring OS.

```bash
# Login interactif (ouvre le browser, attend le callback OAuth2)
$ apollia auth login anthropic
  → Ouverture du browser...
  ✔ Token stocké dans le keyring (anthropic)

# Statut de tous les providers
$ apollia auth status
  PROVIDER    ÉTAT              EXPIRE
  anthropic   ✔ configuré       2026-05-04T10:32:00Z
  openai      ○ non configuré   —
  vertex      ✔ configuré       2026-04-20T08:00:00Z

# Logout
$ apollia auth logout anthropic
  ✔ Token anthropic supprimé du keyring
```

Providers supportés : `anthropic`, `openai`, `vertex`.

> **Voir aussi :** [Briques Auth](./Briques-Auth.md) · [ADR-064](../adr/ADR-064-oauth2-pkce-keyring.md)

### `apollia update`

Auto-updater via GitHub Releases avec vérification SHA256 et remplacement atomique.

```bash
# Vérifier si une mise à jour est disponible
$ apollia update --check
  ✔ Nouvelle version disponible : 0.2.0 (actuel : 0.1.0)

# Installer la mise à jour
$ apollia update
  → Téléchargement apollia-os-linux-x86_64 (0.2.0)...
  → Vérification SHA256...
  → Remplacement atomique du binaire...
  ✔ Apollia OS mis à jour vers 0.2.0

# Sans prompt interactif (CI/CD)
$ apollia update --yes

# Déjà à jour
$ apollia update --check
  ✔ Apollia OS est à jour (0.2.0)
```

> **Voir aussi :** [ADR-065](../adr/ADR-065-auto-updater-distribution.md)

### `apollia mcp set-approval` / `list-pending` / `revoke-approval`

Gestion des approbations HITL pour les serveurs MCP avec `requires_approval = true`.

```bash
# Approuver un outil MCP (expire après approval_ttl_hours)
$ apollia mcp set-approval code-tools bash_exec
  ✔ Approbation enregistrée (expire : 2026-04-06T10:00:00Z)

# Lister les demandes en attente
$ apollia mcp list-pending
  ID                SERVEUR       OUTIL        DEPUIS
  3f7a2b9c          code-tools    bash_exec    14min

# Révoquer
$ apollia mcp revoke-approval code-tools bash_exec
  ✔ Approbation révoquée

# Découverte mDNS
$ apollia mcp list --discover
  Scan réseau local (3s)...
  notion-mcp  192.168.1.10  8080  [search_pages, create_page]
```

> **Voir aussi :** [Briques MCP §15](./Briques-MCP.md#15-hitl-mcp--approbations-sqlite--)

### `apollia memory export` / `import` / `purge` amélioré

```bash
# Export/import de mémoire
$ apollia memory export --agent crm-agent --output backup.apollia-mem.gz
  ✔ 42 épisodes, 18 clés sémantiques exportés → backup.apollia-mem.gz

$ apollia memory import --agent crm-agent --input backup.apollia-mem.gz
  ✔ 42 épisodes importés (mode merge)

$ apollia memory import --agent crm-agent --input backup.apollia-mem.gz --replace
  ✔ 42 épisodes importés (mode replace — namespace réinitialisé)

# Purge configurable par type
$ apollia memory purge --agent crm-agent --older-than 7 --type episodic
  5 entrée(s) épisodique(s) supprimée(s).
```

---

## 11. Nouvelles commandes et fonctionnalités

### `apollia mcp-server`

Lance Apollia en mode serveur MCP stdio. Des clients MCP externes (Claude Desktop, Cursor, VS Code) peuvent invoquer les outils natifs.

```bash
$ apollia mcp-server                # 9 outils natifs
$ apollia mcp-server --with-runtime # + outil submit_task
```

> **Voir aussi :** [Briques MCP — Mode Serveur](./Briques-MCP.md#12-mode-serveur-mcp--)

### `apollia workspace`

Inspecte le workspace courant et initialise `APOLLIA.md`.

```bash
$ apollia workspace status          # Affiche branche, fichiers modifiés, APOLLIA.md
$ apollia workspace init            # Crée APOLLIA.md avec template
$ apollia workspace init --force    # Écrase APOLLIA.md existant
$ apollia --json workspace status   # Sortie JSON : workspace_root, git_branch, ...
```

Sortie JSON :
```json
{
  "workspace_root": "/home/user/mon-projet",
  "git_branch": "main",
  "modified_files": ["src/main.rs"],
  "apollia_md_found": true,
  "file_count": 142
}
```

### `apollia run --alternatives`

Génère deux plans alternatifs et demande à l'opérateur de choisir avant l'exécution.

```bash
$ apollia run --alternatives "Migre la base de données"

--- Plan A (conservateur, température 0.3) ---
  1. Faire un backup de la BDD
  2. Appliquer les migrations
  ...

--- Plan B (exploratoire, température 0.8) ---
  1. Créer un environnement de test
  ...

Choisissez un plan [1/2] :
```

### `--allowed-tools` / `--disallowed-tools`

Restreint les outils disponibles pour une session sans modifier la config globale.

```bash
# Autoriser uniquement la lecture
$ apollia run --allowed-tools file_read,glob "Analyse le codebase"

# Interdire les outils bash et file_write
$ apollia run --disallowed-tools bash_executor,file_write "Réponds à ma question"
```

`disallowed-tools` a priorité sur `allowed-tools` en cas de conflit.

### REPL history persisté

L'historique du REPL `apollia chat` est persisté dans `~/.apollia/repl_history` (format readline, max 10 000 entrées). Flèches haut/bas et Ctrl-R fonctionnent entre les sessions.

### `/fork` — Conversation forking

```
/fork             → fork depuis maintenant (copie tout l'historique)
/fork 5           → fork depuis le message #5
/fork list        → liste toutes les sessions filles
```

```bash
$ apollia chat --list   # Affiche l'arborescence parent → enfants
```

### Slash commands custom — `APOLLIA_COMMANDS`

Définissez des commandes réutilisables dans `.apollia/commands/*.md` :

```markdown
---
description: Revue de code ciblée
args: [focus]
---

Analyse le code avec un focus sur {{focus}}.
Vérifie : correctness, performance, sécurité.
```

Usage dans le REPL :
```
/review security          → exécute le prompt avec {{focus}} = "security"
/list-commands            → liste les commandes disponibles (built-in + custom)
```

**Priorité :** `.apollia/commands/` (CWD) > `~/.apollia/commands/` (home). Hot reload via `FileTimestampCache` si les fichiers `.md` sont modifiés.

---

## 12. Commandes de résilience et cache

### `apollia-os resilience <verb>`

Expose les circuit breakers de la `ResilienceLayer` (ORIA engine). Requiert le runtime démarré.

```bash
# Lister tous les circuit breakers
$ apollia-os resilience list
  TOOL                           STATE      FAILURES COOLDOWN
  file_io                        CLOSED     0        -
  mcp_erp                        OPEN       3        15s
  python_executor                HALF_OPEN  1        -

# Détail d'un circuit breaker
$ apollia-os resilience show mcp_erp
  Tool          : mcp_erp
  State         : OPEN
  Failures      : 3
  Cooldown left : 15s

# Réinitialiser manuellement un circuit (sans redémarrer le runtime)
$ apollia-os resilience reset mcp_erp
  Circuit breaker for 'mcp_erp' reset to CLOSED

# Outil inexistant → exit 1
$ apollia-os resilience reset unknown_tool
  Error: Tool 'unknown_tool' not found in Tool Registry

# JSON mode
$ apollia-os --json resilience list
  { "circuit_breakers": [ { "tool_name": "mcp_erp", "state": "OPEN", "failure_count": 3, "cooldown_remaining_secs": 15 } ] }
```

**Endpoints runtime :** `GET /api/v1/resilience/status`, `GET /api/v1/resilience/status/{tool}`, `POST /api/v1/resilience/reset/{tool}`.

---

### `apollia-os plan-cache <verb>`

Gère le cache de plans d'exécution ORIA (`~/.apollia/plan_cache.db`). Ne requiert **pas** le runtime.

```bash
# Statistiques du cache
$ apollia-os plan-cache stats
  Plan cache statistics:
    Total entries : 47
    Cache hits    : 312
    Oldest entry  : 2026-04-19T08:00:00
    Newest entry  : 2026-04-26T09:58:12

# Vider le cache (confirmation interactive)
$ apollia-os plan-cache clear
  This will delete all cached plans. Continue? [y/N] y
  Plan cache cleared: 47 entries removed.

# Vider sans confirmation (scripts / CI)
$ apollia-os plan-cache clear --force
  Plan cache cleared: 47 entries removed.

# Expulser les entrées expirées (défaut 7 jours)
$ apollia-os plan-cache evict
  Evicted 12 entries older than 7 days.

$ apollia-os plan-cache evict --max-age-days 3
  Evicted 28 entries older than 3 days.

# JSON mode
$ apollia-os --json plan-cache stats
  { "total_entries": 47, "cache_hits": 312, "oldest_entry_at": "...", "newest_entry_at": "..." }
```

**Accès direct SQLite :** lit `~/.apollia/plan_cache.db` sans passer par le socket Unix.
