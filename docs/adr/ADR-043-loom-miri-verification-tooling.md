# ADR-043 - Outillage de vérification concurrence (Loom) et UB (Miri)

**Date :** 2026-07-10
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Chantier :** #9 (vérification Loom / Miri)

---

## Contexte

Le coeur du runtime est un modèle actor (mpsc borné + handle clonable, aucun
`Arc<Mutex>` partagé entre actors, principe #5), et la frontière FFI PyO3
(`apollia-aip`) fait passer des objets entre Rust et Python. Le chantier #9 vise
deux garanties : prouver que les algorithmes concurrents sont sains, et que la
frontière FFI n'introduit pas d'undefined behavior.

Deux réalités techniques contraignent la méthode :

1. **Loom ne peut pas instrumenter Tokio.** Les actors reposent entièrement sur
   `tokio::sync` (`mpsc`, `broadcast`, `oneshot`, `Semaphore`) exécutés sur le
   scheduler Tokio, que Loom ne voit pas. Pire, `--cfg loom` est un flag rustc
   global : Tokio conditionne `tokio::net` derrière `cfg(not(loom))`, donc
   compiler une crate qui dépend de Tokio sous ce flag casse le build
   (hyper-util / axum perdent `tokio::net::UnixStream`).
2. **Miri ne peut pas exécuter la frontière PyO3.** `apollia-aip` ne contient
   aucun bloc `unsafe` écrit à la main ; tout le `unsafe` vient des macros pyo3.
   Miri intercepte les appels de fonction étrangère : `Python::with_gil` appelle
   dans libpython, non supporté. Le seul `unsafe` de production ailleurs est
   `unsafe impl Send/Sync for LoadedWhisper` (bindings whisper.cpp), hors de
   portée de Miri.

Le brief supposait de réécrire les primitives de synchro des actors sous
`cfg(loom)`. Ce n'est pas réalisable pour des actors Tokio. Il faut donc arbitrer
la forme réelle de l'outillage.

## Décision

Adopter Loom et Miri comme outillage de vérification **dev-only**, sans aucune
dépendance runtime ajoutée, avec des frontières de couverture documentées et
honnêtes.

**Loom : modèles abstraits dans une crate exclue.** Une crate autonome
`crates/apollia-loom-models`, exclue du workspace (comme `fuzz/`), sans Tokio
dans son arbre. Chaque modèle réimplémente l'algorithme concurrent d'un actor
avec les primitives Loom et cite le `file:line` de production qu'il reflète. Sept
modèles couvrent : éviction du registry, sémaphore du coordinator, exclusivité de
bail du mailbox, garde de statut terminal du router, latch de force-exit du
shutdown, décision unique du plan gate, fenêtre snapshot/subscribe du drain.
`loom` entre uniquement sous `[target.'cfg(loom)'.dependencies]`.

**Miri : suite de helpers purs, job nightly.** Une suite `miri_pure` dans
`apollia-aip`, en tests unitaires nommés pour un filtrage ciblé
(`cargo +nightly miri test -p apollia-aip --lib miri_pure`), touchant uniquement
des helpers sans interpréteur (arithmétique de date, parsing de chaînes,
composition de namespace). Miri est un composant rustup nightly : aucune crate
ajoutée.

**Frontière d'honnêteté.** Un modèle Loom prouve l'algorithme, pas le code Tokio
exact. Deux modèles (`mailbox_lease_exclusivity`, `shutdown_drain_snapshot_gap`)
modélisent le correctif recommandé d'un défaut que le code de production
n'implémente pas encore ; l'écart est un finding tracé (F3, F4), pas une preuve
sur la production actuelle. Miri couvre les helpers purs ; la frontière PyO3 et
les bindings C restent couverts par les tests d'intégration et la revue SAFETY.

**CI.** Deux jobs advisory dans `nightly.yml` : `loom` (sur le `1.95.0` épinglé,
`--cfg loom`) et `miri` (premier job nightly du dépôt, `nightly` + composant
`miri`). Ni l'un ni l'autre ne bloque une PR.

## Alternatives considérées

- **Réécrire les actors sous `cfg(loom)` (le brief initial).** Rejeté :
  impossible pour des actors Tokio, et `--cfg loom` casse la compilation de tout
  le graphe dépendant de Tokio.
- **`loom` en dev-dependency d'`apollia-runtime`.** Rejeté : vérifié
  empiriquement, `RUSTFLAGS="--cfg loom" cargo test -p apollia-runtime` échoue à
  la compilation (perte de `tokio::net`). D'où la crate exclue.
- **ThreadSanitizer / `shuttle`.** Écartés pour ce chantier : TSan ne donne pas
  l'exhaustivité de Loom sur les petits algorithmes ; `shuttle` couvre l'async
  mais ajoute une dépendance et une instrumentation plus lourde. Réexaminables si
  un besoin d'entrelacement async réel apparaît.
- **Extraire les helpers purs d'`apollia-aip` dans une crate sans pyo3** pour un
  Miri plus large. Non retenu maintenant (refactor hors périmètre) ; noté comme
  repli si un jour Miri ne peut plus compiler `apollia-aip`.

## Conséquences

Positives :

- Les invariants concurrents critiques (garde de statut terminal, sémaphore,
  décision unique du gate HITL) sont prouvés race-free sur leur algorithme.
- La suite Miri valide l'absence d'UB sur le code Rust pur proche du FFI.
- Aucune dépendance runtime ; impact nul sur le build normal (crate Loom exclue,
  helpers Miri = tests rapides also exécutés par `cargo test`).
- Deux findings (F3 drain, F4 bail mailbox) sont désormais adossés à un modèle du
  correctif recommandé.

Négatives / coûts :

- Les modèles Loom sont abstraits : ils peuvent diverger du code de production si
  un actor évolue sans mettre à jour le modèle. Mitigation : chaque modèle cite
  le `file:line` reflété.
- Miri exige une toolchain nightly (premier usage nightly récurrent du dépôt hors
  fuzz).
- Loom demande une invocation `RUSTFLAGS` dédiée, hors des gates de PR standard.
