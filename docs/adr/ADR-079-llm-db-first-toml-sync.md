# ADR-079 - LLM Backend : DB-first, sync TOML atomique après mutation

**Date :** 2026-04-24
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** Sprint 43 - LLM Backend Management + Model Hub

---

## Contexte

Apollia stocke la configuration LLM dans deux endroits :

1. **`system.db`** - table `llm_backends`, source de toutes les mutations UI (CRUD via `LlmBackendRepository`)
2. **`apollia.toml`** - section `[[llm.backends]]`, lue au démarrage pour construire l'`LlmRouter`

Avant ce sprint, ces deux sources pouvaient diverger : une modification effectuée via la page Settings
(ajout, mise à jour, suppression d'un backend) était persistée en DB mais **jamais propagée vers le TOML**.
Au prochain démarrage de l'app, le `LlmRouter` était reconstruit depuis le TOML - potentiellement obsolète -
plutôt que depuis la DB.

Symptôme concret observé : après un switch Qwen3-235B → Qwen3-30B via l'UI, `apollia.toml` contenait
toujours le chemin du modèle 235B. La RAM était occupée par l'ancien modèle jusqu'au prochain redémarrage
complet de l'application.

Deux stratégies possibles :
- **TOML-first** : TOML est la source de vérité, UI génère le TOML directement
- **DB-first + sync** : DB est la source de vérité, TOML est regénéré depuis la DB après chaque mutation

---

## Décision

Nous adoptons **DB-first avec sync TOML atomique** après chaque mutation.

### Mécanisme

`LlmBackendRepository::sync_to_toml(toml_path: &Path)` :
1. Lit tous les backends depuis SQLite (`SELECT * FROM llm_backends ORDER BY is_default DESC, name ASC`)
2. Charge le contenu brut de `apollia.toml`
3. Supprime tous les blocs `[[llm.backends]]` existants (parsage ligne par ligne - pas de TOML round-trip
   pour préserver les commentaires et la mise en forme des autres sections)
4. Ajoute à la fin les blocs régénérés depuis la DB, précédés d'un commentaire d'avertissement
5. Écrit le résultat atomiquement (`write_all` sur le chemin d'origine)

Cette fonction est appelée en **best-effort** après chaque handler REST LLM :
- `POST /api/v1/llm/backends` (CREATE)
- `PUT /api/v1/llm/backends/:name` (UPDATE)
- `DELETE /api/v1/llm/backends/:name` (DELETE)
- `POST /api/v1/llm/backends/:name/default` (SET-DEFAULT)

Un échec du sync TOML génère un `tracing::warn!` mais ne fait pas échouer la requête HTTP - la DB
reste toujours cohérente, le TOML sera re-synced à la prochaine mutation.

### Rechargement mémoire

`reload_llm_from_db()` (commande IPC Tauri) effectue un **swap atomique** du router :
```rust
let mut guard = shared.write()?;
let old = guard.take();   // retire l'ancien Arc<LlmRouter>
drop(old);                // libère immédiatement si zéro autres clones
*guard = Some(new_router);
```

Si des requêtes en cours détiennent un clone de l'ancien `Arc`, le modèle reste chargé jusqu'à leur
fin - comportement correct et documenté (backpressure naturelle).

---

## Conséquences

### Positives
- **Toujours cohérent après un restart** : le TOML reflète exactement l'état DB
- **Zéro migration nécessaire** : les utilisateurs qui éditaient `apollia.toml` manuellement avant
  voient leurs backends préservés (DB reste source de vérité au runtime, TOML au démarrage)
- **TOML reste commitable** : un développeur peut versionner son `apollia.toml` et voir les backends
  changer après chaque modification UI (utile pour le debugging)
- **Déchargement RAM immédiat** : `drop(old)` libère le modèle GGUF avant le chargement du nouveau

### Négatives
- **Section `[[llm.backends]]` écrasée** : tout commentaire ou formatage custom dans cette section est
  perdu lors du sync. Documenté dans le commentaire généré automatiquement.
- **Double persistance** : légère redondance DB + TOML. Acceptable car le TOML est considéré comme
  un artéfact dérivé de la DB, non l'inverse.
- **Accès disque à chaque mutation** : `sync_to_toml` lit et réécrit le TOML entier. Sur SSD, le coût
  est négligeable (<1 ms pour un fichier <10 KB).

---

## Alternatives écartées

**TOML-first** : aurait nécessité de parser le TOML en mémoire pour chaque mutation UI, de gérer les
conflits entre l'édition manuelle et l'édition UI, et de propager les changements vers la DB. Complexité
disproportionnée - la DB est déjà le bus de persistance de toute la configuration runtime.

**TOML supprimé** : abandonner le TOML pour les backends et tout lire depuis la DB au démarrage.
Rejeté car le TOML reste utile pour le bootstrapping (avant que la DB soit accessible) et pour les
utilisateurs power qui veulent voir/committer leur configuration.
