# Template — User Story Apollia OS

> Copier ce template pour chaque nouvelle story. Supprimer les commentaires `<!-- -->` avant de livrer.

---

## [SPRINT-N][CRATE] Titre court et concret

**ID :** STORY-NNN  
**Sprint :** N  
**Crate cible :** `apollia-XXX`  
**Fichier(s) cible(s) :** `crates/apollia-XXX/src/xxx.rs`  
**Taille :** S | M | L  
**Dépend de :** STORY-NNN (ou "aucune")  
**Statut :** 🔲 À faire | 🔄 En cours | ✅ Terminée

---

## User Story

```
En tant que [runtime | développeur d'agent | opérateur CLI],
je veux [action précise],
afin de [bénéfice mesurable et concret].
```

<!-- Exemples :
En tant que runtime, je veux un AgentRegistry qui maintient l'état ProcessState de chaque agent,
afin que le TaskRouter puisse vérifier si un agent est ACTIVE avant de router une tâche.

En tant que développeur d'agent, je veux déclarer tools_required dans mon AgentManifest,
afin que le runtime échoue clairement au démarrage si un outil est absent.
-->

---

## Contexte technique

<!-- 2-5 lignes max. Pourquoi cette story existe maintenant, quelle brique elle complète,
     quel principe architectural elle implémente. -->

**Principe(s) architectural(aux) concerné(s) :**
- Principe #N — [nom du principe]

**Position dans l'architecture :**
```
<!-- Petit schéma ASCII si utile pour situer le composant -->
Runtime Core
  └── AgentRegistry  ← cette story
        └── EventBus (dépendance, STORY-001)
```

---

## Critères d'Acceptation

### AC-1 — [Nom du cas principal]

```
ÉTANT DONNÉ [état initial]
QUAND [action]
ALORS [résultat vérifiable]
```

### AC-2 — [Cas d'erreur principal]

```
ÉTANT DONNÉ [état d'erreur]
QUAND [action qui devrait échouer]
ALORS [erreur retournée avec le bon type]
```

### AC-3 — [Cas limite si pertinent]

```
ÉTANT DONNÉ [cas limite]
QUAND [action]
ALORS [comportement attendu]
```

<!-- Règle : minimum 3 AC. Toujours au moins 1 cas d'erreur. -->

---

## Spécification technique

### Types à créer / modifier

```rust
// Nouveaux types à définir dans cette story
pub struct NomStruct {
    pub champ: Type,
    // ...
}

pub enum NomEnum {
    Variant1,
    Variant2 { data: String },
}

// Messages de l'acteur (si acteur Tokio)
enum Message {
    ActionX { param: Type, reply: oneshot::Sender<Result<Type, Error>> },
    ActionY(Type),
    Shutdown,
}
```

### Dépendances Cargo

```toml
# Nouvelles dépendances à ajouter dans crates/apollia-XXX/Cargo.toml
# (laisser vide si aucune nouvelle dépendance)
[dependencies]
nouvelle-dep = "X.Y"
```

### Comportement attendu

<!-- Description en prose du comportement. Inclure :
     - La séquence d'initialisation si acteur
     - Les transitions d'état si machine d'état
     - Les cas de restart/recovery si supervisé -->

### Ce que cette story N'implémente PAS

<!-- Expliciter les limites du scope pour éviter le scope creep -->
- XXX sera implémenté dans STORY-NNN
- La gestion de YYY est hors scope de cette story

---

## Tests requis

### Tests unitaires

```rust
// Dans crates/apollia-XXX/src/xxx.rs ou tests/

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ac1_cas_nominal() {
        // GIVEN
        // WHEN  
        // THEN
        todo!()
    }

    #[tokio::test]
    async fn test_ac2_cas_erreur() {
        todo!()
    }
}
```

### Test d'intégration (si cross-crate)

```rust
// Dans tests/integration/test_xxx.rs
// Requis si la story touche à 2+ crates
```

---

## Definition of Done

**Qualité code :**
- [ ] `cargo test -p apollia-XXX` passe (0 test ignoré)
- [ ] `cargo clippy -p apollia-XXX -- -D warnings` : zéro warning
- [ ] `cargo fmt --check` : code formatté
- [ ] Zéro `unwrap()` dans le code de production
- [ ] Zéro `todo!()` dans le code de production
- [ ] Docstring `///` sur chaque struct, enum, et fonction publique

**Architectural :**
- [ ] Aucune dépendance externe non prévue dans l'architecture
- [ ] Principe(s) architecturaux respectés (voir section Contexte)
- [ ] Pas de régression sur les stories précédentes du sprint

**Documentation :**
- [ ] [items spécifiques à la story]
- [ ] Si décision architecturale notable → entrée dans `docs/Decisions-Log.md`

**Commit :**
- [ ] Commit conventionnel : `feat(apollia-XXX): description concise`

---

## Notes d'implémentation

<!-- Remplir APRÈS implémentation. Laisser vide avant. -->

**Décisions prises pendant l'implémentation :**

**Déviations par rapport à la spec :**

**Dette technique identifiée :**

---

## Liens

- Epic parent : [nom de l'epic]
- Story précédente : STORY-NNN
- Story suivante : STORY-NNN
- ADR associé : ADR-NNN (si décision architecturale documentée)
