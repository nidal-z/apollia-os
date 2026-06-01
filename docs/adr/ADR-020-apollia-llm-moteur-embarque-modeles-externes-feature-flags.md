# ADR-020 - apollia-llm : moteur d'inférence embarqué (llama.cpp), modèles fichiers externes, feature flags

**Date :** 2026-03-08 (architecture initiale) / 2026-03-26 (migration llama.cpp)
**Statut :** Accepté
**Décideur :** Nidal (solo)
**Sprint :** 8 (architecture) → 25 (llama.cpp)

---

## Contexte

Sprint 8 introduit la capacité LLM native dans Apollia OS : un agent Python doit pouvoir appeler `ctx.llm.chat()` sans aucun service externe obligatoire.

Trois contraintes non-négociables :

1. **Principe #1 - Local-first** : l'inférence doit fonctionner offline, sans API key, sans cloud.
2. **Principe #2 - Zéro dépendance opérationnelle** : `apollia-os start` ne peut pas supposer qu'un daemon tiers (ollama, llama.cpp-server) est déjà lancé.
3. **Principe #4 - Fail fast** : si le modèle est absent ou corrompu, l'erreur est signalée au démarrage.

En parallèle, certains utilisateurs préfèrent déléguer l'inférence à un backend cloud (Anthropic, OpenAI). Les deux cas doivent être couverts sans imposer la compilation du moteur d'inférence à ceux qui n'en ont pas besoin.

**Sprint 25 (remplacement mistral.rs → llama.cpp) :** Le moteur `mistralrs` v0.7 présentait trois problèmes bloquants en QA : seulement 16 architectures GGUF supportées (Qwen3.5, GLM-4.7, Llama 4 Scout non reconnus) ; crash Metal sur les modèles MoE (`indexed_moe_forward` absent dans candle-metal) ; streaming limité par un lifetime non-`'static` impossible à passer dans un `tokio::spawn`. `llama.cpp` couvre 30+ architectures, a des kernels Metal natifs pour MoE, et offre un streaming token-by-token natif.

---

## Décision

La crate `apollia-llm` utilise **deux feature flags Cargo exclusifs** :

- `cloud` (défaut) - compile les clients HTTP `AnthropicClient`, `OpenAICompatibleClient`, `BedrockClient`, `VertexClient` via `reqwest`. Binaire léger, aucun moteur d'inférence.
- `local` - compile en plus `EmbeddedBackend` via `llama-cpp-2` (bindings safe Rust pour llama.cpp, lié statiquement).

Le modèle (`.gguf`) n'est **jamais embarqué dans le binaire**. Il réside dans `~/.apollia/models/` comme un fichier de données externe - exactement comme une base SQLite (ADR-002). Le moteur d'inférence est compilé statiquement dans le binaire quand `feature = "local"`.

`LlmRouter` dispatche vers le bon backend au runtime selon la config. Si une clé API cloud est absente, le backend est ignoré avec un `tracing::warn!`. Si aucun backend n'est disponible, `ctx.llm` est `None` et l'agent passe en `DEGRADED`.

### Feature flags

```toml
# Cargo.toml features (apollia-llm)
cloud        = ["reqwest", "async-openai"]
local        = ["cloud", "llama-cpp-2"]
local-metal  = ["local", "llama-cpp-2/metal"]
local-cuda   = ["local", "llama-cpp-2/cuda"]
```

### Moteur embarqué (feature `local`)

- **Chargement :** `llama_cpp_2::LlamaModel::load_from_file()`. Configuration GPU Metal via `LlamaModelParams`.
- **Inférence :** `LlamaContext` + tokenization via `model.str_to_token()` + `llama_decode()`.
- **Chat templates :** `llama_chat_apply_template()` (support natif Jinja, large couverture).
- **Streaming :** token-by-token natif via la boucle de décodage llama.cpp, émis comme `StreamChunk::Text`.

### Backends cloud (feature `cloud`)

- **Anthropic / OpenAI-compatible :** via `async-openai` + `reqwest`
- **AWS Bedrock :** signature SigV4 native via `aws-sigv4` + `reqwest` (l'aws-sdk-rust complet ajouterait ~50 crates et +8 MB pour 2% des fonctionnalités utilisées)
- **Google Vertex AI :** Application Default Credentials via `gcp-auth` (chaîne ADC standard - GOOGLE_APPLICATION_CREDENTIALS → gcloud credentials → service account métadonnées d'instance)

**Configuration Bedrock :**
```toml
[[llm.backends]]
type = "api"
provider = "bedrock"
model = "anthropic.claude-3-sonnet-20241022-v2:0"
region = "us-east-1"
# Credentials via AWS_ACCESS_KEY_ID + AWS_SECRET_ACCESS_KEY ou ~/.aws/credentials
```

**Configuration Vertex :**
```toml
[[llm.backends]]
type = "api"
provider = "vertex"
model = "gemini-2.0-flash-001"
project_id = "my-gcp-project"
region = "us-central1"
# Credentials via ADC (gcloud auth application-default login ou GOOGLE_APPLICATION_CREDENTIALS)
```

---

## Alternatives considérées

### Daemon externe géré par le Supervisor (rejetée)

Lancer un processus `llama-server` ou `ollama serve` comme enfant du Supervisor. **Contre :** viole le Principe #2 (suppose llama.cpp ou ollama installé sur la machine). Gestion PID complexe (race conditions, zombie processes, port conflicts). Pas de déploiement "single binary" réel.

### Modèle GGUF embarqué dans le binaire (rejetée)

Un modèle quantifié 4-bit minimal (Llama 3.2 3B) pèse ~2 Go - binaire inutilisable. Impossible de changer de modèle sans recompiler.

### Attendre mistral.rs 0.8+ (rejetée, Sprint 25)

Délai inconnu, pas de garantie sur le support Metal MoE (dépend de candle upstream). `mistralrs-core` v0.7.0 publiée le 28/01/2026, aucune version 0.7.1 depuis 2 mois. Le support GGUF `qwen35` absent de tous les commits.

### llama-server comme processus externe (rejetée, Sprint 25)

Zéro modification du crate `apollia-llm`, mais viole le Principe #2. Nécessite de distribuer un binaire séparé, gérer son lifecycle (spawn, health check, kill). Incompatible avec le modèle binaire unique.

### aws-sdk-rust complet pour Bedrock (rejetée)

~50 crates supplémentaires, +45s de compilation, +8 MB binaire pour utiliser ~2% du SDK. `aws-sigv4` seul couvre 100% du besoin (SigV4 est stable depuis 2012).

### Clé de service JSON comme mécanisme primaire pour Vertex (rejetée)

Fichier de clé statique exfiltrable avec la config. Pas d'expiration automatique. Les meilleures pratiques Google Cloud recommandent ADC pour les applications locales. La clé de service JSON reste supportée via `GOOGLE_APPLICATION_CREDENTIALS` (premier élément de la chaîne ADC).

---

## Conséquences

**Positives :**
- `apollia-os start` avec un `.gguf` valide → inférence locale zéro-latence réseau
- 30+ architectures GGUF supportées (Qwen3.5, GLM-4.7, Llama 4, etc.)
- Metal MoE fonctionnel sur Apple Silicon
- Vrai streaming token-by-token
- Taille binaire réduite vs mistralrs+candle (~5-8 Mo vs ~15-25 Mo)
- Bedrock + Vertex intégrés sans dépendances SDK lourdes
- Observabilité native : `LlmCallCompleted` sur EventBus après chaque appel (tokens, latence, coût)
- La boucle ReAct intègre `StepBudget` - garde-fou Principe #7 respecté

**Négatives / Compromis :**
- Build chain C++ obligatoire pour `feature = "local"` (cmake + compilateur C++ - déjà présente via apollia-stt/whisper.cpp)
- Deux binaires de distribution à tester en CI (`cloud` + `local`)
- Gestion de la tokenization explicite (vs abstraction de mistralrs)
- ADC Vertex requiert `gcloud` installé ou `GOOGLE_APPLICATION_CREDENTIALS` configuré

**Neutres / À surveiller :**
- Conflit de symboles ggml entre whisper.cpp et llama.cpp : validé - les deux se compilent statiquement dans le même binaire sans conflit
- Montée de version llama.cpp : suivre les releases pour les nouvelles architectures

---

## Principes architecturaux impactés

- **Principe #1 - Local-first** : `feature = "local"` offre une inférence 100% offline
- **Principe #2 - Zéro dépendance externe** : compilation statique, binaire unique ; aws-sigv4 minimal, gcp-auth pour ADC
- **Principe #4 - Fail fast** : modèle absent → `LlmError::ModelNotFound` au démarrage ; credentials manquants → `LlmError::ApiKeyMissing`
- **Principe #7 - Garde-fous non-négociables** : `run_tools()` consulte `StepBudget.is_exhausted()` à chaque itération ReAct

---

## Liens

- Stories Sprint 8 : STORY-051 → STORY-064
- Stories Sprint 25 : remplacement mistralrs → llama-cpp-2
- Stories Sprint 37 : STORY-494 (Bedrock), STORY-495 (Vertex)
- Crate : [llama-cpp-2](https://crates.io/crates/llama-cpp-2)
- ADR-041 - STT whisper.cpp (même pattern build chain C++)
- ADR-047 - Multi-LLM backend registry
- ADR-079 - LLM DB-first config (LLM config migrée de apollia.toml vers system.db)
