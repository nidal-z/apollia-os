# ADR-056 - Workspace Context, ContextProvider Trait, Memory Namespace & ContextBootstrap

**Date :** 2026-04-04 (workspace) / 2026-04-15 (namespace + bootstrap)
**Statut :** Accepté
**Décideur :** Nidal
**Sprint :** 35 (workspace + ContextProvider) → 39 (namespace scoping) → 40 (bootstrap)

---

## Contexte

### Workspace context assembly (Sprint 35)

Une analyse comparative Apollia OS vs Claude Code a identifié que l'agent ignore complètement le projet dans lequel il opère. Sans contexte workspace, un agent doit découvrir lui-même la branche git, les fichiers modifiés, le langage dominant - en consommant des steps et tokens précieux.

**Besoin :** injecter automatiquement dans le system prompt les métadonnées du projet courant avant chaque appel Reasoner.

### ContextProvider trait (Sprint 35)

Si `WorkspaceAssembler` est le seul mécanisme, le contexte injecté est limité au périmètre workspace. Il faut distinguer :
- **Mémoire** (Principe #6) : accumulation de connaissances par l'agent, à son initiative exclusive
- **Context** : situation courante du runtime au moment de l'exécution - fournie par le runtime

### Memory namespace scoping (Sprint 39)

`memory_namespace` dans `AgentManifest` est une chaîne statique. Si `dev-assistant` tourne sur deux projets différents, les deux sessions partagent le même namespace - l'agent peut rappeler des règles du mauvais projet. Le `project_id` existait déjà dans `ChatSession` mais n'avait jamais été propagé jusqu'à `MemoryInterface`.

### ContextBootstrap (Sprint 40)

Les quatre assistants Sprint 39 implémentaient un pattern mémoire ad-hoc avec trois lacunes : aucune détection de péremption (cache indéfiniment valide), duplication de code dans 3 agents, contexte superficiel (texte brut uniquement). Il manquait un protocole officiel de bootstrapping.

---

## Décisions

### 1 - WorkspaceAssembler et crate `apollia-workspace`

La crate `apollia-workspace` expose :

- **`WorkspaceAssembler`** : orchestrateur principal, agrège les providers avec timeout global 2s et TTL de cache 30s
- **`GitContextCollector`** : collecte branche, HEAD, fichiers modifiés via subprocess `git`
- **`ApolliamdFinder`** : recherche `APOLLIA.md` en remontant depuis CWD → parents → `$HOME`
- **`DirectoryTreeBuilder`** : arborescence limitée à 3 niveaux, exclusions `.git`, `node_modules`, `target`

**Rejet de git2 crate :** dépendance dynamique à `libgit2` (C) - incompatible avec Principe #2. Sur les repos sans git, `GitContextCollector` retourne `None` (fail-silent).

**Convention APOLLIA.md :** même priorité que `CLAUDE.md` de Claude Code. Premier fichier trouvé gagne. Si aucun n'existe, le champ est `None`.

### 2 - Trait `ContextProvider` (défini dans `apollia-core`)

```rust
/// Fournisseur de contexte situationnel pour le system prompt.
/// Distingué de la mémoire (Principe #6) : le Context décrit la situation
/// courante du runtime - la mémoire est accumulée par l'agent à sa propre initiative.
#[async_trait]
pub trait ContextProvider: Send + Sync {
    fn provider_id(&self) -> &'static str;
    async fn collect(&self) -> Option<ContextSection>;
    fn is_applicable(&self) -> bool { true }
}

pub struct ContextSection {
    pub provider_id: String,
    pub title: String,
    pub content: String,
    pub token_estimate: u32,
}
```

**Trois niveaux d'extension :**

- **Niveau 1 - Rust natif :** implémenter `ContextProvider` dans une crate du workspace
- **Niveau 2 - Duck-typing Python :** un agent expose `context_providers()` retournant des callables async renvoyant `{ title, content }`
- **Niveau 3 - Script stdin/stdout JSON :** subprocess recevant `{"cwd": ..., "session_id": ...}` et retournant un `ContextSection` JSON (timeout 500ms)

`WorkspaceAssembler` concret unique rejeté car non extensible et incompatible avec les providers Python/scripts.

### 3 - Memory namespace project-scoped

Convention de préfixage `project_id` sur le namespace mémoire effectif :

```
effective_namespace = "{project_id}:{manifest.memory_namespace}"   si project_id est Some(_)
effective_namespace = manifest.memory_namespace                     si project_id est None
```

**Exemples :**

| Scénario | Namespace effectif |
|---|---|
| `dev-assistant` dans projet `proj_abc123` | `"proj_abc123:dev-assistant"` |
| `dev-assistant` en session standalone | `"dev-assistant"` |

**Implémentation (`crates/apollia-aip/src/context.rs`) :**

```rust
fn effective_memory_namespace(manifest_namespace: &str, project_id: Option<&str>) -> String {
    match project_id {
        Some(pid) if !pid.is_empty() => format!("{}:{}", pid, manifest_namespace),
        _ => manifest_namespace.to_owned(),
    }
}
```

Transparent pour l'agent Python : l'agent déclare `"dev-assistant"` dans son manifest, le runtime gère le préfixage automatiquement.

### 4 - ContextBootstrap : protocole SDK officiel

`ContextBootstrap` est un protocole Python abstrait distribué dans le SDK (`sdk/apollia/bootstrap.py`, version 0.2.0+). Contrat minimal : **2 méthodes abstraites** + **4 méthodes d'infrastructure**.

```python
from apollia.bootstrap import ContextBootstrap

class MyBootstrap(ContextBootstrap):
    async def is_stale(self, ctx) -> bool:
        """Le snapshot existant est-il périmé ? Retourner True en cas de doute."""
        ...

    async def run_bootstrap(self, ctx) -> dict:
        """Explorer le domaine, construire un snapshot, appeler self.persist(). Doit être idempotent."""
        ...
```

**Infrastructure héritée (4 méthodes, override rare) :**

| Méthode | Comportement par défaut |
|---|---|
| `needs_bootstrap(ctx)` | Lit `bootstrap.status` → si None/missing/partial → True ; si complete → délègue à `is_stale()` |
| `load_snapshot(ctx)` | Lit `bootstrap.snapshot` → `json.loads()` ou None |
| `load_meta(ctx)` | Lit `bootstrap.meta` → `json.loads()` ou None |
| `persist(ctx, snapshot, *, staleness_marker, ...)` | Écrit snapshot + meta + status. Refuse de downgrader `complete` → `partial`. |

**Convention de clés mémoire :**

```
bootstrap.snapshot    # Le snapshot complet (JSON sérialisé)
bootstrap.meta        # {"version": int, "created_at": str, "staleness_marker": str}
bootstrap.status      # "complete" | "partial" | "missing"
```

**Pattern d'intégration dans `run()` :**

```python
async def run(self, task, ctx):
    input_text, history = _extract_task_input(task)
    is_first_turn = not history

    if is_first_turn and await self._bootstrap.needs_bootstrap(ctx):
        await self._bootstrap.run_bootstrap(ctx)

    snapshot = await self._bootstrap.load_snapshot(ctx)
    rules = snapshot.get("rules_raw", "") if snapshot else ctx.workspace.rules or ""
```

**Distinction workspace / bootstrap :**

| | `ctx.workspace` | `ContextBootstrap` |
|---|---|---|
| Portée | Session courante (éphémère) | Cross-session (persistant SQLite) |
| Contenu | APOLLIA.md parsé par le runtime | Snapshot enrichi : architecture, patterns, deps |
| Coût | Zéro | Payé une fois, réutilisé indéfiniment |
| Fraîcheur | Toujours à jour | Validé par staleness marker |

**Variantes de staleness marker :**

| Domaine | Staleness marker | Logique |
|---|---|---|
| Dev (pipeline Apollia) | Hash du dernier commit git | `current_hash != meta["staleness_marker"]` |
| Document | Timestamp max des fichiers | `latest_mtime > float(meta["staleness_marker"])` |
| Comptabilité | TTL 7 jours | `datetime.now() - created_at > timedelta(days=7)` |

**Invariants du protocole :**
1. Idempotence : `run_bootstrap()` appelé N fois produit le même état final
2. Non-destructif : `persist("partial")` ne remplace jamais un `"complete"` existant
3. Opt-in : pattern Python SDK, pas un contrat AIP - le runtime Rust ne connaît pas `ContextBootstrap`
4. Principe #6 : le bootstrap est à l'initiative de l'agent, jamais injecté par le runtime
5. Graceful degradation : si `ctx.memory is None`, le protocole retourne des valeurs neutres

---

## Alternatives considérées

### Workspace : git2 crate (rejetée)

Dépendance dynamique à `libgit2` (C) - incompatible avec Principe #2. Temps de compilation > 30s. Binary size +5 MB inutile quand un subprocess `git` suffit.

### ContextProvider : WorkspaceAssembler concret unique (rejetée)

Non extensible - tout nouveau type de contexte requiert de modifier `WorkspaceAssembler`. Viole le Principe #5 si `WorkspaceAssembler` gère plusieurs domaines. Incompatible avec les providers Python.

### Namespace : namespace déclaré par l'agent lui-même (rejetée)

Le `project_id` n'est pas connu à l'écriture du manifest Python - c'est une donnée runtime. Viole le Principe #3 (contrat minimal).

### Namespace : table de relation agent/projet séparée (rejetée)

Surcharge architecturale disproportionnée. Le préfixage de namespace est suffisant et aligné avec les patterns de namespacing par convention (Kubernetes, Redis).

### Bootstrap : nouveau hook `on_bootstrap()` dans le contrat AIP (rejetée)

Viole le Principe #3 (contrat minimal). Impose le bootstrapping à tous les agents, y compris les workers qui n'en ont pas besoin.

### Bootstrap : injection automatique par le runtime (rejetée)

Viole explicitement le Principe #6. Le runtime ne peut pas savoir quelle logique de staleness est pertinente pour chaque domaine d'agent.

---

## Conséquences

**Positives :**
- Timeout global 2s : la collecte workspace ne bloque jamais l'exécution d'une tâche
- Isolation mémoire complète entre projets - un assistant ne peut plus contaminer son contexte
- Suppression du pattern copié-collé dans les 4 assistants Sprint 39
- Économie de tokens : la phase de découverte n'est payée qu'une fois par session
- Extensibilité : tout nouveau type de contexte est ajouté sans modifier le core

**Négatives / Compromis :**
- Données orphelines en mémoire si un projet est supprimé sans purge explicite
- Premier lancement sur un projet vierge : latence 3-5s le temps du bootstrap
- Double source of truth workspace (`ctx.workspace`) / bootstrap (SQLite) - par design, coexistent

**Neutres / À surveiller :**
- Si `git` n'est pas dans `$PATH`, `GitContextCollector` retourne `None` silencieusement (Windows)
- Taille des snapshots : tronquer les contenus bruts à 8K max

---

## Principes architecturaux impactés

- **Principe #2 - Zéro dépendance externe** : Pas de `libgit2`. Subprocess `git` ubiquitaire, fail-silent si absent.
- **Principe #3 - Contrat minimal** : `ContextBootstrap` est couche SDK, pas AIP. Aucun agent existant cassé.
- **Principe #5 - Un acteur, une responsabilité** : chaque `ContextProvider` a une responsabilité unique.
- **Principe #6 - Mémoire à initiative de l'agent** : Le Context décrit la situation courante. La mémoire est à l'initiative de l'agent. Le bootstrap est déclenché explicitement dans `run()`.

---

## Liens

- Stories : STORY-458, STORY-459, STORY-460 (Sprint 35) + STORY-502 (Sprint 39) + STORY-511 → STORY-514 (Sprint 40)
- Implémenté dans : `crates/apollia-workspace/`, `crates/apollia-core/src/context_provider.rs`, `crates/apollia-aip/src/context.rs`, `sdk/apollia/bootstrap.py`
- Wiki : [Briques-Workspace](../wiki/Briques-Workspace.md)
