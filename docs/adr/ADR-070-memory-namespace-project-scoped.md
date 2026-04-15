# ADR-070 — Memory namespace project-scoped

**Date :** 2026-04-15
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Pré-implémentation (cible : Sprint 39, STORY-502)

---

## Contexte

Le champ `memory_namespace` dans `AgentManifest` est une chaîne statique déclarée par l'agent
dans son `manifest()` (ex. `"dev-assistant"`). Cette valeur est passée telle quelle à
`MemoryInterface` lors de la construction du contexte agent dans
`crates/apollia-aip/src/context.rs`. Il n'y a aucun scoping par projet.

**Conséquence concrète :** si `dev-assistant` tourne simultanément ou successivement sur
le projet `apollia-v2` et sur un projet `client-crm`, les deux sessions partagent le même
namespace `"dev-assistant"` dans SQLite. L'agent peut rappeler des règles architecturales
du mauvais projet, des patterns incorrects, des décisions obsolètes — sans jamais s'en
apercevoir.

**Le `project_id` existe déjà :** `ChatSession` porte un champ `pub project_id: Option<String>`
qui est fourni lors de la création de session (ADR-069, Sprint 38 file picker). Il n'a jamais
été propagé jusqu'à `MemoryInterface`.

**Sévérité :** 🔴 BLOQUANT pour tout déploiement d'assistant à mémoire persistante. Sans
ce fix, les quatre assistants de Sprint 39 polluent les projets entre eux dès l'instant où
un utilisateur jongle entre deux projets.

---

## Décision

Nous adoptons une **convention de préfixage `project_id`** sur le namespace mémoire effectif,
calculé à l'initialisation du contexte agent :

```
effective_namespace = "{project_id}:{manifest.memory_namespace}"   si project_id est Some(_)
effective_namespace = manifest.memory_namespace                     si project_id est None
```

**Exemples :**

| Scénario | Namespace effectif |
|---|---|
| `dev-assistant` dans projet `proj_abc123` | `"proj_abc123:dev-assistant"` |
| `dev-assistant` en session standalone (Chat Libre) | `"dev-assistant"` |
| `spec-assistant` dans projet `proj_abc123` | `"proj_abc123:spec-assistant"` |
| `excel-worker` (worker sans mémoire persistante) | inchangé |

### Implémentation

**1. `crates/apollia-aip/src/context.rs`** — Ajouter la fonction de calcul :

```rust
/// Calcule le namespace effectif pour la mémoire d'un agent.
///
/// Si l'agent tourne dans un contexte projet, préfixe avec le project_id
/// pour garantir l'isolation entre projets.
///
/// Convention : "{project_id}:{manifest_namespace}" | "{manifest_namespace}"
fn effective_memory_namespace(
    manifest_namespace: &str,
    project_id: Option<&str>,
) -> String {
    match project_id {
        Some(pid) if !pid.is_empty() => format!("{}:{}", pid, manifest_namespace),
        _ => manifest_namespace.to_owned(),
    }
}
```

Utiliser cette fonction à l'appel de `MemoryInterface::new()`, en propageant le `project_id`
de la `ChatSession` courante via la chaîne d'initialisation du contexte.

**2. `crates/apollia-aip/src/memory.rs`** — Aucun changement de signature : `MemoryInterface`
stocke déjà `namespace: String`. C'est l'appelant qui passe la valeur préfixée.

**3. `crates/apollia-core/src/manifest.rs`** — Pas de changement de type. Documenter dans le
docstring de `memory_namespace` que le runtime préfixe automatiquement avec `project_id` en
mode projet.

### Invariants

- **Rétrocompatibilité des données** : les namespaces existants (sans project_id) continuent
  de fonctionner en session standalone. Il n'y a pas de migration de données.
- **Stateless** : `effective_memory_namespace()` est une fonction pure — conforme au Principe #4.
- **Transparent pour l'agent Python** : l'agent déclare `"dev-assistant"` dans son manifest,
  le runtime gère le préfixage automatiquement. L'API Python `ctx.memory.remember()` ne change
  pas.
- **Workers non-impactés** : les worker agents (workers A2A sans mémoire persistante déclarée)
  ne sont pas affectés.

---

## Alternatives considérées

### Option A — Namespace déclaré par l'agent lui-même (rejetée)

L'agent inclut `project_id` dans son `memory_namespace` statique.
**Contre :** Le `project_id` n'est pas connu à l'écriture du manifest Python — c'est une
donnée runtime. Crée du couplage agent ↔ infrastructure. Viole le Principe #3 (contrat minimal).

### Option B — Namespace par défaut = project_id seul (rejetée)

`effective_namespace = project_id` pour tous les agents d'un projet, sans distinction.
**Contre :** Deux agents différents (`spec-assistant` et `dev-assistant`) dans le même projet
partagent leur namespace, les clés se mélangent. Pire que le statu quo.

### Option C — Table de relation agent/projet séparée dans SQLite (rejetée)

Ajouter une table `agent_project_memory` avec jointure.
**Contre :** Surcharge architecturale disproportionnée. Le préfixage de namespace est suffisant
et aligné avec les patterns de namespacing par convention (Kubernetes, Redis, etc.).

### Option retenue — Préfixage automatique `project_id:namespace`

**Pour :**
- Isolation garantie sans changement de schéma SQLite.
- Transparent pour les agents Python.
- Compatible avec la logique FTS5 existante (FTS5 filtre par namespace, la valeur exacte importe peu).
- Zéro coût en session standalone (le préfixage ne s'applique que si `project_id` est `Some`).

**Compromis acceptés :**
- Les mémoires de la même IA sur deux projets différents ne sont jamais fusionnées — c'est intentionnel.
- Si un utilisateur supprime un projet, les entrées mémoire préfixées restent en base (orphelines).
  Nettoyage à l'initiative de l'agent ou via `apollia memory purge --project <id>` (feature future).

---

## Conséquences

**Positives :**
- Isolation mémoire complète entre projets — un assistant ne peut plus "contaminer" son contexte
  d'un projet à l'autre.
- Aucune modification de l'API Python. Aucune migration de schéma.
- Cohérent avec le `project_id` déjà présent dans `ChatSession` (ADR-069).

**Négatives / Compromis :**
- Données orphelines si un projet est supprimé sans purge explicite. Toléré pour v1.
- Impossible de partager une mémoire sémantique entre deux agents d'un même projet via le namespace
  — par design. Si besoin de partage, utiliser des clés de convention (`"shared.*"`).

**Neutres / À surveiller :**
- Performance FTS5 : le préfixage allonge légèrement les clés de namespace. Impact négligeable
  sur les volumes attendus.
- CLI `apollia memory inspect` : afficher le namespace effectif, pas le namespace statique déclaré.

---

## Principes architecturaux impactés

- **Principe #5 — Un acteur, une responsabilité** : La fonction `effective_memory_namespace()`
  est une responsabilité du pont PyO3, pas de l'agent ni du moteur mémoire.
- **Principe #6 — Mémoire à initiative de l'agent** : Non-impacté. L'agent écrit et lit sa
  mémoire comme avant. Le runtime préfixe le namespace de manière transparente.
- **Principe #4 — Fail fast** : La fonction est pure et synchrone — erreur impossible à ce stade.

---

## Liens

- Story d'implémentation : [STORY-502](../internal/STORIES/sprint-39/story-502-memory-namespace-project-scoped.md)
- ADR connexe : [ADR-007 — Mémoire à initiative de l'agent](ADR-007-memoire-initiative-agent.md)
- ADR connexe : [ADR-038 — Global User Memory](ADR-038-global-user-memory.md)
- Fichiers impactés :
  - `crates/apollia-aip/src/context.rs`
  - `crates/apollia-aip/src/memory.rs`
  - `crates/apollia-core/src/manifest.rs`
