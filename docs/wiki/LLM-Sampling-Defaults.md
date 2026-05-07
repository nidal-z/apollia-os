# Sampling Defaults — Résolution par modèle

> *Chaque famille de modèle a ses paramètres de sampling officiels (`temperature`, `top_p`, `top_k`, `repetition_penalty`). Apollia les applique automatiquement, fait remonter les défauts officiels HuggingFace au téléchargement, et laisse l'opérateur surcharger localement.*

---

## 1. Pourquoi

Un sampler purement déterministe (`greedy`) produit deux fois la même sortie pour la même entrée — incompatible avec un agent qui doit explorer plusieurs angles d'analyse au cours du temps. Un sampler stochastique avec température 0.7 / top-p 0.95 donne du variant mais ignore les recommandations propres à chaque famille (Qwen3 préfère `top_p=0.8 top_k=20`, Llama 3 préfère `temperature=0.6 top_p=0.9`, Phi-3 préfère `temperature=0.5`, etc.).

Apollia résout les paramètres de sampling **par modèle** au moment de chaque appel d'inférence, à partir de quatre sources superposées avec une précédence stricte.

---

## 2. Précédence de résolution

```
┌──────────────────────────────────────────────────────────────┐
│ 1. req.temperature explicite (caller)                        │ ← gagne tout
├──────────────────────────────────────────────────────────────┤
│ 2. ~/.apollia/models/sampling-defaults.json (user overrides) │
│    ↑ écrit auto à chaque download HF (generation_config.json)│
├──────────────────────────────────────────────────────────────┤
│ 3. embedded.toml — table curated dans le binaire             │
│    match par GGUF general.architecture + general.name        │
├──────────────────────────────────────────────────────────────┤
│ 4. DEFAULT_TEMPERATURE / TOP_P / TOP_K — fallback global     │
└──────────────────────────────────────────────────────────────┘
```

La résolution se fait **champ par champ** avec `fill_missing` : un override utilisateur qui ne renseigne que `temperature` laisse `top_p` et `top_k` venir de la table embedded ; aucune source n'est tout-ou-rien.

| Source | Quand elle gagne | Implémentation |
|---|---|---|
| `CompletionRequest.temperature` | Toujours, si fournie | `req.temperature.or(resolved.temperature)` |
| User override | Modèle présent dans `sampling-defaults.json` | `UserOverrides::lookup(keys)` |
| Embedded table | Architecture GGUF reconnue | `embedded_lookup(arch, name_hints)` |
| Hard fallback | Aucune source ne renseigne le champ | `DEFAULT_TEMPERATURE = 0.7`, `DEFAULT_TOP_P = 0.95`, `DEFAULT_TOP_K = 40` |

---

## 3. Types publics — `apollia_llm::model_defaults`

```rust
/// Paramètres de sampling. Tous champs Option pour permettre la fusion
/// champ par champ (`fill_missing`).
pub struct ModelDefaults {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub repetition_penalty: Option<f32>,
}

/// Indices fournis par l'appelant pour matcher un modèle.
pub struct ModelHints<'a> {
    pub arch: Option<&'a str>,        // GGUF general.architecture
    pub name: Option<&'a str>,        // GGUF general.name
    pub file_name: Option<&'a str>,   // ex. "Qwen3-30B-A3B-Q4_K_M.gguf"
    pub repo_id: Option<&'a str>,     // ex. "Qwen/Qwen3-30B-A3B"
    pub model_id: Option<&'a str>,    // identifiant logique apollia
}

/// Map persistée sur disque.
pub struct UserOverrides { /* HashMap<String, ModelDefaults> */ }

impl UserOverrides {
    pub fn default_path() -> PathBuf;       // ~/.apollia/models/sampling-defaults.json
    pub fn load(path: &Path) -> io::Result<Self>;
    pub fn upsert(path: &Path, key: &str, defaults: ModelDefaults) -> io::Result<()>;
    pub fn lookup(&self, keys: &[&str]) -> ModelDefaults;
}

/// Résout les defaults en combinant override utilisateur + table embarquée.
pub fn resolve(hints: &ModelHints<'_>, overrides: &UserOverrides) -> ModelDefaults;
```

---

## 4. Table embarquée — `embedded.toml`

11 entrées curated shippées dans le binaire, sourcées des `generation_config.json` officiels publiés sur HuggingFace par les éditeurs.

| Famille | `arch_pattern` | `name_pattern` | `temperature` | `top_p` | `top_k` |
|---|---|---|---|---|---|
| Qwen3 (thinking) | `qwen3` | `thinking` | 0.6 | 0.95 | 20 |
| Qwen3 (instruct) | `qwen3` | — | 0.7 | 0.8 | 20 |
| Qwen2.5 | `qwen2` | — | 0.7 | 0.8 | 20 (`rep_penalty=1.05`) |
| Llama 3.1+ | `llama` | `llama-3` | 0.6 | 0.9 | — |
| Llama 2 | `llama` | `llama-2` | 0.6 | 0.9 | — |
| Mistral Instruct | `llama` | `mistral` | 0.7 | 0.95 | — |
| Mixtral | `llama` | `mixtral` | 0.7 | 0.95 | — |
| Phi-3 | `phi3` | — | 0.5 | 0.95 | 40 |
| Gemma 2 | `gemma2` | — | 0.95 | 0.95 | 64 |
| Gemma 3 | `gemma3` | — | 1.0 | 0.95 | 64 |
| DeepSeek R1 | `deepseek2` | `r1` | 0.6 | 0.95 | — |
| DeepSeek V3 | `deepseek2` | — | 0.7 | 0.95 | — |

**Règles de matching :**
- `arch_pattern` est testé contre `general.architecture` du GGUF (lower-case, exact OU prefix terminé par `*`).
- `name_pattern` est testé contre `general.name` ou le filename GGUF (lower-case, substring).
- Précédence : entrées plus haut dans le TOML l'emportent — les plus spécifiques en haut (Qwen3 Thinking avant Qwen3 generic).

**Source des valeurs :** chaque entrée du TOML cite l'URL HF de son `generation_config.json` source. Les valeurs numériques sont des faits non-copyrightables (Feist v. Rural / directive 96/9/CE).

---

## 5. Override utilisateur

Fichier : `~/.apollia/models/sampling-defaults.json`

Format JSON aplati `{ "<clé>": ModelDefaults }`. La clé peut être un `repo_id` (`Qwen/Qwen3-30B-A3B`), un `model_id` interne, un `file_name` GGUF, ou n'importe quel hint passé via `ModelHints`.

```json
{
  "Qwen2.5-Coder-7B-Instruct-IQ2_M.gguf": {
    "temperature": 0.7,
    "top_p": 0.8,
    "top_k": 20,
    "repetition_penalty": 1.05
  },
  "Qwen3-30B-A3B-Q4_K_M.gguf": {
    "temperature": 1.5,
    "top_p": 0.99,
    "top_k": 100,
    "repetition_penalty": null
  }
}
```

Le fichier est lu à chaque appel `complete()` / `stream()` (~5 KB, coût négligeable). Toute modification est prise en compte au prochain appel — pas de cache à invalider.

`UserOverrides::upsert` écrit atomiquement (write-then-rename) ; un échec d'écriture ne corrompt pas le fichier existant.

> **Erreurs.** Un fichier présent mais JSON corrompu lève `io::ErrorKind::InvalidData` côté Rust et est loggé en `warn` côté backend embedded — la résolution retombe alors silencieusement sur la table embarquée. Le fichier absent est traité comme une map vide (pas d'erreur).

---

## 6. Auto-fetch HuggingFace au téléchargement

Quand un modèle est téléchargé via Apollia (Hub modèles desktop, CLI, ou route HTTP), le `repo_id` HF est propagé jusqu'au downloader. À la fin du téléchargement, `persist_sampling_defaults` :

1. Fetch `https://huggingface.co/{repo_id}/resolve/main/generation_config.json`.
2. Si le repo direct n'a pas le fichier (cas standard pour Bartowski, Unsloth, mradermacher — quanteurs qui republient seulement le GGUF), résout le **base model** :
   - lit `cardData.base_model` de `/api/models/{repo_id}` ;
   - sinon parse les tags `base_model:org/name` (préfère un tag simple ; retombe sur `base_model:quantized:org/name` en dernier recours).
3. Retry `get_generation_config` sur le base model.
4. Convertit `f64 → f32`, écrit dans `~/.apollia/models/sampling-defaults.json` indexé par filename GGUF.

L'opération est best-effort : un échec (HF down, repo sans `generation_config.json` ni `base_model`) est loggé `info` mais ne fait jamais échouer le téléchargement.

**Logs à surveiller :**

```
INFO model download completed
INFO generation_config.json absent du repo dérivé, retry sur base_model base_model="Qwen/Qwen2.5-Coder-7B-Instruct"
INFO sampling defaults HF persistés repo="bartowski/Qwen2.5-Coder-7B-Instruct-GGUF" file="Qwen2.5-Coder-7B-Instruct-IQ2_M.gguf" path="/Users/x/.apollia/models/sampling-defaults.json"
```

---

## 7. Indexation par filename — pourquoi pas par repo_id

Un même repo HF (`Qwen/Qwen3-30B-A3B-GGUF`) shippe plusieurs quantisations (Q4_K_M, Q5_K_M, Q8_0…). L'opérateur peut télécharger plusieurs d'entre elles. Indexer par filename permet :

- chaque quantisation reçoit son entrée — l'opérateur peut finetuner les params différemment selon la quant ;
- au reload d'un modèle, le backend matche directement sur `model_id` (filename sans extension) sans avoir à connaître le repo source.

Les entrées portent des hyperparamètres identiques au moment de l'écriture (toutes proviennent du même `generation_config.json` upstream), mais peuvent diverger ensuite si l'opérateur en édite une.

---

## 8. Intégration backend embedded

`EmbeddedBackend::resolve_sampler_defaults()` est appelé au début de chaque `complete()` / `stream()` :

```rust
let resolved = self.resolve_sampler_defaults();
let temperature = req.temperature.or(resolved.temperature);
let top_p = resolved.top_p;
let top_k = resolved.top_k;
let seed = req.seed;
let sampler = build_tail_sampler(temperature, top_p, top_k, seed);
```

`build_tail_sampler` :
- `temperature == Some(0.0)` → `LlamaSampler::greedy()` (déterministe, ignore seed) ;
- sinon → chaîne `top_k → top_p → temp → dist(seed)`. Seed dérivée de l'horloge nanoseconde si `req.seed` est `None`. La seed effective est tracée à `debug` :
  ```
  DEBUG embedded sampler stochastique seed=1778163098044545000 temperature=0.7 top_p=0.8 top_k=20
  ```
  → utile pour replay manuel : récupérer la seed dans les logs et la passer via `req.seed`.

Voir [Briques-LLM-Backend §6](./Briques-LLM-Backend) pour le détail du backend.

---

## 9. Légalité

Les valeurs publiées dans les `generation_config.json` officiels sont des paramètres numériques recommandés par les éditeurs — des **faits non-copyrightables** au sens de Feist v. Rural (US) et de la directive 96/9/CE (UE). La table embarquée cite la source (URL HF) pour chaque entrée et n'inclut que les valeurs numériques, pas le code accompagnant.

Le **modèle lui-même** reste sous sa licence (Llama Community License, Qwen License, Gemma Terms, Apache 2.0 selon les cas) — Apollia n'en redistribue rien : l'opérateur télécharge directement depuis HF, on ne fait que lire les hyperparamètres associés.

---

## 10. Tests et debug

```bash
# Vérifier le contenu actuel des overrides
cat ~/.apollia/models/sampling-defaults.json

# Forcer une seed reproductible (caller-side, via CompletionRequest.seed)
# Pas exposé via la CLI : test en code Rust ou via l'API HTTP llm/complete.

# Activer les logs de résolution
RUST_LOG=apollia_llm=debug apollia-desktop
# → cherche "embedded sampler stochastique" dans les logs

# Tests unitaires du module
cargo test -p apollia-llm --lib model_defaults     # 10 tests
cargo test -p apollia-llm --lib hf_registry        # 7 tests (dont 6 sur extract_base_model)
```

---

## 11. Voir aussi

- [Briques-LLM-Backend §6 EmbeddedBackend](./Briques-LLM-Backend) — sampler, max_tokens, n_ctx clamping
- [Briques-Desktop §3 Commandes Tauri IPC](./Briques-Desktop) — `start_model_download` avec `repo_id`
- [Outils-Reference](./Outils-Reference) — outils filesystem (expansion `~`)
