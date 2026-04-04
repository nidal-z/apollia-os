# apollia-workspace — Contexte Workspace et ContextProvider

> *Collecte automatique du contexte situationnel du projet courant — injecté dans le system prompt de chaque agent.*

---

## 1. Rôle et architecture

`apollia-workspace` est la crate qui fournit le contexte situationnel du projet à ORIA. Elle répond à la question : *"Dans quel contexte le projet dans lequel l'agent opère ?"*

**Distinction fondamentale avec la mémoire (Principe #6) :**

| Mémoire | Context (apollia-workspace) |
|---------|----------------------------|
| Accumulée par l'agent à son initiative | Fournie par le runtime avant chaque appel |
| Persiste entre les sessions (SQLite) | Collectée à chaque appel (TTL 30s) |
| Connaissance accumulée | Situation courante |
| Principe #6 : jamais d'injection automatique | Injection automatique dans le system prompt |

---

## 2. Trait `ContextProvider`

Défini dans `apollia-core`, ce trait est l'interface générique pour tous les fournisseurs de contexte :

```rust
/// Fournisseur de contexte situationnel pour le system prompt.
/// Distingué de la mémoire : le Context décrit la situation courante du runtime.
#[async_trait]
pub trait ContextProvider: Send + Sync {
    /// Identifiant unique du provider (ex. "workspace", "user-profile").
    fn provider_id(&self) -> &'static str;

    /// Collecte le contexte. Retourne None si non applicable dans l'environnement courant.
    async fn collect(&self) -> Option<ContextSection>;

    /// Indique si ce provider est applicable. Appelé avant collect() pour éviter les appels inutiles.
    /// Défaut : true.
    fn is_applicable(&self) -> bool { true }
}

pub struct ContextSection {
    pub provider_id: String,
    pub title: String,        // En-tête de section dans le system prompt
    pub content: String,      // Contenu Markdown
    pub token_estimate: u32,  // Estimation du coût en tokens
}
```

### Niveaux d'extension

**Niveau 1 — Rust natif :** Implémenter `ContextProvider` dans une crate du workspace. Le provider est enregistré au démarrage du runtime.

```rust
pub struct GitWorkspaceProvider { pub cwd: PathBuf }

#[async_trait]
impl ContextProvider for GitWorkspaceProvider {
    fn provider_id(&self) -> &'static str { "workspace" }

    fn is_applicable(&self) -> bool {
        self.cwd.join(".git").exists()  // Inactif hors repo git
    }

    async fn collect(&self) -> Option<ContextSection> {
        // Collecte via subprocess git...
    }
}
```

**Niveau 2 — Duck-typing Python :** Un agent Python peut exposer `context_providers()` retournant une liste de callables async.

```python
class MonAgent:
    async def context_providers(self) -> list:
        return [self._collect_project_context]

    async def _collect_project_context(self) -> dict | None:
        return {
            "title": "Contexte Projet",
            "content": "Client principal : Acme Corp. Stack : Django + PostgreSQL."
        }
```

**Niveau 3 — Script stdin/stdout JSON :** Pour les providers externes, un script est lancé en subprocess. Il reçoit un JSON de contexte sur stdin et retourne un `ContextSection` JSON sur stdout.

```bash
# Input stdin : {"cwd": "/path/to/project", "session_id": "s-001"}
# Output stdout : {"title": "CI Status", "content": "Build: ✓ / Tests: 142 passing"}
```

> **Référence technique :** [ADR-060](../adr/ADR-060-context-provider-trait.md)

---

## 3. `WorkspaceAssembler`

`WorkspaceAssembler` orchestre tous les providers enregistrés avec un timeout global et une mise en cache :

```rust
pub struct WorkspaceAssembler {
    providers: Vec<Arc<dyn ContextProvider>>,
    /// TTL du cache en secondes. Défaut : 30s.
    pub cache_ttl: Duration,
    /// Timeout global pour la collecte de tous les providers. Défaut : 2s.
    pub timeout: Duration,
}

impl WorkspaceAssembler {
    /// Collecte le contexte de tous les providers applicables.
    /// Retourne le résultat mis en cache si le TTL n'est pas expiré.
    pub async fn collect(&self) -> WorkspaceContext;

    /// Forçe un refresh du cache (ignore le TTL).
    pub async fn collect_fresh(&self) -> WorkspaceContext;
}
```

**Comportement sur timeout :** si un provider dépasse sa part du timeout global, il est ignoré et le contexte partiel est utilisé. La collecte ne bloque jamais l'exécution d'une tâche.

---

## 4. Providers intégrés

### 4.1 `GitContextCollector`

Collecte les informations git via subprocess `git` (pas de `libgit2` — voir ADR-056) :

```rust
pub struct GitContextCollector { pub cwd: PathBuf }
```

**Commandes exécutées :**
- `git rev-parse --abbrev-ref HEAD` → branche
- `git rev-parse --short HEAD` → SHA court
- `git status --porcelain` → fichiers modifiés

**Fail-silent :** si `git` n'est pas dans le `$PATH` ou si le répertoire n'est pas un repo git, `GitContextCollector` retourne `None`. L'agent continue sans contexte git.

### 4.2 `ApolliamdFinder`

Recherche `APOLLIA.md` en remontant depuis le CWD :

```
CWD/APOLLIA.md → CWD/../APOLLIA.md → ... → $HOME/APOLLIA.md
```

Premier fichier trouvé gagne. Le contenu est lu et inclus tel quel dans le `WorkspaceContext`. Si aucun fichier n'est trouvé, le champ est `None`.

**Taille maximale :** 32 KB. Au-delà, le contenu est tronqué avec `truncate_middle()`.

### 4.3 `DirectoryTreeBuilder`

Génère une arborescence Markdown du répertoire courant :

```
src/
  main.rs
  lib.rs
  utils/
    parser.rs
    formatter.rs
tests/
  integration_test.rs
```

**Limites :**
- Profondeur maximale : 3 niveaux
- Nombre d'entrées maximum : 200
- Exclusions automatiques : `.git/`, `node_modules/`, `target/`, `.DS_Store`

---

## 5. `WorkspaceContext` type

```rust
pub struct WorkspaceContext {
    pub cwd: PathBuf,
    pub git: Option<GitContext>,
    pub apollia_md: Option<String>,        // Contenu du fichier APOLLIA.md
    pub apollia_md_path: Option<PathBuf>,  // Chemin du fichier APOLLIA.md trouvé
    pub directory_tree: Option<String>,    // Arborescence Markdown
    pub sections: Vec<ContextSection>,     // Sections des providers additionnels
    pub collected_at: Instant,
}

pub struct GitContext {
    pub branch: String,
    pub head_sha: String,             // 8 premiers caractères du SHA
    pub modified_files: Vec<String>,  // Fichiers modifiés (git status --porcelain)
    pub is_dirty: bool,
}
```

---

## 6. Injection dans ORIA

Le `WorkspaceContext` est injecté dans le system prompt avant chaque appel Reasoner. Le format est Markdown :

```
## Contexte workspace courant

**Répertoire :** /Users/alice/dev/mon-projet
**Branche git :** feature/add-caching (2 fichiers modifiés)
**APOLLIA.md :** Répondre toujours en français. Priorité aux tests unitaires.

### Structure du projet
src/
  main.rs
  lib.rs
tests/
  lib.rs
```

Si aucun provider ne retourne de contexte (répertoire sans git, sans APOLLIA.md), la section est omise du system prompt.

### Injection dans le bridge Python (AIP)

Le contexte workspace est également accessible depuis Python via `ctx.workspace` :

```python
async def run(self, task: AIPTask, ctx: RuntimeContext) -> AIPResult:
    if ctx.workspace:
        branch = ctx.workspace.get("git", {}).get("branch")
        apollia_md = ctx.workspace.get("apollia_md")
```

---

## 7. `APOLLIA.md` — Personnalisation par projet

`APOLLIA.md` est le mécanisme de personnalisation du comportement de l'agent par projet. Il suit la même convention que `CLAUDE.md` dans l'écosystème Claude Code.

**Cas d'usage typiques :**

```markdown
# APOLLIA.md — Projet Mon-API

Répondre toujours en français.
Utiliser des noms de variables explicites (pas de `x`, `tmp`, `data`).
Stack : Rust + axum + SQLite.
Tests : `cargo test -p <crate>` avant chaque commit.
```

**Priorité de recherche :**
1. `./APOLLIA.md` (CWD)
2. `../APOLLIA.md` (parent)
3. `../../APOLLIA.md` (remontée)
4. `$HOME/APOLLIA.md` (global)

Un `APOLLIA.md` à la racine du projet s'applique à tous les agents lancés depuis ce projet.

---

## 8. CLI `apollia workspace`

```bash
# Affiche le contexte workspace courant
apollia-os workspace status

# Exemple de sortie :
#   Répertoire : /Users/alice/dev/mon-projet
#   Branche    : main (3 fichiers modifiés)
#   APOLLIA.md : /Users/alice/dev/mon-projet/APOLLIA.md ✓
#   Providers  : git ✓, tree ✓, apollia-md ✓
#   Cache      : frais (collecté il y a 2s)

# Forçe un refresh du cache
apollia-os workspace refresh

# Affiche le contexte tel qu'il sera injecté dans le system prompt
apollia-os workspace show
```

---

## 9. Décisions architecturales clés

| Décision | Justification |
|---|---|
| Subprocess `git` (rejet `git2`) | Zéro dépendance C, binary size +0 MB, fail-silent si git absent (ADR-056) |
| TTL 30s | Évite les I/O répétées sur les sessions longues sans staleness significative |
| Timeout global 2s | La collecte ne bloque jamais l'exécution d'une tâche |
| `ContextProvider` trait (rejet implémentation concrète unique) | Extensibilité Rust/Python/script — 3 niveaux d'extension (ADR-060) |
| `is_applicable()` sur le trait | Évite les appels inutiles (ex. git hors repo) |
| APOLLIA.md priorité CWD > parents > $HOME | Convention identique à CLAUDE.md — comportement attendu par les développeurs |
| Exclusions arborescence | `.git/`, `node_modules/`, `target/` exclus par défaut — tokens économisés |

---

---

## 10. CommandLoader — Chargement des slash commands custom — Sprint 36

Depuis le Sprint 36 (STORY-493), `apollia-workspace` fournit `CommandLoader` pour charger les commandes slash custom depuis le disque.

```rust
// crates/apollia-workspace/src/commands.rs

/// Charge les fichiers .md de commandes depuis un répertoire.
pub struct CommandLoader;

impl CommandLoader {
    /// Lit tous les .md dans `dir`, parse le frontmatter YAML et le template.
    pub async fn load_from_dir(dir: &Path) -> Vec<CustomCommand> { ... }
}
```

**Format d'un fichier de commande** (`.apollia/commands/review.md`) :

```markdown
---
description: Revue de code de la tâche courante
args: [focus]
---

Analyse le code en cours avec un focus sur {{focus}}.
Vérifie : correctness, performance, sécurité.
```

**Règles de chargement :**
- Répertoire absent → fail-silent (registry vide, pas de panic)
- CWD `.apollia/commands/` a priorité sur `~/.apollia/commands/`
- `list()` retourne les commandes triées alphabétiquement
- Hot reload via `FileTimestampCache` si les fichiers `.md` sont modifiés

> **Voir aussi :** [Briques CLI — Slash commands custom](./Briques-CLI.md#slash-commands-custom--apollia_commands-story-493)

---

## Voir aussi

- [Briques ORIA Engine — Workspace Context](./Briques-ORIA-Engine.md#workspace-context) — injection dans le system prompt
- [ADR-056](../adr/ADR-056-workspace-context-assembly.md) — Workspace Context Assembly
- [ADR-060](../adr/ADR-060-context-provider-trait.md) — ContextProvider trait
- [Briques LLM Backend](./Briques-LLM-Backend.md) — TokenBudget et Prompt Caching
