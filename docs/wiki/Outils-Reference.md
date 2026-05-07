# Outils natifs — Référence rapide

> Référence des outils natifs Apollia OS. Version.

---

## Vue d'ensemble

| Outil | Catégorie | Feature flag | Description courte |
|---|---|---|---|
| `bash_executor` | Shell | — | Exécution shell dans namespace isolé |
| `python_executor` | Python | — | Code Python dans venv isolé |
| `file_read` | Filesystem | — | Lire un fichier (offset/limit) |
| `file_write` | Filesystem | — | Écrire un fichier |
| `file_edit` | Filesystem | — | Remplacement chirurgical de texte |
| `file_list` | Filesystem | — | Lister les entrées d'un répertoire |
| `file_glob` | Recherche | — | Recherche par glob pattern |
| `file_grep` | Recherche | — | Recherche par regex avec contexte |
| `http_fetch` | Réseau | `http` | Requête HTTP avec allowlist |
| `web_search` | Réseau | `web-search` | Recherche web (DuckDuckGo / Brave) |
| `web_read` | Réseau | `web-read` | Extraction texte d'une URL publique |
| `memory_search` | Mémoire | `memory-search` | Recherche FTS5/BM25 en mémoire locale |
| `permission_rule_add` | Gouvernance | — | Ajouter une règle de permission dans `governance.db` (HITL) |
| `permission_rule_remove` | Gouvernance | — | Supprimer une règle par ID (HITL) |
| `permission_rule_list` | Gouvernance | — | Lister les règles (lecture seule, filtre par `created_by`/`scope`) |

---

## Outils Filesystem

### `file_read`

Lit le contenu d'un fichier texte. Supporte la lecture partielle via offset et limit.

**Input**

```json
{
  "path":   "String          — chemin absolu, relatif au sandbox, ou avec préfixe `~`/`~/` (expansé vers $HOME)",
  "offset": "u32 (optionnel) — première ligne à lire (1-based)",
  "limit":  "u32 (optionnel) — nombre maximum de lignes à retourner"
}
```

**Output**

```json
{
  "content":     "String — contenu du fichier avec numéros de ligne préfixés",
  "total_lines": "u32   — nombre total de lignes dans le fichier",
  "truncated":   "bool  — true si le fichier a été tronqué par limit"
}
```

**Erreurs**

| Code | Description |
|---|---|
| `not_found` | Le fichier n'existe pas |
| `sandbox_violation` | Le chemin sort du sandbox autorisé |
| `binary_file` | Le fichier est binaire et ne peut pas être lu |
| `io_error` | Erreur I/O générique |

**Exemple**

```json
// Input
{ "path": "src/main.rs", "offset": 10, "limit": 30 }

// Output
{
  "content": "10\tfn main() {\n11\t    ...\n",
  "total_lines": 120,
  "truncated": false
}
```

---

### `file_write`

Crée ou remplace un fichier avec le contenu fourni.

**Input**

```json
{
  "path":    "String — chemin du fichier à écrire (préfixe `~`/`~/` expansé vers $HOME)",
  "content": "String — contenu complet à écrire"
}
```

**Output**

```json
{
  "bytes_written": "u64    — nombre d'octets écrits",
  "path":          "String — chemin du fichier écrit (normalisé)"
}
```

**Erreurs**

| Code | Description |
|---|---|
| `sandbox_violation` | Le chemin sort du sandbox autorisé |
| `io_error` | Erreur I/O générique (permissions, disque plein…) |

**Exemple**

```json
// Input
{ "path": "output/result.txt", "content": "Hello, Apollia!\n" }

// Output
{ "bytes_written": 16, "path": "output/result.txt" }
```

---

### `file_edit`

Effectue un remplacement exact et chirurgical dans un fichier existant. L'outil refuse de modifier si `old_str` est ambigu ou absent.

**Input**

```json
{
  "path":    "String — chemin du fichier à modifier",
  "old_str": "String — chaîne exacte à remplacer (doit être unique dans le fichier)",
  "new_str": "String — chaîne de remplacement"
}
```

**Output**

```json
{
  "replaced": "bool   — true si le remplacement a eu lieu",
  "path":     "String — chemin du fichier modifié"
}
```

**Erreurs**

| Code | Description |
|---|---|
| `not_found` | Le fichier n'existe pas |
| `sandbox_violation` | Le chemin sort du sandbox autorisé |
| `not_unique` | `old_str` correspond à plusieurs occurrences dans le fichier |
| `not_present` | `old_str` n'a été trouvé nulle part dans le fichier |

**Exemple**

```json
// Input
{
  "path":    "config.toml",
  "old_str": "log_level = \"info\"",
  "new_str": "log_level = \"debug\""
}

// Output
{ "replaced": true, "path": "config.toml" }
```

---

### `file_list`

Liste les entrées d'un répertoire, avec traversée optionnelle en profondeur.

**Input**

```json
{
  "path":  "String          — chemin du répertoire à lister",
  "depth": "u32 (optionnel) — profondeur de traversée (1 = entrées directes uniquement)"
}
```

**Output**

```json
{
  "entries": [
    {
      "name":     "String        — nom de l'entrée",
      "is_dir":   "bool          — true si répertoire",
      "size":     "u64 (optionnel) — taille en octets (fichiers uniquement)",
      "modified": "String (optionnel) — date de modification ISO 8601"
    }
  ],
  "count": "u32 — nombre total d'entrées retournées"
}
```

**Erreurs**

| Code | Description |
|---|---|
| `not_found` | Le répertoire n'existe pas |
| `sandbox_violation` | Le chemin sort du sandbox autorisé |
| `not_a_directory` | Le chemin pointe vers un fichier, pas un répertoire |

**Exemple**

```json
// Input
{ "path": "src/", "depth": 1 }

// Output
{
  "entries": [
    { "name": "main.rs", "is_dir": false, "size": 4096, "modified": "2026-03-29T10:00:00Z" },
    { "name": "lib.rs",  "is_dir": false, "size": 2048, "modified": "2026-03-28T18:30:00Z" }
  ],
  "count": 2
}
```

---

### `file_glob`

Recherche des fichiers par glob pattern. Retourne les chemins correspondants triés par date de modification.

**Input**

```json
{
  "pattern": "String          — glob pattern (ex: \"**/*.rs\", \"src/**/*.toml\")",
  "path":    "String (optionnel) — répertoire racine de la recherche (défaut: \".\")"
}
```

**Output**

```json
{
  "matches": ["String"] ,
  "count":   "u32 — nombre de correspondances"
}
```

**Erreurs**

| Code | Description |
|---|---|
| `sandbox_violation` | Le chemin racine sort du sandbox autorisé |
| `invalid_pattern` | Le glob pattern est syntaxiquement invalide |

**Exemple**

```json
// Input
{ "pattern": "**/*.rs", "path": "crates/apollia-tools" }

// Output
{
  "matches": ["crates/apollia-tools/src/lib.rs", "crates/apollia-tools/src/registry.rs"],
  "count": 2
}
```

---

### `file_grep`

Recherche par expression régulière dans les fichiers, avec contexte optionnel autour de chaque correspondance. Plafonné à 1000 résultats.

**Input**

```json
{
  "pattern":       "String          — expression régulière (syntaxe Rust regex)",
  "path":          "String (optionnel) — répertoire ou fichier cible (défaut: \".\")",
  "glob":          "String (optionnel) — filtre de fichiers par glob (ex: \"*.rs\")",
  "context_lines": "u32 (optionnel) — nombre de lignes de contexte avant/après chaque correspondance"
}
```

**Output**

```json
{
  "matches": [
    {
      "file":           "String          — chemin du fichier",
      "line":           "u32             — numéro de ligne (1-based)",
      "content":        "String          — contenu de la ligne correspondante",
      "context_before": "[String] (optionnel) — lignes avant la correspondance",
      "context_after":  "[String] (optionnel) — lignes après la correspondance"
    }
  ],
  "count": "u32  — nombre de correspondances retournées",
  "capped": "bool — true si les résultats ont été tronqués à 1000"
}
```

**Erreurs**

| Code | Description |
|---|---|
| `sandbox_violation` | Le chemin sort du sandbox autorisé |
| `invalid_regex` | L'expression régulière est syntaxiquement invalide |

**Exemple**

```json
// Input
{ "pattern": "fn run\\(", "path": "crates/", "glob": "*.rs", "context_lines": 2 }

// Output
{
  "matches": [
    {
      "file": "crates/apollia-oria/src/agent.rs",
      "line": 42,
      "content": "    pub async fn run(&self) -> Result<(), OriaError> {",
      "context_before": ["    /// Démarre la boucle principale de l'agent.", "    #[tracing::instrument]"],
      "context_after":  ["        loop {", "            self.step().await?;"]
    }
  ],
  "count": 1,
  "capped": false
}
```

---

## Outils Shell et Exécution

### `bash_executor`

Exécute une commande shell dans un namespace Linux isolé (réseau, PID, mount). Le processus est tué automatiquement à l'expiration du timeout.

**Input**

```json
{
  "command":     "String          — commande shell à exécuter",
  "timeout":     "u32             — timeout en secondes (obligatoire)",
  "working_dir": "String (optionnel) — répertoire de travail (défaut: sandbox root)"
}
```

**Output**

```json
{
  "stdout":      "String — sortie standard complète",
  "stderr":      "String — sortie d'erreur complète",
  "exit_code":   "i32    — code de retour du processus",
  "duration_ms": "u64    — durée d'exécution en millisecondes"
}
```

**Exemple**

```json
// Input
{ "command": "cargo test -p apollia-tools --quiet", "timeout": 120 }

// Output
{
  "stdout":      "running 12 tests\n............ ok\n",
  "stderr":      "",
  "exit_code":   0,
  "duration_ms": 4321
}
```

---

### `python_executor`

Exécute du code Python arbitraire dans un venv isolé. Le module `apollia` est pré-importé avec l'interface SDK de l'agent courant.

**Input**

```json
{
  "code":            "String          — code Python à exécuter",
  "timeout_seconds": "u32 (optionnel) — timeout en secondes"
}
```

**Output**

```json
{
  "stdout":    "String — sortie standard du script",
  "stderr":    "String — sortie d'erreur du script",
  "exit_code": "i32    — code de retour de l'interpréteur"
}
```

**Exemple**

```json
// Input
{
  "code": "import apollia\nresult = apollia.memory.search('dernière erreur')\nprint(result)",
  "timeout_seconds": 30
}

// Output
{
  "stdout":    "[{'content': 'RuntimeError at step 7', 'score': 0.91}]\n",
  "stderr":    "",
  "exit_code": 0
}
```

---

## Outils Réseau

### `http_fetch`

Effectue une requête HTTP vers un hôte externe. Nécessite le feature flag `http` et que l'hôte soit présent dans l'allowlist configurée par l'opérateur. La réponse est plafonnée à 1 Mo.

**Input**

```json
{
  "url":          "String                    — URL complète de la requête",
  "method":       "String (optionnel)        — méthode HTTP : GET | POST (défaut: GET)",
  "headers":      "Map<String,String> (optionnel) — en-têtes HTTP additionnels",
  "body":         "String (optionnel)        — corps de la requête (pour POST)",
  "timeout_secs": "u64 (optionnel)           — timeout en secondes (défaut: 30)"
}
```

**Output**

```json
{
  "status":      "u16               — code de statut HTTP",
  "body":        "String            — corps de la réponse (max 1 Mo)",
  "headers":     "Map<String,String> — en-têtes de la réponse",
  "duration_ms": "u64               — durée totale de la requête en millisecondes"
}
```

**Erreurs**

| Code | Description |
|---|---|
| `host_not_allowed` | L'hôte n'est pas dans l'allowlist de l'opérateur |
| `no_allowlist` | Aucune allowlist HTTP n'est configurée pour cet agent |
| `ssrf_blocked` | URL vers hôte privé refusée (loopback, RFC 1918, link-local, metadata cloud, domaine interne) |
| `invalid_url` | L'URL est malformée ou ne contient pas de hôte |
| `request_failed` | La requête a échoué (DNS, TLS, connexion refusée…) |
| `response_too_large` | La réponse dépasse la limite de 1 Mo |
| `timeout` | La requête a dépassé le timeout configuré |

**Exemple**

```json
// Input
{
  "url":    "https://api.example.com/data",
  "method": "POST",
  "headers": { "Content-Type": "application/json" },
  "body":   "{\"query\": \"status\"}",
  "timeout_secs": 10
}

// Output
{
  "status":      200,
  "body":        "{\"status\": \"ok\"}",
  "headers":     { "content-type": "application/json" },
  "duration_ms": 243
}
```

---

### `web_search` *(feature flag `web-search`)*

Effectue une recherche web et retourne une liste de résultats structurés. Utilise DuckDuckGo par défaut (zero-config), ou Brave Search si une clé API est disponible. Le backend actif est sélectionné selon `[tools.web_search]` dans `apollia.toml`.

**Input**

```json
{
  "query":       "String          — requête de recherche",
  "max_results": "u32 (optionnel) — nombre maximum de résultats (défaut: 10, max: 20)",
  "region":      "String (optionnel) — région au format BCP-47 (ex: \"fr-fr\", \"en-us\")",
  "time_range":  "String (optionnel) — filtre temporel : \"day\" | \"week\" | \"month\" | \"year\""
}
```

**Output**

```json
{
  "results": [
    {
      "title":       "String — titre de la page",
      "url":         "String — URL canonique",
      "description": "String — extrait de description"
    }
  ],
  "count":         "u32    — nombre de résultats retournés",
  "backend_used":  "String — backend effectivement utilisé : \"duckduckgo\" | \"brave\"",
  "duration_ms":   "u64    — durée totale de la requête"
}
```

**Erreurs**

| Code | Description |
|---|---|
| `no_backends` | Aucun backend opérationnel (configuration invalide) |
| `timeout` | La requête a dépassé le timeout configuré |
| `rate_limited` | Trop de requêtes (DuckDuckGo 429 ou Brave quota dépassé) |
| `request_failed` | Erreur réseau (DNS, TLS, connexion refusée…) |

**Configuration opérateur**

Le backend, les timeouts et le nombre de résultats sont configurables dans `[tools.web_search]` (voir [Config-apollia-toml — §tools.web_search](./Config-apollia-toml#toolsweb_search)). La clé Brave Search peut être stockée dans le credential store chiffré (`apollia-os tools credentials set web_search brave.api_key <clé>`) ou dans la variable d'environnement `BRAVE_SEARCH_API_KEY`.

**Exemple**

```json
// Input
{ "query": "apollia os rust agent", "max_results": 5, "region": "en-us" }

// Output
{
  "results": [
    {
      "title":       "Apollia OS — Local-first agent runtime",
      "url":         "https://github.com/apollia-os/apollia",
      "description": "Rust runtime for autonomous AI agents..."
    }
  ],
  "count":        1,
  "backend_used": "duckduckgo",
  "duration_ms":  312
}
```

---

### `web_read` *(feature flag `web-read`)*

Récupère une URL publique et en extrait le contenu textuel (article, documentation, page web). Applique un garde anti-SSRF par défaut : les URL à destination d'hôtes privés sont rejetées.

**Input**

```json
{
  "url":       "String          — URL complète à lire (schéma https:// ou http://)",
  "max_chars": "u32 (optionnel) — longueur maximale du texte extrait (défaut: valeur runtime)"
}
```

**Output**

```json
{
  "url":         "String — URL finale après redirections",
  "title":       "String — titre de la page (balise <title> ou premier <h1>)",
  "text":        "String — contenu textuel extrait (HTML strippé)",
  "is_truncated": "bool  — true si le texte a été tronqué par max_chars",
  "duration_ms": "u64   — durée totale de la requête"
}
```

**Erreurs**

| Code | Description |
|---|---|
| `ssrf_blocked` | URL à destination d'un hôte privé ou loopback (garde anti-SSRF) |
| `invalid_url` | URL malformée ou schéma non supporté |
| `invalid_content_type` | Le serveur retourne un type MIME non textuel (binaire, vidéo…) |
| `response_too_large` | La réponse dépasse `max_response_kb` configuré |
| `timeout` | La requête a dépassé le timeout configuré |
| `request_failed` | Erreur réseau (DNS, TLS, connexion refusée…) |

**Configuration opérateur**

Timeout, taille maximale et activation du garde SSRF sont configurables dans `[tools.web_read]` (voir [Config-apollia-toml — §tools.web_read](./Config-apollia-toml#toolsweb_read)). `ssrf_guard = false` ne doit être utilisé qu'en lab isolé — jamais en production.

**Exemple**

```json
// Input
{ "url": "https://docs.rs/tokio/latest/tokio/", "max_chars": 5000 }

// Output
{
  "url":          "https://docs.rs/tokio/latest/tokio/",
  "title":        "tokio — Rust — Docs.rs",
  "text":         "Tokio is an asynchronous runtime for the Rust programming language...",
  "is_truncated": true,
  "duration_ms":  891
}
```

---

## Outils Mémoire

### `memory_search`

Recherche en mémoire locale par full-text search (FTS5/BM25). Accès restreint au namespace de l'agent courant par défaut. Nécessite le feature flag `memory-search`.

**Input**

```json
{
  "query":     "String          — requête de recherche (texte libre, opérateurs FTS5 supportés)",
  "namespace": "String (optionnel) — namespace mémoire cible (défaut: namespace de l'agent courant)",
  "limit":     "u32 (optionnel) — nombre maximum de résultats (défaut: 10, max: 50)",
  "source":    "String (optionnel) — filtre par source : \"episodic\" | \"semantic\""
}
```

**Output**

```json
{
  "results": [
    {
      "content":    "String          — contenu du fragment mémoriel",
      "score":      "f32             — score BM25 de pertinence",
      "source":     "String          — origine : \"episodic\" | \"semantic\"",
      "key":        "String (optionnel) — clé unique du fragment",
      "created_at": "String (optionnel) — date de création ISO 8601"
    }
  ],
  "count":     "u32    — nombre de résultats retournés",
  "namespace": "String — namespace effectivement interrogé"
}
```

**Erreurs**

| Code | Description |
|---|---|
| `empty_query` | La requête est vide ou ne contient que des espaces |
| `namespace_not_allowed` | L'agent n'a pas accès au namespace demandé |
| `search_failed` | Erreur interne du moteur FTS5 |

**Exemple**

```json
// Input
{ "query": "connexion base de données erreur", "limit": 5, "source": "episodic" }

// Output
{
  "results": [
    {
      "content":    "Step 12 — SQLite connection failed: database is locked",
      "score":      0.87,
      "source":     "episodic",
      "key":        "ep_20260329_001",
      "created_at": "2026-03-29T09:12:00Z"
    }
  ],
  "count":     1,
  "namespace": "agent:my-agent"
}
```

---

---

## Outils Gouvernance (ADR-086)

Ces trois outils permettent aux agents de lire et de proposer des règles de permission dans `governance.db`. Les écritures (`add` / `remove`) passent systématiquement par le HITL standard — l'utilisateur valide chaque règle.

### `permission_rule_add`

Persiste une nouvelle règle `Allow` ou `Deny` dans `governance.db`. HITL obligatoire (ADR-082).

**Input**

```json
{
  "tool_name":    "String          — outil ciblé (ex. 'bash_executor')",
  "action":       "String          — 'allow' | 'deny'",
  "arg_prefix":   "String?         — préfixe d'argument (None = tout argument)",
  "scope":        "String          — 'global' (défaut) | 'project' | 'agent'",
  "project_path": "String?         — requis si scope='project'",
  "agent_id":     "String?         — requis si scope='agent'",
  "expires_at":   "i64?            — Unix timestamp d'expiration (None = permanent)"
}
```

**Output**

```json
{
  "rule_id":  "i64    — identifiant SQLite de la règle créée",
  "tool_name": "String",
  "action":   "String",
  "scope":    "String"
}
```

**Erreurs**

| Code | Description |
|---|---|
| `invalid_action` | `action` n'est ni `'allow'` ni `'deny'` |
| `invalid_scope` | `scope` non reconnu |
| `missing_project_path` | `scope='project'` sans `project_path` |
| `missing_agent_id` | `scope='agent'` sans `agent_id` |
| `engine_error` | Erreur SQLite |

---

### `permission_rule_remove`

Supprime une règle par son identifiant SQLite. HITL obligatoire.

**Input**

```json
{
  "rule_id": "i64 — identifiant SQLite de la règle à supprimer"
}
```

**Output**

```json
{
  "rule_id": "i64",
  "removed": "bool — false si la règle n'existait pas"
}
```

---

### `permission_rule_list`

Liste les règles persistées. Lecture seule, pas de HITL.

**Input**

```json
{
  "tool_name":  "String? — filtre sur le nom d'outil",
  "created_by": "String? — filtre sur l'auteur (ex. 'onboarding-agent', 'user-hitl')",
  "scope":      "String? — filtre sur la portée"
}
```

**Output**

```json
{
  "rules": [
    {
      "id":           "i64",
      "tool_name":    "String",
      "arg_prefix":   "String?",
      "action":       "'allow' | 'deny'",
      "scope":        "String",
      "project_path": "String?",
      "agent_id":     "String?",
      "created_by":   "String?",
      "created_at":   "i64",
      "expires_at":   "i64?"
    }
  ],
  "count": "usize"
}
```

---

## Voir aussi

- [Briques-Tool-Registry.md](Briques-Tool-Registry.md) — spécification complète du Tool Registry : cycle de vie, sandbox, StepBudget, feature flags, et implémentation des outils natifs
- [Agents-RuntimeContext-Guide.md](Agents-RuntimeContext-Guide.md) — guide complet du RuntimeContext : comment les agents déclarent leurs besoins en outils et interagissent avec le runtime
- [Briques-Permissions.md](Briques-Permissions.md) — moteur de permissions 3 couches, gouvernance `governance.db`, ADR-086
