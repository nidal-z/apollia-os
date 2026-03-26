# ADR-042 — Remplacement de mistral.rs par llama.cpp (lié statiquement) comme moteur d'inférence GGUF

**Date :** 2026-03-26
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 25

---

## Contexte

Le moteur d'inférence GGUF embarqué dans `apollia-llm` utilise `mistralrs` v0.7 (Rust pur via
candle). En phase QA du sprint 24, trois problèmes bloquants ont été identifiés :

1. **16 architectures GGUF supportées seulement** — les modèles Qwen3.5 (`qwen35`),
   GLM-4.7 (`glm4moe`), et Llama 4 Scout ne sont pas reconnus. Cela limite
   drastiquement le choix de modèles proposés aux utilisateurs.

2. **Crash Metal sur les modèles MoE** — le kernel `indexed_moe_forward` n'est pas
   implémenté dans candle-metal. Tout modèle MoE (Qwen3-30B-A3B, Qwen3.5-35B-A3B,
   GLM-4.7) provoque un panic irrémédiable empoisonnant le KV cache.

3. **Streaming limité** — `mistralrs::Model::Stream<'a>` porte un lifetime non-`'static`,
   impossible à transférer dans un `tokio::spawn`. Le backend actuel retourne le contenu
   complet en un seul chunk (fallback documenté dans `embedded.rs`).

En parallèle, **llama.cpp** est la référence communautaire pour l'inférence GGUF :
- 30+ architectures GGUF supportées, dont `qwen35`, `glm4moe`, `llama4`
- Kernels Metal natifs pour MoE, flash-attention, quantized GEMM
- Streaming token-by-token natif
- Communauté de centaines de contributeurs, support de nouveaux modèles en jours

Le crate `llama-cpp-2` (bindings Rust safe) permet un lien statique préservant le
binaire unique (Principe #2). Le trait `CompletionModel` (ADR-020) isole complètement
le changement dans `apollia-llm::backends::embedded`.

**Note :** L'ADR-041 avait identifié un "conflit ggml potentiel futur si llama.cpp intégré
directement". Ce conflit est désormais résolu : whisper.cpp (STT) et llama.cpp (LLM)
partagent le même format ggml mais n'ont pas de symboles en conflit dans leurs versions
actuelles. Les deux se compilent statiquement dans le même binaire.

## Décision

Nous remplaçons `mistralrs` + `mistralrs-core` par `llama-cpp-2` (bindings safe Rust
pour llama.cpp) comme moteur d'inférence GGUF embarqué dans `apollia-llm`.

Le changement est strictement contenu dans :
- `apollia-llm/Cargo.toml` — remplacement des dépendances
- `apollia-llm/src/backends/embedded.rs` — réécriture du `EmbeddedBackend` (~200 lignes)
- `Cargo.toml` workspace — dépendances `[workspace.dependencies]`

Le trait `CompletionModel`, le `LlmRouter`, les backends cloud, le config TOML, et
l'ensemble de l'API publique de `apollia-llm` ne changent pas.

### Détails techniques

1. **Chargement du modèle** : `llama_cpp_2::LlamaModel::load_from_file()` remplace
   `GgufModelBuilder::new().build()`. Configuration GPU Metal via `LlamaModelParams`.

2. **Inférence** : `LlamaContext` + tokenization via `model.str_to_token()` +
   `llama_decode()` remplace `Model::send_chat_request()`. Le chat template est
   appliqué via `llama_chat_apply_template()` (support natif Jinja).

3. **Streaming** : Implémentation token-by-token native via la boucle de décodage
   llama.cpp. Chaque token décodé est émis comme `StreamChunk::Text`.

4. **Feature flags** : Les features existantes `local`, `local-metal`, `local-cuda`
   sont préservées et redirigées vers les features `llama-cpp-2` correspondantes.

## Alternatives considérées

### Option A — Attendre mistral.rs 0.8+ (rejetée)

**Pour :** Zéro effort, cohérence Rust pur.
**Contre :** Délai inconnu. Pas de garantie sur le support Metal MoE (dépend de candle
upstream). `mistralrs-core` v0.7.0 publiée le 28/01/2026, aucune version 0.7.1 ou 0.8
depuis 2 mois. Le support GGUF `qwen35` n'a été ajouté dans aucun commit du repo.

### Option B — llama-server comme processus externe (rejetée)

**Pour :** Zéro modification du crate `apollia-llm`. Support immédiat via OpenAI-compatible API.
**Contre :** **Viole le Principe #2** (zéro dépendance externe). Nécessite de distribuer
un binaire séparé `llama-server`, gérer son lifecycle (spawn, health check, kill),
et ajouter un mode d'échec supplémentaire. Incompatible avec le modèle binaire unique.

### Option C — Contribuer les kernels Metal MoE à candle (rejetée)

**Pour :** Résout le problème à la racine dans l'écosystème Rust.
**Contre :** Effort disproportionné (3-6 mois). Nécessite une expertise MSL (Metal Shading
Language) avancée. Ne résout pas le problème des 16 architectures GGUF manquantes.
Dépendance sur l'acceptation upstream de la PR.

### Option retenue — llama.cpp lié statiquement via `llama-cpp-2`

**Pour :**
- 30+ architectures GGUF supportées immédiatement
- Metal MoE natif et optimisé
- Streaming token-by-token natif
- Binaire unique préservé (compilation statique)
- Communauté massive, support nouveaux modèles en jours
- Taille binaire réduite (~5-8 Mo vs ~15-25 Mo mistralrs+candle)
- CMake déjà dans la build chain (ADR-041 whisper.cpp)

**Compromis acceptés :**
- Dépendance C++ dans la build chain (cmake + compilateur C++)
- Boundary FFI (`llama-cpp-2` wrappe le `unsafe`, pas de `unsafe` direct dans notre code)
- Chat templates gérées par llama.cpp (couverture large mais pas identique à mistralrs)

## Conséquences

**Positives :**
- Déblocage immédiat de Qwen3.5, GLM-4.7, Llama 4 et tout futur modèle GGUF
- Metal MoE fonctionnel — les modèles MoE ne crashent plus sur Apple Silicon
- Vrai streaming token-by-token (amélioration UX significative dans le chat)
- Taille binaire réduite
- Alignement build chain avec `apollia-stt` (whisper.cpp utilise déjà cmake)

**Négatives / Compromis :**
- Build chain C++ obligatoire (déjà présente via ADR-041 whisper.cpp)
- Gestion de la tokenization explicite (vs abstraction `send_chat_request` de mistralrs)
- Pas de pure Rust — le backend embarqué dépend maintenant de code C/C++ compilé

**Neutres / À surveiller :**
- Conflit de symboles ggml entre whisper.cpp et llama.cpp : tester la co-compilation
  statique des deux dans le même binaire (risk identifié ADR-041, à valider)
- Montée de version llama.cpp : suivre les releases pour les nouvelles architectures
- Évaluer `ggml-sys` unifié si les deux projets convergent vers un ggml commun

## Principes architecturaux impactés

- **Principe #1 — Local-first** : renforcé — plus de modèles accessibles localement
- **Principe #2 — Zéro dépendance externe** : respecté — compilation statique, binaire unique
- **Principe #4 — Fail fast** : respecté — architecture GGUF non supportée détectée au load
- **Principe #5 — Un acteur, une responsabilité** : respecté — changement contenu dans `apollia-llm`

## Liens

- ADR précédent sur le même sujet : ADR-020 (LLM embarqué initial avec mistralrs)
- ADR connexe : ADR-041 (STT whisper.cpp — même pattern build chain C++)
- Crate bindings : [llama-cpp-2](https://crates.io/crates/llama-cpp-2)
- Issue upstream mistralrs : [#1939 — Qwen3.5 GGUF support](https://github.com/EricLBuehler/mistral.rs/issues/1939)
