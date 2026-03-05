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

## 4. Stratégie d'embedding — dégradation gracieuse

La recherche sémantique (vectorielle) est puissante mais nécessite un modèle d'embedding. Apollia OS adopte une stratégie de dégradation gracieuse :

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

```toml
# apollia.toml — configuration embedding
[memory]
embedding_strategy = "auto"     # "fts_only" | "local_gguf" | "ollama" | "auto"
gguf_model_path    = ""         # Rempli si local_gguf souhaité
ollama_url         = ""         # Rempli si Ollama souhaité
```

En mode `auto`, le runtime détecte ce qui est disponible et utilise le niveau le plus élevé sans intervention manuelle.

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

---

## 6. Isolation par namespace

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
    memory_namespace="crm-dupont",              # Namespace privé (lecture/écriture)
    shared_memory_namespaces=["shared"],         # Namespaces partagés (lecture par défaut)
)
```

La concurrence SQLite est gérée par WAL (Write-Ahead Logging) : plusieurs lecteurs simultanés, un seul writer, sans blocage.

---

## 7. Configuration TTL

```toml
[memory]
episodic_ttl_days    = 90     # Épisodes purgés après 90 jours
semantic_ttl_days    = null   # Faits permanents par défaut
procedural_ttl_days  = null   # Workflows permanents par défaut
purge_on_startup     = true   # Purge au démarrage (async, non-bloquant)
```

La purge est **asynchrone et non-bloquante** au démarrage. Elle ne ralentit pas le démarrage de l'agent.

---

## 8. CLI de gestion

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

## 9. Décisions architecturales clés

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
