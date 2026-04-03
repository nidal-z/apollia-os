# Memory Engine — Persistance Souveraine Multi-Types

> *Architecture complète du système de mémoire : 4 types, SQLite local, FTS5 unicode, embedding optionnel.*

---

## 1. Pourquoi une architecture de mémoire dédiée

### 1.1 Le problème de la mémoire sans architecture

La plupart des agents IA sont amnésiques par design : chaque session repart de zéro. Ce n'est pas un choix délibéré — c'est l'absence d'infrastructure pour gérer la persistance.

Les solutions ad hoc courantes :
- **Variables globales Python** : disparaissent au redémarrage du processus
- **Fichiers JSON** : pas de recherche, pas de TTL, pas de types structurés
- **Bases vectorielles cloud** : dépendance externe, coût, latence, problèmes RGPD
- **SQLite artisanal** : réimplémenté différemment dans chaque projet, sans FTS

Le Memory Engine d'Apollia OS fournit une solution structurée, souveraine, et adaptée aux cas d'usage réels des agents en PME.

### 1.2 Le challenge de la surfonctionnalité

La littérature 2025 sur la mémoire des agents décrit des architectures très sophistiquées : consolidation automatique, self-evolving memory, graphes de connaissance, TTL dynamiques, extraction de faits par LLM...

Pour Apollia OS, implémenter tout cela dès le MVP serait une **erreur stratégique** :
- Complexité inutile pour la cible PME (un agent CRM n'a pas besoin de mémoire auto-évolutive)
- Appels LLM non contrôlés (résumés automatiques, extraction de faits) = coûts imprévisibles
- Comportements difficiles à debugger en production

**Décision :** Architecture à **deux niveaux d'activation** :
- v0.1 : 3 types persistants + working memory RAM + FTS5 — zero LLM requis
- v1.0 : Consolidation opt-in, embedding vectoriel opt-in, extraction de faits opt-in

---

## 2. Les 4 types de mémoire

### 2.1 Working Memory (RAM uniquement, pas de persistance)

**Rôle :** Scratchpad de l'agent pendant une tâche en cours. État temporaire, calculs intermédiaires, brouillons.

**Implémentation :** Variables Python dans le scope de `run()`. Géré directement dans `RuntimeContext`, pas dans SQLite. Disparu à la fin de la tâche.

```python
async def run(self, task: AIPTask, ctx: RuntimeContext) -> AIPResult:
    # Working memory = variables locales Python
    draft = ""
    intermediate_results = []

    # ... logique de l'agent ...
    # draft et intermediate_results disparaissent à la fin de run()
```

**Pourquoi pas SQLite :** Overhead inutile pour des données temporaires. La working memory ne doit pas persister — c'est sa définition.

### 2.2 Episodic Memory (persistante, datée)

**Rôle :** Journal des événements passés. Historique des tâches, interactions importantes, contexte temporel.

**Exemples PME :**
- "Le 14/02/2026 : Devis #142 refusé par Dupont SA, budget insuffisant"
- "Le 30/01/2026 : Devis #138 refusé par Martin SAS, délai trop court"
- "Le 05/03/2026 : Rapport hebdomadaire généré — 3 anomalies détectées"

```python
# Enregistrer un épisode
await ctx.memory.record(
    content=f"Devis #{devis_id} généré pour {client}. Montant: {montant}€",
    importance=0.8,           # 0.0 à 1.0 — filtre les recherches
    task_id=task.task_id,
    expires_in=timedelta(days=90),  # None = permanent
    metadata={"devis_id": devis_id, "client": client}
)

# Consulter l'historique
recent = await ctx.memory.history(limit=20, since=datetime(2026, 1, 1))
```

### 2.3 Semantic Memory (persistante, factuelle)

**Rôle :** Base de connaissances structurée. Faits, préférences, configurations, informations client.

**Exemples PME :**
- `client.dupont_sa.budget_max` → `"15000"`
- `client.dupont_sa.contact` → `"Marie Dupont, +33 6 12 34 56 78"`
- `client.dupont_sa.last_devis` → `{"id": 143, "montant": 5100, "date": "2026-03-05"}`
- `agent.devis_generator.default_tva` → `"0.20"`

```python
# Stocker une connaissance
await ctx.memory.remember(
    key="client.dupont_sa.budget_max",
    value=15000,
    confidence=1.0,
    source="agent:crm-qualifier"
)

# Récupérer une connaissance
budget = await ctx.memory.recall("client.dupont_sa.budget_max")

# Supprimer une connaissance obsolète
await ctx.memory.forget("client.dupont_sa.old_email")
```

### 2.4 Procedural Memory (persistante, comportementale)

**Rôle :** Workflows qui ont bien fonctionné. Séquences d'actions efficaces pour des situations récurrentes.

**Exemples PME :**
- Trigger : "devis client grand compte" → Steps : ["Vérifier SIRET", "Récupérer historique commandes", "Appliquer remise 10%", "Envoyer pour validation DG"]
- Trigger : "rapport client mensuel" → Steps : ["Consolider données CRM", "Calculer KPIs", "Générer PDF", "Notifier commercial"]

```python
# Apprendre un workflow
await ctx.memory.learn_procedure(
    trigger="devis client grand compte",
    steps=[
        "Vérifier SIRET via API officielle",
        "Récupérer historique commandes des 12 derniers mois",
        "Appliquer remise volume selon historique",
        "Soumettre pour validation commerciale si > 10K€"
    ]
)

# Récupérer un workflow
steps = await ctx.memory.recall_procedure("devis client grand compte")
```

---

## 3. Schéma SQLite

Un fichier SQLite par namespace (`~/.apollia/memory/<namespace>.db`). WAL mode activé pour les lectures concurrentes.

```sql
-- Mémoire épisodique
CREATE TABLE IF NOT EXISTS episodic_memories (
    id           TEXT PRIMARY KEY,
    namespace    TEXT NOT NULL,
    task_id      TEXT,
    agent_id     TEXT NOT NULL,
    content      TEXT NOT NULL,
    summary      TEXT,
    importance   REAL DEFAULT 0.5,
    created_at   DATETIME NOT NULL,
    expires_at   DATETIME,
    metadata     TEXT DEFAULT '{}'      -- JSON
);

-- Mémoire sémantique
CREATE TABLE IF NOT EXISTS semantic_memories (
    id           TEXT PRIMARY KEY,
    namespace    TEXT NOT NULL,
    key          TEXT NOT NULL,
    value        TEXT NOT NULL,
    source       TEXT,
    confidence   REAL DEFAULT 1.0,
    created_at   DATETIME NOT NULL,
    updated_at   DATETIME NOT NULL,
    expires_at   DATETIME,
    UNIQUE(namespace, key)
);

-- Mémoire procédurale
CREATE TABLE IF NOT EXISTS procedural_memories (
    id            TEXT PRIMARY KEY,
    namespace     TEXT NOT NULL,
    trigger       TEXT NOT NULL,
    steps         TEXT NOT NULL,        -- JSON array de strings
    success_count INTEGER DEFAULT 1,
    last_used_at  DATETIME NOT NULL,
    created_at    DATETIME NOT NULL
);

-- Index vectoriel (optionnel, nécessite sqlite-vec)
CREATE VIRTUAL TABLE IF NOT EXISTS memory_vec USING vec0(
    embedding float[384]               -- all-MiniLM-L6-v2 : 384 dims
);

CREATE TABLE IF NOT EXISTS memory_vec_index (
    vec_rowid    INTEGER PRIMARY KEY,
    source_table TEXT NOT NULL,        -- "episodic" | "semantic"
    source_id    TEXT NOT NULL
);

-- Index plein texte (toujours disponible, pas d'extension)
CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
    content,
    source_table UNINDEXED,
    source_id    UNINDEXED,
    tokenize='unicode61'               -- CRITIQUE : accentuation française
);
```

**Pourquoi `unicode61` :**
Le tokenizer `unicode61` est indispensable pour une cible PME française. Sans lui, "réunion" n'est pas retrouvé quand on cherche "reunion", "société" rate pour "societe". Les accents sont une réalité du français professionnel — les ignorer dégrade significativement la qualité de la recherche.

---

## 4. Embeddings vectoriels (opt-in)

La recherche sémantique (vectorielle) est puissante mais nécessite un modèle d'embedding. Apollia OS adopte une stratégie de dégradation gracieuse : FTS5 fonctionne toujours, les embeddings s'activent uniquement si le matériel et les modèles sont présents.

```
Niveau 1 — FTS5 uniquement (défaut, toujours disponible)
  → Recherche par mots-clés avec ranking BM25
  → Zéro dépendance supplémentaire
  → 100% fonctionnel, très bon pour les cas PME

Niveau 2 — sqlite-vec + sqlite-lembed + GGUF local (opt-in)
  → all-MiniLM-L6-v2.gguf (22MB) : zéro cloud, local-first
  → 384 dimensions, multilingue, français natif
  → Activé si le fichier GGUF est présent sur le système

Niveau 3 — sqlite-vec + Ollama (opt-in avancé)
  → nomic-embed-text-v1.5 via Ollama local (768 dims)
  → Pour les déploiements avec Ollama déjà installé
  → Meilleure qualité sémantique
```

**Règle absolue : Apollia OS ne télécharge jamais automatiquement un modèle.**

### 4.1 Activation

La feature `embeddings` est désactivée par défaut dans `apollia-memory`. Pour l'activer dans un agent ou un projet utilisant le SDK :

```toml
# Cargo.toml de l'agent ou du workspace consommateur
apollia-memory = { path = "../apollia-memory", features = ["embeddings"] }
```

La feature active la dépendance `sqlite-vec` et le moteur d'embedding local via `sqlite-lembed`. Sans elle, toutes les recherches passent par FTS5 — le binaire reste plus léger et aucune erreur n'est levée.

La configuration dans `apollia.toml` sélectionne le backend :

```toml
[memory]
embedding_strategy = "auto"     # "fts_only" | "local_gguf" | "ollama" | "auto"
gguf_model_path    = ""         # Chemin absolu vers le fichier .gguf (local_gguf)
ollama_url         = ""         # URL Ollama si embedding_strategy = "ollama"
```

En mode `auto`, le runtime détecte ce qui est disponible et utilise le niveau le plus élevé sans intervention manuelle. Si `embedding_strategy = "fts_only"`, les embeddings sont désactivés même si la feature est compilée.

### 4.2 Modèles GGUF supportés

| Modèle | Dimensions | Taille fichier | Usage recommandé |
|---|---|---|---|
| `all-MiniLM-L6-v2.gguf` | 384 | ~23 MB | Défaut — bon compromis vitesse/qualité, multilingue |
| `nomic-embed-text-v1.5.gguf` | 768 | ~137 MB | Meilleure qualité sémantique, phrases longues |

Les fichiers `.gguf` doivent être téléchargés manuellement et placés dans un répertoire accessible au runtime. Le chemin est renseigné dans `memory.gguf_model_path`. Apollia OS ne télécharge jamais un modèle automatiquement.

### 4.3 Limitations connues

- **Pas d'index HNSW** : `sqlite-vec` effectue un scan linéaire sur tous les vecteurs — la recherche est en O(n). Acceptable jusqu'à environ 100 000 vecteurs ; au-delà, les latences de recherche augmentent notablement.
- **CPU uniquement** : le calcul des embeddings via `sqlite-lembed` s'exécute sur CPU. Pas de GPU requis, mais le temps de génération d'un embedding est de l'ordre de 5-20 ms selon le modèle et le matériel.
- **Un modèle par namespace** : la table `memory_vec` est créée avec une dimension fixe lors de la première activation. Changer de modèle (ex. de 384 à 768 dims) nécessite de recréer la table vectorielle du namespace.
- **Feature additive** : activer `embeddings` sur un namespace existant indexe uniquement les nouveaux enregistrements. Les entrées antérieures ne sont pas rétroactivement vectorisées.

### 4.4 Exemple d'utilisation depuis un agent Python

La recherche sémantique est transparente pour l'agent : `ctx.memory.search()` utilise automatiquement les embeddings si disponibles, FTS5 sinon.

```python
async def run(self, task: AIPTask, ctx: RuntimeContext) -> AIPResult:
    # Recherche hybride : embeddings vectoriels si activés, FTS5 sinon
    results = await ctx.memory.search(
        "devis client refusé budget insuffisant",
        limit=5,
        sources=["episodic"],
        min_importance=0.6,
    )

    for entry in results:
        ctx.log.info(
            "memory_hit",
            score=entry.score,
            content=entry.content[:80],
            source=entry.source_table,
        )

    # Enregistrer un épisode — sera vectorisé automatiquement si embeddings activés
    await ctx.memory.record(
        content=f"Analyse terminée : {len(results)} épisodes pertinents trouvés",
        importance=0.7,
        task_id=task.task_id,
    )
```

L'agent ne distingue pas FTS5 d'un embedding vectoriel — le runtime gère la dégradation gracieuse. Si le modèle GGUF n'est pas disponible au démarrage, le runtime bascule sur FTS5 et l'indique dans les logs (`tracing::warn`).

---

## 5. MemoryInterface — API complète

```python
class MemoryInterface(Protocol):

    # ── SÉMANTIQUE ──
    async def remember(
        self, key: str, value: Any,
        *, confidence: float = 1.0,
        expires_in: timedelta | None = None,
        source: str | None = None
    ) -> None: ...

    async def recall(self, key: str) -> Any | None: ...

    async def forget(self, key: str) -> None: ...

    # ── ÉPISODIQUE ──
    async def record(
        self, content: str,
        *, importance: float = 0.5,
        task_id: str | None = None,
        expires_in: timedelta | None = None,
        metadata: dict | None = None
    ) -> str: ...  # retourne l'ID de l'épisode

    async def history(
        self, limit: int = 20,
        *, since: datetime | None = None,
        agent_id: str | None = None
    ) -> list[EpisodicEntry]: ...

    # ── RECHERCHE HYBRIDE ──
    async def search(
        self, query: str,
        *, limit: int = 10,
        sources: list[Literal["episodic", "semantic"]] | None = None,
        min_importance: float | None = None
    ) -> list[MemoryEntry]: ...

    # ── PROCÉDURALE ──
    async def learn_procedure(
        self, trigger: str, steps: list[str]
    ) -> None: ...

    async def recall_procedure(
        self, trigger: str
    ) -> list[str] | None: ...

    # ── GESTION ──
    async def stats(self) -> MemoryStats: ...
    async def purge_expired(self) -> int: ...
```

### `MemoryStats` — champs retournés par `stats()`

```python
# Obtenir les statistiques d'un namespace
s = await ctx.memory.stats()

s.namespace         # str    — nom du namespace
s.episodic_count    # int    — nombre d'entrées épisodiques
s.semantic_count    # int    — nombre de faits sémantiques
s.procedural_count  # int    — nombre de procédures
s.fts_entries       # int    — nombre d'entrées dans l'index FTS5
s.db_size_bytes     # int    — taille du fichier .db en octets
```

Exemple d'usage :

```python
async def run(self, task, ctx):
    if ctx.memory:
        s = await ctx.memory.stats()
        ctx.log.info(
            "memory_stats",
            episodic=s.episodic_count,
            semantic=s.semantic_count,
            db_kb=s.db_size_bytes // 1024,
        )
        # Alerte si la base dépasse 50 MB
        if s.db_size_bytes > 50 * 1024 * 1024:
            ctx.log.warn("memory_db_large", size_bytes=s.db_size_bytes)
```

---

## 6. Isolation par namespace et modèle d'accès

```
memory_namespace = "crm-dupont"
→ Fichier : ~/.apollia/memory/crm-dupont.db

memory_namespace = "shared"
→ Fichier : ~/.apollia/memory/shared.db (accès multi-agents)

memory_namespace = None
→ Pas de persistance (working memory uniquement)
```

Un agent peut aussi déclarer des namespaces partagés en lecture :

```python
AgentManifest(
    memory_namespace="crm-dupont",              # Namespace privé (ReadWrite)
    shared_memory_namespaces=["shared", "kb"],  # Namespaces partagés (ReadOnly)
)
```

### `MemoryAccess` — niveaux d'accès

```rust
pub enum MemoryAccess {
    ReadWrite,  // namespace_privé de l'agent (memory_namespace)
    ReadOnly,   // namespace partagé (shared_memory_namespaces)
}
```

| Niveau | Qui peut l'avoir | Opérations autorisées |
|---|---|---|
| `ReadWrite` | L'agent propriétaire du namespace (`memory_namespace`) | `record`, `remember`, `recall`, `search`, `forget`, `learn_procedure`, `stats`, `purge_expired` |
| `ReadOnly` | Tout agent ayant le namespace dans `shared_memory_namespaces` | `recall`, `search`, `history` uniquement |

La méthode `MemoryManager::access_level(namespace)` retourne `Some(ReadWrite)`, `Some(ReadOnly)`, ou `None` (namespace non autorisé).

### Modèle d'accès multi-agents — qui lit quoi, qui écrit quoi

```
Agent A
  memory_namespace          = "crm-dupont"        ← ReadWrite (A écrit ici)
  shared_memory_namespaces  = ["shared", "kb"]    ← ReadOnly (A lit seulement)

Agent B
  memory_namespace          = "shared"             ← ReadWrite (B écrit ici)
  shared_memory_namespaces  = []

Agent C (lecture seule, pas de mémoire privée)
  memory_namespace          = None
  shared_memory_namespaces  = ["shared", "crm-dupont"]  ← ReadOnly (C lit seulement)
```

**Règles immuables :**
- Un agent ne peut **jamais écrire** dans un namespace `shared_memory_namespaces` — c'est `ReadOnly`
- Tenter une écriture dans un namespace partagé lève `MemoryManagerError::ReadOnlyNamespace`
- Accéder à un namespace non déclaré lève `MemoryManagerError::NamespaceNotAllowed`
- `memory_namespace = None` → `ctx.memory` est `None` — pas d'accès mémoire du tout

**Erreurs `MemoryManager` :**

| Erreur | Cause |
|---|---|
| `NoNamespace` | `memory_namespace = None` (pas de mémoire configurée) |
| `ReadOnlyNamespace(ns)` | Tentative d'écriture dans un namespace `shared_memory_namespaces` |
| `NamespaceNotAllowed(ns)` | Accès à un namespace ni privé ni partagé |
| `OpenFailed { namespace, reason }` | Échec d'ouverture du fichier `.db` |

La concurrence SQLite est gérée par WAL (Write-Ahead Logging) : plusieurs lecteurs simultanés, un seul writer, sans blocage.

---

## 7. Configuration TTL et constantes

```toml
[memory]
episodic_ttl_days       = 90     # Épisodes purgés après 90 jours
semantic_ttl_days       = null   # Faits permanents par défaut
procedural_ttl_days     = null   # Workflows permanents par défaut
purge_on_startup        = true   # Purge au démarrage (async, non-bloquant)
step_output_max_chars   = 200    # Limite de troncature de la sortie mémorisée par step
```

La purge est **asynchrone et non-bloquante** au démarrage. Elle ne ralentit pas le démarrage de l'agent.

### Constantes de configuration

| Constante | Valeur par défaut | Description |
|---|---|---|
| `STEP_MEMORY_OUTPUT_MAX_CHARS` | `200` | Limite maximale en caractères de la sortie d'un step mémorisée dans la mémoire épisodique. Au-delà, le contenu est tronqué avec un suffixe `[truncated]`. Configurable via `memory.step_output_max_chars` dans `apollia.toml`. |

La troncature est appliquée systématiquement par le runtime lors de l'appel `ctx.memory.record()` depuis ORIA. Elle borne la croissance de la base épisodique sans heuristique de consolidation (voir [ADR-054](../adr/ADR-054-memory-episodic-consolidation.md)).

---

## 8. `MemoryStore` — référence pour contributeurs

`MemoryStore` est la couche bas niveau qui gère un fichier SQLite unique par namespace. C'est la brique sur laquelle s'appuient `EpisodicMemory`, `SemanticMemory` et `ProceduralMemory`.

### Responsabilités

- Ouverture / création du fichier `<namespace>.db`
- Activation du mode WAL (`PRAGMA journal_mode=WAL`)
- Migrations de schéma versionnées (`_schema_version`, `SCHEMA_VERSION = 1`)
- Accès à la `rusqlite::Connection` pour les backends mémoire

### Méthodes publiques

| Méthode | Description |
|---|---|
| `open(path: &Path) -> Result<Self, MemoryStoreError>` | Ouvre ou crée la base, applique les migrations |
| `schema_version() -> Result<u32, MemoryStoreError>` | Retourne la version du schéma actuel |
| `stats(namespace, db_path) -> Result<MemoryStats, MemoryStoreError>` | Statistiques du namespace |
| `delete_entry_by_id(id: &str) -> Result<bool, MemoryStoreError>` | Supprime une entrée par UUID (toutes tables + FTS5) |
| `clear_episodic(namespace) -> Result<u64, MemoryStoreError>` | Vide la mémoire épisodique du namespace |
| `clear_semantic(namespace) -> Result<u64, MemoryStoreError>` | Vide la mémoire sémantique du namespace |
| `clear_procedural(namespace) -> Result<u64, MemoryStoreError>` | Vide la mémoire procédurale du namespace |

### Migration de schéma

```
Versioning : table _schema_version (INTEGER)
Version 1 (actuelle) : créée au premier open() si absente
  → episodic_memories + index (namespace, created_at DESC)
  → semantic_memories + index (namespace, key) + contrainte UNIQUE(namespace, key)
  → procedural_memories + index (namespace, trigger_text)
  → memory_fts (FTS5 virtual table, tokenize='unicode61')
```

Les futures migrations incrémentent `SCHEMA_VERSION` et appliquent uniquement les étapes manquantes via `apply_migrations(current_version)`.

### Relation avec `MemoryManager`

```
MemoryManager
  ├── primary_namespace: Option<String>          → ReadWrite
  ├── shared_namespaces: Vec<String>             → ReadOnly
  └── stores: HashMap<String, MemoryStore>       → lazy-opened au premier accès

MemoryStore
  └── conn: rusqlite::Connection                 → une connexion par namespace.db
       ├── EpisodicMemory::new(store)
       ├── SemanticMemory::new(store)
       └── ProceduralMemory (lecture via trigger_text)
```

`MemoryManager` est le seul point d'entrée autorisé — ne pas instancier `MemoryStore` directement dans les agents. L'interface Python `MemoryInterface` passe toujours par `MemoryManager`.

---

## 9. CLI de gestion

```bash
# Inspecter un namespace
$ apollia-os memory inspect crm-dupont
  Namespace   : crm-dupont
  Fichier     : ~/.apollia/memory/crm-dupont.db (2.3 MB)
  Embedding   : local_gguf (all-MiniLM-L6-v2, 384 dims)
  Épisodes    : 847 (12 expirés)
  Sémantique  : 234 clés
  Procédures  : 8

# Recherche directe
$ apollia-os memory search crm-dupont "devis refusé client"
  [0.89] (episodic)  2026-02-14 · Devis #142 refusé par Dupont SA
  [0.81] (episodic)  2026-01-30 · Devis #138 refusé par Martin SAS
  [0.74] (semantic)  client.dupont_sa.budget_max → "15000"

# Export (souveraineté totale)
$ apollia-os memory export crm-dupont --format json > backup.json

# Import
$ apollia-os memory import crm-dupont backup.json
  ✔ 847 entrées importées

# Purge manuelle
$ apollia-os memory purge crm-dupont
  ✔ 12 entrées expirées supprimées
```

---

## 10. Décisions architecturales clés

| Décision | Justification |
|---|---|
| SQLite unifié (pas base externe) | Zéro dépendance opérationnelle, fichier copiable/backupable, monolithic-first |
| 3 types persistants + working RAM | Couverture cas PME réels sans over-engineering |
| Working memory = scratchpad RAM | Pas d'overhead SQLite pour état temporaire de tâche |
| Dégradation FTS5 → GGUF → Ollama | Agent fonctionnel sans embedding, sémantique s'active progressivement |
| Écriture toujours à initiative agent | Pas d'automatisme opaque, coût LLM maîtrisé, comportement prévisible |
| Pas de consolidation automatique MVP | Réduit le risque de comportement imprévisible et de coûts cachés |
| Tokenizer `unicode61` | Cible PME française — accentuation native correcte |
| Pas de téléchargement automatique | Souveraineté totale — aucun binaire sans consentement explicite |

---

*Prochaine lecture recommandée : [ORIA Engine](./Briques-ORIA-Engine)*
