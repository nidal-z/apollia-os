# ADR-060 — ContextProvider Trait

**Date :** 2026-04-04
**Statut :** Accepté
**Décideur :** Nidal
**Sprint :** 35 — Workspace Intelligence & Execution Performance

---

## Contexte

Le Sprint 35 introduit le contexte workspace (branche git, APOLLIA.md, arborescence) injecté dans le system prompt avant chaque appel Reasoner. Cette injection est initialement implémentée par `WorkspaceAssembler` dans `apollia-workspace`.

**Problème :** Si `WorkspaceAssembler` est le seul mécanisme, le contexte injecté est limité au périmètre workspace. D'autres sources de contexte situationnel peuvent être utiles :
- Contexte réseau (latences actuelles, endpoints disponibles)
- Contexte utilisateur (préférences, niveau d'expertise)
- Contexte domaine (données métier du projet)

**Distinction fondamentale :**
- **Mémoire** (Principe #6) : accumulation de connaissances par l'agent, à son initiative exclusive
- **Context** : situation courante du runtime au moment de l'exécution — fournie par le runtime, pas par l'agent

---

## Décision

Définir le trait `ContextProvider` dans `apollia-core` :

```rust
/// Fournisseur de contexte situationnel pour le system prompt.
/// Distingué de la mémoire (Principe #6) : le Context décrit la situation
/// courante du runtime — la mémoire est accumulée par l'agent à sa propre initiative.
#[async_trait]
pub trait ContextProvider: Send + Sync {
    /// Identifiant unique du provider (ex. "workspace", "user-profile").
    fn provider_id(&self) -> &'static str;

    /// Collecte le contexte. Retourne None si non applicable (ex. hors repo git).
    async fn collect(&self) -> Option<ContextSection>;

    /// Indique si ce provider est applicable dans l'environnement courant.
    /// Appelé avant collect() pour éviter des appels inutiles.
    fn is_applicable(&self) -> bool { true }
}

pub struct ContextSection {
    pub provider_id: String,
    pub title: String,      // En-tête de section dans le system prompt
    pub content: String,    // Contenu Markdown
    pub token_estimate: u32,
}
```

### Trois niveaux d'extension

**Niveau 1 — Rust natif :**
Implémenter `ContextProvider` dans une crate du workspace. Le provider est enregistré au démarrage du runtime.

```rust
pub struct GitWorkspaceProvider { cwd: PathBuf }

#[async_trait]
impl ContextProvider for GitWorkspaceProvider {
    fn provider_id(&self) -> &'static str { "workspace" }
    fn is_applicable(&self) -> bool { self.cwd.join(".git").exists() }
    async fn collect(&self) -> Option<ContextSection> { /* ... */ }
}
```

**Niveau 2 — Duck-typing Python :**
Un agent Python peut exposer `context_providers()` retournant une liste de callables async. Chaque callable retourne un dict `{ title, content }`.

```python
async def context_providers(self) -> list:
    return [self._collect_project_context]

async def _collect_project_context(self) -> dict:
    return {"title": "Contexte Projet", "content": "..."}
```

**Niveau 3 — Script stdin/stdout JSON :**
Pour les providers externes, un script est lancé en subprocess. Il reçoit un JSON de contexte sur stdin et retourne un `ContextSection` JSON sur stdout.

```bash
# Le script reçoit : {"cwd": "/path", "session_id": "..."}
# Le script retourne : {"title": "...", "content": "..."}
```

### Rejet de WorkspaceAssembler concret unique

L'option de garder `WorkspaceAssembler` comme implémentation concrète unique est rejetée car :
1. Non extensible — tout nouveau type de contexte requiert de modifier `WorkspaceAssembler`
2. Viole le Principe #5 (un acteur, une responsabilité) si `WorkspaceAssembler` gère plusieurs domaines
3. Incompatible avec les providers Python (duck-typing) et les scripts externes

### `is_applicable()` — Exemple GitWorkspaceProvider

`GitWorkspaceProvider.is_applicable()` retourne `false` si le CWD n'est pas dans un repo git (absence de `.git/`). Cela évite d'appeler `git status` inutilement dans des projets non-versionés.

---

## Conséquences

**Positives :**
- Extensibilité : tout nouveau type de contexte situationnel est ajouté sans modifier le core
- Rétrocompatibilité : le comportement V1 (workspace git) est préservé par `GitWorkspaceProvider`
- Les providers Python permettent aux agents de contribuer à leur propre contexte situationnel (domaine métier)

**Négatives / Compromis :**
- Overhead d'abstraction pour un cas d'usage V1 simple — justifié par les niveaux 2 et 3 prévus à court terme
- Les providers scripts (niveau 3) ont une latence de subprocess — un timeout strict est appliqué (500ms par provider externe)

---

## Principes architecturaux impactés

- **Principe #6 — Mémoire à initiative de l'agent** : Ce trait concerne le Context (situation courante), pas la Mémoire. La distinction est documentée dans le trait lui-même. Conforme.
- **Principe #5 — Un acteur, une responsabilité** : Chaque `ContextProvider` a une responsabilité unique. `WorkspaceAssembler` orchestre sans accumuler. Conforme.

---

## Liens

- Story d'implémentation : STORY-465b (retrofit)
- Implémenté dans : `crates/apollia-core/src/context_provider.rs`, `crates/apollia-workspace/`
- Wiki : [Briques-Workspace](../wiki/Briques-Workspace.md)
- ADR connexe : [ADR-056](ADR-056-workspace-context-assembly.md) — WorkspaceAssembler
