# ADR-059 — Concurrent Tool Execution

**Date :** 2026-04-04
**Statut :** Accepté
**Décideur :** Nidal
**Sprint :** 35 — Workspace Intelligence & Execution Performance

---

## Contexte

L'analyse comparative a identifié que les plans ORIA d'observation exécutent les outils **sériellement**, même quand ils sont indépendants.

**Mesure :**
- Plan d'analyse : 20 appels `FileReadExecutor` ou `GrepExecutor`, 50ms chaque
- Exécution sérielle : 20 × 50ms = **1 000ms**
- Exécution parallèle (`join_all`) : ~50ms (limited by the slowest) → **facteur 20×**

Sur les sessions d'analyse de repo (sprint goal : "un agent peut résumer un repo entier"), ce pattern est dominant. La latence sérielle est le principal frein à l'adoption.

---

## Décision

### Champ `is_read_only` sur `ToolDescriptor`

```rust
pub struct ToolDescriptor {
    // champs existants...
    pub is_read_only: bool,  // Défaut : false (conservateur)
}
```

**Défaut `false` = conservateur :** tout nouvel outil est considéré avec effets de bord jusqu'à annotation explicite. Cela évite les régressions sur les outils futurs oubliés.

### Méthode `execute_batch()` sur `ToolDispatcher`

```rust
impl ToolDispatcher {
    /// Exécute un batch d'invocations.
    /// - Si TOUS les outils sont read-only → join_all + Semaphore(10)
    /// - Si au moins UN n'est pas read-only → exécution sérielle (ordre garanti)
    pub async fn execute_batch(
        &self,
        calls: Vec<ToolCall>,
    ) -> Vec<Result<Value, ToolExecutionError>>;
}
```

**Règle fondamentale : batch mixte → sériel obligatoire.** Un seul outil avec effets de bord dans un batch force l'exécution séquentielle de l'ensemble. L'ordre des effets est toujours garanti.

### Semaphore(10) pour les batches read-only

La concurrence est limitée par un `tokio::sync::Semaphore` de 10 permits pour éviter :
- La saturation des file descriptors système
- La surcharge de la table SQLite d'audit
- Les pics CPU sur les machines contraintes

### Outils marqués `is_read_only = true`

| Outil | Justification |
|-------|---------------|
| `FileReadExecutor` | Lecture seule, pas d'écriture disque |
| `GlobExecutor` | Parcours d'arborescence, lecture seule |
| `GrepExecutor` | Recherche regex, lecture seule |
| `ListExecutor` (`file_list`) | Listage, lecture seule |
| `MemorySearchExecutor` | FTS5 SELECT-only |
| `GitStatusExecutor` | `git status --porcelain`, lecture seule |

### Outils NON marqués (is_read_only = false)

| Outil | Justification |
|-------|---------------|
| `BashExecutor` | Effets de bord arbitraires |
| `FileWriteExecutor` | Écriture disque |
| `FileEditExecutor` | Modification disque |
| `PersistentBashExecutor` | Shell stateful avec effets de bord |
| `McpToolExecutor` | Effets de bord inconnus côté serveur |

### Préservation de l'ordre

`join_all` de `futures::future` préserve l'ordre des résultats correspondant à l'ordre d'entrée. L'index `i` de `calls[i]` correspond toujours à l'index `i` du vecteur de résultats retourné.

---

## Conséquences

**Positives :**
- Plans d'observation 20 outils read-only : 1000ms → ~50ms (facteur 20×)
- `is_read_only = false` par défaut : pas de régression sur les outils existants ou futurs
- Semaphore(10) : protection contre la saturation des ressources système

**Négatives / Compromis :**
- Un seul outil non-read-only dans un batch force le sériel pour tout le batch. Si ORIA génère des plans mixtes, le gain est partiel. La solution est que le Reasoner génère des batches homogènes — documenté dans le wiki.
- L'ordre des résultats dans un batch parallel est celui de l'entrée, pas celui de completion — l'outil le plus lent bloque la progression perçue mais pas le résultat final.

**Neutres / À surveiller :**
- Les outils MCP (`McpToolExecutor`) sont `is_read_only = false` par défaut même si le serveur n'a que des outils de lecture. Une annotation per-tool sera envisagée si un serveur MCP déclare explicitement `read-only: true` dans sa réponse `tools/list`.

---

## Principes architecturaux impactés

- **Principe #5 — Un acteur, une responsabilité** : `ToolDispatcher` gère le routing et la concurrence. `ToolExecutor` gère l'exécution d'un outil individuel. Conforme.
- **Principe #4 — Fail fast** : Un batch mixte → sériel immédiatement, sans tentative d'optimisation partielle risquée. Conforme.

---

## Liens

- Story d'implémentation : STORY-456
- Implémenté dans : `crates/apollia-tools/src/dispatcher.rs`
- Wiki : [Briques Tool Registry — Concurrence d'outils](../wiki/Briques-Tool-Registry.md#concurrence-doutils)
- ADR connexe : [ADR-015](ADR-015-tool-executor-trait-abstraction.md) — `ToolExecutor` trait
- ADR connexe : [ADR-043](ADR-043-decomposition-atomique-outils.md) — décomposition atomique
