# ADR-046 - Preuves Kani des invariants cardinaux et fence de bail du mailbox

**Date :** 2026-07-13
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Chantier :** #10 (preuves formelles Kani)

---

## Contexte

Deux invariants conditionnent la crédibilité du runtime et doivent tenir sous
tout ordre d'opérations, pas seulement sur un échantillon :

1. **Budget non contournable** (principe #7). `StepBudget`
   (`crates/apollia-oria/src/budget.rs`) ne doit jamais laisser `used > cap` sur
   une dimension, quelle que soit la séquence d'incréments.
2. **Exclusivité de bail du mailbox.** Un message acquitté est supprimé, un bail
   expiré redevient livrable, et un `ack` périmé ne doit jamais supprimer un
   message re-loué à un autre consommateur.

Les tests échantillonnent ; un model-checker (Kani, bit-precis, AWS) prouve sur
tout un espace borné. Deux réalités ont contraint la méthode :

1. **Kani ne tourne pas sur les machines de dev.** Kani lie sa propre toolchain
   via rustup, absent des machines Homebrew du dépôt. Une preuve locale n'est
   donc pas possible ; il faut un repli exécutable localement.
2. **Kani ne modélise ni `Instant` ni SQLite ni Tokio.** La dimension wall-clock
   du budget repose sur `Instant::now`, et l'état du mailbox vit dans SQLite. Le
   model-checker ne peut prouver que de la logique pure.

L'exploration a de plus confirmé deux défauts réels :

- **C9-F4** : `handle_ack` supprimait par `(message_id, to_agent)` sans aucun
  fence de propriétaire de bail. Un consommateur périmé, dont le bail avait
  expiré puis été re-loué, pouvait supprimer le message qu'un second consommateur
  traitait.
- **Débordement d'incrément budget** : `prev + 1` dans `increment_*` est un `+`
  vérifié qui panique en debug à `u32::MAX`.

## Décision

Adopter Kani comme outillage de preuve **dev-only** (aucune dépendance runtime),
réservé aux invariants cardinaux, avec un miroir proptest exécutable localement,
et corriger les deux défauts que les preuves exposent.

**Extraction en logique pure prouvable.** La décision embarquée dans les méthodes
est extraite en helpers purs cités ligne à ligne : côté budget `effective_cap`,
`dimension_exhausted`, `remaining` ; côté mailbox `is_deliverable`,
`owner_matches` et les transitions de bail. Chaque helper réencode exactement un
prédicat de production, et les méthodes de production l'appellent (pas de modèle
divergent). Les harnesses `#[cfg(kani)]` prouvent ces helpers ; le vrai
`StepBudget` atomique et le vrai store SQLite sont adossés au modèle par les
tests proptest et le test de régression bout en bout
(`test_ack_fenced_to_lease_owner`).

**Fence de bail du mailbox (correctif C9-F4).** Ajout d'une colonne
`lease_owner`, positionnée au `run_id` qui loue lors du `receive`, et fence
null-safe `lease_owner IS ?` sur `ack` et `nack`. Un `ack`/`nack` dont le
`run_id` diffère du propriétaire courant agit sur zéro ligne. Le `run_id` circule
déjà vers `receive` et `ack` ; `nack` le reçoit désormais par le même canal
interne, sans changement du contrat SDK Python (`nack(message_id)` inchangé, le
`run_id` est injecté côté Rust). Une migration `ALTER TABLE ... ADD COLUMN`
idempotente couvre les stores existants.

**Correctif de débordement budget.** `prev + 1` devient `prev.saturating_add(1)`,
identique pour tout état atteignable, sans panique à `u32::MAX`, et prouvable sans
`assume`.

**CI.** Un job advisory `kani` dans `nightly.yml` installe `kani-verifier`
(`cargo install --locked kani-verifier && cargo kani setup`) et lance
`cargo kani -p apollia-oria` et `cargo kani -p apollia-runtime`. Il ne bloque
jamais une PR.

## Alternatives considérées

- **Jeton de bail explicite (retourné par `receive`, exigé à l'`ack`).** Rejeté
  pour ce chantier : garantie plus forte mais churn transverse (signatures
  `receive`/`ack`, événement `AgentMessageDelivered`, route axum, SDK Python).
  Conservé comme évolution possible si le résidu `None`/`None` doit être fermé.
- **Fence sur seule validité du bail (`lease_until > now` à l'ack).** Rejeté : ne
  ferme pas la course. Après re-bail, le bail est de nouveau valide, donc un ack
  périmé passerait encore.
- **Prouver le `StepBudget` atomique complet sous Kani.** Rejeté : `Instant::now`,
  `tokio::sync::watch` et le temps ne sont pas modélisables. Kani prouve les
  helpers purs ; le reste est adossé par proptest.
- **S'en tenir aux modèles Loom (chantier #9).** Insuffisant seul : Loom prouve un
  entrelacement d'algorithme abstrait ; Kani ajoute la preuve bit-precise du
  prédicat exact de fence, et le budget n'avait aucun modèle Loom.

## Conséquences

Positives :

- Le cap non contournable, la stabilité de l'épuisement et l'absence de
  débordement d'incrément sont prouvés sur tout le domaine `u32`.
- L'exclusivité de bail (C9-F4) est corrigée en production et prouvée : un
  consommateur périmé ne peut plus supprimer un message re-loué.
- Miroir proptest vert sous `cargo test` ; le test de régression échoue contre le
  code non fencé et passe une fois fencé.
- Aucune dépendance runtime ; le job Kani est isolé et advisory.

Négatives / coûts :

- Les preuves ne valent que sur leur espace **borné** (tout `u32` pour le budget ;
  identités et temps Unix bornés par `kani::assume` pour le mailbox). Ne pas
  survendre "prouvé correct" au-delà.
- Deux points restent hors preuve : la dimension wall-clock du budget (`Instant`)
  et la collision `None`/`None` de propriétaire (deux baux sans `run_id` ne sont
  pas mutuellement fencés). Documentés comme tels.
- Kani exige rustup : preuve en CI seulement, jamais en local sur ce dépôt.
