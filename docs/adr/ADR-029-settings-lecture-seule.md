# ADR-029 — Settings lecture seule dans l'application desktop

**Date :** 2026-03-13
**Statut :** Accepte
**Contexte :** Sprint 15 — STORY-149 (Vue Settings)

---

## Probleme

L'application desktop doit permettre a l'utilisateur de consulter la configuration Apollia OS (`apollia.toml`). La question est : doit-on permettre l'edition in-app ou deleguer a un editeur externe ?

## Decision

La vue Settings est **lecture seule**. L'edition de la configuration est deleguee a l'editeur natif du systeme via `open::that(config_path)`.

## Justification

Le round-trip TOML (parse → modifier → serialiser) **detruit les commentaires utilisateur** et reorganise les sections. La crate `toml` de Rust ne preserve pas les commentaires lors de la deserialisation/re-serialisation. Utiliser `toml_edit` ajouterait de la complexite pour un cas d'usage marginal.

**Alternatives evaluees :**

| Option | Avantages | Inconvenients |
|---|---|---|
| **A — Lecture seule + editeur natif** (choisi) | Zero risque de perte de commentaires, simple | Necessite redemarrage apres modification |
| B — Edition in-app avec `toml_edit` | UX integree | `toml_edit` complexe, risque de bugs, commentaires fragiles |
| C — Edition partielle (quelques champs) | Compromis | Surface de bugs, incoherence UX (certains champs editables, d'autres non) |

## Implementation

- `get_config()` : lit `apollia.toml`, retourne une vue structuree `ApollaConfigView` (struct Rust plate, pas de round-trip)
- `open_config_in_editor()` : appelle `open::that(config_path)` pour ouvrir dans l'editeur systeme
- Frontend affiche les sections par categorie avec liens vers les vues dediees (/llm, /triggers)
- Message informatif : "Pour modifier, editez apollia.toml et redemarrez."

## Consequences

- L'utilisateur doit utiliser un editeur externe pour modifier la configuration
- Le redemarrage du runtime est necessaire pour appliquer les changements
- Aucun risque de corruption ou perte de commentaires dans le fichier TOML
- La complexite du frontend reste minimale (pas de formulaires d'edition)
