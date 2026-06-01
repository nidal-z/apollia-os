# ADR-075 - Chargement de modèles GGUF multi-fichiers (shards)

**Date :** 2026-04-19
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 41 - GGUF Multi-File Support

---

## Contexte

Les modèles LLM open-weights de grande taille (>30 GB quantisés, ex : Llama-70B
Q5_K_M ≈ 49 GB, Mixtral-8x22B, DeepSeek-V3 ≈ 400 GB) sont distribués shardés
en plusieurs fichiers GGUF. Trois raisons concrètes :

1. **Limites filesystem.** FAT32 plafonne à 4 GB par fichier, exFAT et la
   plupart des partages réseau posent d'autres seuils pratiques. Un GGUF
   monolithique n'est pas transportable.
2. **Téléchargement incrémental.** Un download interrompu sur 400 GB doit
   pouvoir reprendre fichier par fichier - pas redémarrer de zéro.
3. **Convention écosystème.** HuggingFace, Ollama, `llama-quantize --split`
   et la quasi-totalité des hôtes de modèles respectent le pattern standard
   llama.cpp `<prefix>-NNNNN-of-MMMMM.gguf` (5 chiffres zero-padded).

Avant Sprint 41, `EmbeddedBackendConfig` acceptait uniquement un
`model_path: PathBuf` et `EmbeddedBackend::load()` appelait
`LlamaModel::load_from_file` sur ce chemin unique. Un opérateur qui
téléchargeait un modèle shardé ne pouvait donc pas le charger sans
ré-assemblage manuel - ré-assemblage qui est techniquement impossible
(les shards GGUF ont des en-têtes distincts, une concat brute produit un
fichier invalide).

Par ailleurs, certains hôtes (forks HuggingFace, conversions communautaires)
produisent des splits au naming non standard, auxquels le pattern
`-NNNNN-of-NNNNN` ne s'applique pas. Il faut pouvoir les charger aussi.

La décision doit être prise maintenant car le support multi-file bloque
l'usage de la classe de modèles 70B+ sur Apollia - classe qui cible
précisément les usages où le local-first apporte le plus de valeur (coûts
cloud prohibitifs, latence réseau, confidentialité).

---

## Décision

Apollia supporte le chargement de modèles GGUF multi-fichiers via **deux
modes exclusifs**, sans introduire de format manifest propriétaire :

1. **Mode standard (automatique).** L'opérateur renseigne `model_path` et
   pointe sur le premier shard (`-00001-of-NNNNN.gguf`). `EmbeddedBackend`
   détecte le pattern, valide la complétude de la série au démarrage
   (Principe #4 - Fail fast), puis délègue à `LlamaModel::load_from_file`
   qui charge automatiquement les shards suivants via
   `llama_load_model_from_file`.

2. **Mode custom (explicite).** L'opérateur renseigne
   `model_paths: Vec<PathBuf>` (liste ordonnée), mutuellement exclusif avec
   `model_path`. `EmbeddedBackend` valide l'existence de chaque chemin puis
   appelle `llama_model_load_from_splits` via FFI direct
   (`llama-cpp-sys-2`), contournant l'absence de wrapper dans
   `llama-cpp-2 0.1.140`.

Les deux champs sont mutuellement exclusifs et exactement un des deux est
requis : la violation de cette règle déclenche `LlmError::ConfigConflict`
au démarrage.

---

## Alternatives considérées

### Option A - Format manifest Apollia (`.gguf.apollia.toml`) (rejetée)

**Pour :** Unifie les deux modes derrière un seul fichier. Permet d'ajouter
des métadonnées Apollia propres (checksum, version, origine).

**Contre :** (1) Introduit un format propriétaire qui désaligne Apollia
de l'écosystème llama.cpp, alors que tout le bénéfice du Principe #2
(zéro dépendance externe) passe par l'alignement avec les outils existants.
(2) Oblige l'opérateur à maintenir un fichier en plus de ses shards, pour
zéro information nouvelle que le nommage ne porte pas déjà.
(3) Duplique l'information de la liste de shards à deux endroits
(manifest + disque), avec risque de désynchronisation silencieuse.

### Option B - Renommage / concaténation manuelle (rejetée)

**Pour :** Pas de code Apollia à écrire.

**Contre :** (1) Techniquement faux - les shards GGUF ont des en-têtes
distincts, la concat produit un fichier invalide. (2) Friction inacceptable
pour un modèle de 400 GB. (3) Ne règle pas le cas naming custom.
(4) Punit le 99% de cas standard pour simplifier 1% de cas edge.

### Option C - Téléchargement automatique des shards manquants (rejetée)

**Pour :** Expérience zéro-friction : Apollia détecte les shards absents
et les pull depuis HuggingFace.

**Contre :** Viole frontalement le Principe #1 (local-first) en introduisant
du trafic réseau implicite au démarrage. Ré-examinable plus tard dans une
commande dédiée `apollia model pull` avec consentement explicite
- hors scope Sprint 41.

### Option D - `model_paths` obligatoire partout (rejetée)

**Pour :** Un seul code path à maintenir, pas de branche détection standard.

**Contre :** (1) Casse toutes les configs utilisateur existantes pour un
gain nul. (2) Ajoute du bruit (`model_paths = ["file.gguf"]`) dans le
cas mono-fichier qui concerne 95% des usages. (3) Fait porter à
l'opérateur le coût de la généralité.

### Option retenue - Double mode `model_path` (auto-détection) / `model_paths` (explicite)

**Pour :**
- Les modèles shardés standard fonctionnent sans changement de config
  au-delà de pointer sur le premier shard - zéro friction supplémentaire
  par rapport au mono-fichier.
- Les naming customs restent adressables sans format propriétaire.
- Rétrocompatibilité bit-pour-bit sur les configs mono-fichier
  existantes.
- Alignement 1:1 avec la convention llama.cpp - les outils écosystème
  (`llama-quantize --split`, convertisseurs HF→GGUF) produisent du
  contenu directement utilisable.

**Compromis acceptés :**
- `EmbeddedBackendConfig` a deux champs `Option` au lieu d'un
  `PathBuf` unique - un peu plus de bruit côté serde, compensé par la
  validation fail-fast.
- Un bloc `unsafe` isolé reste nécessaire pour
  `llama_model_load_from_splits` tant que `llama-cpp-2` upstream ne
  wrap pas cette fonction (une PR upstream est envisageable mais hors
  scope Sprint 41).

---

## Conséquences

**Positives :**
- Les modèles shardés téléchargés depuis HuggingFace, Ollama ou
  autres hôtes fonctionnent sans action manuelle au-delà de placer
  les fichiers dans `~/.apollia/models/`.
- Aucune nouvelle dépendance Cargo : `llama-cpp-sys-2` est déjà
  transitif via `llama-cpp-2 = "0.1.140"`.
- Erreurs orientées correction au démarrage
  (`LlmError::ModelShardMissing`, `LlmError::ShardIndexNotFirst`,
  `LlmError::ConfigConflict`), chacune indiquant précisément la
  contremesure à appliquer (Principe #4).
- `apollia model list` agrège un groupe de shards en une ligne unique
  `3 shards` avec flag `(INCOMPLETE)` si la série est cassée -
  l'opérateur voit immédiatement l'état de ses modèles.

**Négatives / Compromis :**
- `EmbeddedBackendConfig.model_path` passe de `PathBuf` à
  `Option<PathBuf>` - changement breaking pour du code Rust externe
  qui construirait la config manuellement. Les configs TOML
  utilisateur ne sont pas affectées (serde tolère l'absence d'un
  champ `Option` avec `#[serde(default)]`).
- Un bloc `unsafe` FFI reste dans `backends/embedded.rs` jusqu'à ce
  que `llama-cpp-2` upstream wrap `llama_model_load_from_splits`.
  Le bloc est isolé, les `CString` sont validées, les retours nuls
  sont convertis en `LlmError::InferenceError`.
- Un shard corrompu (taille attendue mais contenu tronqué par un
  download interrompu) n'est détecté que par llama.cpp à l'ouverture,
  pas au démarrage. Dette acceptée - la vérification d'intégrité
  demanderait un checksum qui n'est pas porté par le pattern de
  nommage standard, et qui est le travail d'une future commande
  dédiée `apollia model verify`.

**À surveiller :**
- Évolution de `llama-cpp-2` upstream : si un wrapper
  `llama_model_load_from_splits` est livré, supprimer le bloc `unsafe`
  dans `EmbeddedBackend::load`.
- Apparition de naming schemes non-standard répandus dans
  l'écosystème : si un pattern concurrent émerge (ex : HuggingFace
  `.part1.gguf` / `.part2.gguf`), étendre la détection au lieu de
  rester sur le seul pattern `-NNNNN-of-NNNNN`.
- Signalement opérateur : si un utilisateur pointe régulièrement sur
  `-00002-of-NNNNN` par confusion, ajouter à l'erreur
  `ShardIndexNotFirst` une suggestion du chemin correct (aujourd'hui
  elle indique seulement que le premier shard est attendu).

---

## Principes architecturaux impactés

- **Principe #1 - Local-first :** respecté. Aucun trafic réseau
  implicite ; l'opérateur fournit les fichiers, Apollia valide et
  délègue à llama.cpp en local.
- **Principe #2 - Zéro dépendance externe :** respecté et renforcé.
  Aucune nouvelle dépendance Cargo ; pas de format propriétaire ;
  alignement 1:1 avec la convention llama.cpp.
- **Principe #4 - Fail fast :** respecté. Les shards manquants, un
  index non-premier, ou la présence conjointe de `model_path` et
  `model_paths` déclenchent une erreur typée au démarrage, avant tout
  appel FFI vers llama.cpp.
- **Principe #8 - CLI humaine, API machine :** respecté.
  `apollia model list` présente un groupe de shards en une entrée
  unique avec `shard_count` / `total` ; `apollia model list --json`
  émet un objet `{"kind": "split", ...}` machine-parseable.

---

## Liens

- Story associée : STORY-515, STORY-516, STORY-517, STORY-518, STORY-519
- Sprint : [sprint-41/plan.md](../internal/STORIES/sprint-41/plan.md)
- Décision parente : ADR-020 - apollia-llm moteur embarqué (llama.cpp, feature flags, backends cloud)
- Décision parente : ADR-047 - Multi-LLM Backend Registry (SQLite)
- Référence upstream llama.cpp : `llama.cpp/include/llama.h` -
  `llama_load_model_from_file` + `llama_model_load_from_splits`
- Référence upstream `llama-cpp-2` : <https://github.com/utilityai/llama-cpp-rs>
