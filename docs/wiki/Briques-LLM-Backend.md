# apollia-llm — Moteur LLM Embarqué et Clients Cloud

> *Inférence locale in-process ou cloud via HTTP — même interface, même traçabilité, zéro dépendance externe obligatoire.*

---

## 1. Rôle et architecture

`apollia-llm` est le crate d'inférence d'Apollia OS. Il expose un trait unifié `CompletionModel` implémenté par trois types de backends :

| Backend | Feature flag | Protocole | Souveraineté |
|---|---|---|---|
| `EmbeddedBackend` | `local` | In-process (mistral-rs-core) | ✅ 100% local |
| `OpenAICompatibleClient` | `cloud` | HTTP REST (async-openai) | ❌ cloud |
| `AnthropicClient` | `cloud` | HTTP REST (reqwest) | ❌ cloud |

Le `LlmRouter` instancie les backends au démarrage du Supervisor (position 5, avant `TaskRouter`) et dispatche les requêtes par nom. Il est partageable via `Arc<LlmRouter>`.

**Principe fondamental :** le modèle `.gguf` n'est jamais compilé dans le binaire — c'est un fichier de données dans `~/.apollia/models/`. Le moteur d'inférence est compilé via `[feature = "local"]`. Les clients cloud sont compilés via `[feature = "cloud"]` (activé par défaut).

---

## 2. Trait `CompletionModel`

Toute la crate repose sur ce trait. Implémenter ce trait suffit pour créer un backend custom ou un mock de test.

```rust
#[async_trait]
pub trait CompletionModel: Send + Sync {
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, LlmError>;

    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<impl Stream<Item = Result<String, LlmError>>, LlmError>;

    fn info(&self) -> BackendInfo;
}
```

---

## 3. Types fondamentaux

```rust
pub struct CompletionRequest {
    pub messages: Vec<ChatMessage>,
    pub tools: Option<Vec<ToolSpec>>,     // outils disponibles pour le modèle
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: bool,
}

pub struct CompletionResponse {
    pub content: String,
    pub finish_reason: FinishReason,
    pub usage: TokenUsage,
    pub tool_calls: Vec<ToolCall>,
    pub latency_ms: u64,
    pub cost_usd: Option<f64>,           // Some() uniquement pour les backends cloud
}

pub struct ChatMessage {
    pub role: Role,
    pub content: MessageContent,
}

pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

pub struct BackendInfo {
    pub name: String,
    pub model_id: String,
    pub backend_type: String,    // "embedded" | "openai-compatible" | "anthropic"
    pub is_local: bool,
}

pub enum LlmError {
    /// Backend demandé introuvable dans le LlmRouter.
    BackendUnavailable { backend: String, reason: String },
    /// Fichier .gguf absent au chemin configuré.
    ModelNotFound { path: PathBuf },
    /// Erreur interne du moteur d'inférence (mistral-rs-core).
    InferenceError(String),
    /// Erreur HTTP d'un backend cloud (status, body).
    HttpError { status: u16, body: String },
    /// Variable d'environnement de clé API absente (ex: ANTHROPIC_API_KEY).
    ApiKeyMissing { var: String },
    /// StepBudget épuisé pendant la boucle ReAct (ToolCallHelper).
    BudgetExceeded,
    /// Nombre max d'itérations ToolCallHelper atteint.
    MaxIterationsReached { iterations: u32 },
    /// Limite de tokens de génération atteinte.
    MaxTokensReached,
    /// Impossible de parser la réponse JSON du backend.
    ParseError(String),
    /// Device (CUDA/Metal) non compilé dans ce binaire.
    DeviceNotAvailable { device: String, hint: String },
}
```

---

## 4. LlmRouter

Le `LlmRouter` est le point d'entrée unique pour les requêtes LLM. Il est construit au démarrage depuis la configuration TOML.

```rust
pub struct LlmRouter { /* ... */ }

impl LlmRouter {
    /// Construit le router depuis la config TOML et émet les événements Supervisor.
    pub async fn from_config(config: &LlmConfig) -> Result<Self, LlmError>;

    /// Version observée — émet `LlmModelLoading/Ready/Failed` sur l'EventBus.
    pub async fn from_config_with_bus(
        config: &LlmConfig,
        event_bus: EventBusSender,
    ) -> Result<Self, LlmError>;

    /// Résout le backend par nom (None = backend par défaut).
    pub fn get(&self, name: Option<&str>) -> Option<Arc<dyn CompletionModel>>;

    /// Liste tous les backends instanciés.
    pub fn list(&self) -> Vec<BackendInfo>;

    /// Nom du backend par défaut.
    pub fn default_name(&self) -> &str;

    /// Appel avec observabilité intégrée (log tokens, latence, coût, EventBus).
    pub async fn complete_with_observability(
        &self,
        name: Option<&str>,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, LlmError>;
}
```

---

## 5. EmbeddedBackend — Feature `local`

Inférence in-process via `mistral-rs-core`. Le modèle `.gguf` est chargé depuis `~/.apollia/models/`.

```toml
[[llm.backends]]
type = "embedded"
name = "local"
model_path = "~/.apollia/models/llama3.2-3B-q4_K_M.gguf"
device = "cpu"        # "cpu" | "cuda" | "metal"
```

```rust
pub struct EmbeddedBackendConfig {
    pub name: String,
    pub model_path: PathBuf,   // chemin absolu après tilde-expansion
    pub device: AcceleratorDevice,
}

pub enum AcceleratorDevice {
    Cpu,
    Cuda,   // [feature = "local-cuda"]  — nécessite GPU NVIDIA
    Metal,  // [feature = "local-metal"] — nécessite macOS Apple Silicon
}
```

**Feature flags locaux :**

| Feature | Activation | Prérequis | État |
|---|---|---|---|
| `local-cpu` | Inclus dans `local` | Aucun | ✅ Disponible |
| `local-metal` | `--features local-metal` | macOS Apple Silicon (M1+) | ✅ Disponible — `objc2-metal 0.3.2` sur crates.io |
| `local-accelerate` | `--features local-accelerate` | macOS (CPU BLAS vectorisé) | ✅ Disponible — plus rapide que CPU pur sans GPU |
| `local-cuda` | `--features local-cuda` | GPU NVIDIA + CUDA toolkit | ⚠️ Déclaré — non testé (pas de GPU NVIDIA en CI) |

> **Fail-fast (Principe #4) :** Si `device = "cuda"` ou `device = "metal"` mais que la feature correspondante n'est pas compilée, `EmbeddedBackend::load()` retourne `LlmError::DeviceNotAvailable { device, hint }` au démarrage — jamais de panic silencieux.

**Compiler avec Metal (Apple Silicon) :**

```bash
# Build standard — fonctionne sans Xcode complet
# MISTRALRS_METAL_PRECOMPILE=0 est défini par défaut dans .cargo/config.toml
cargo build --release --features local-metal

# Combiner avec Accelerate (BLAS vectorisé Apple) — recommandé sur Apple Silicon
cargo build --release --features local-metal,local-accelerate
```

Le projet configure `MISTRALRS_METAL_PRECOMPILE=0` dans `.cargo/config.toml` : les shaders Metal sont compilés JIT au premier appel d'inférence plutôt que pendant le build (ce qui nécessiterait Xcode complet). Les performances GPU sont identiques après ce premier appel.

Pour la distribution (shaders baked dans le binaire, Xcode requis) :
```bash
MISTRALRS_METAL_PRECOMPILE=1 cargo build --release --features local-metal
```

---

## 6. OpenAICompatibleClient — Feature `cloud`

Client HTTP via `async-openai`. Compatible avec tout endpoint OpenAI-like (OpenAI, Azure OpenAI, Ollama avec API OpenAI, etc.).

```toml
[[llm.backends]]
type = "api"
name = "gpt-4o-mini"
api_url = "https://api.openai.com/v1"
model = "gpt-4o-mini"
api_key_env = "OPENAI_API_KEY"   # nom de la variable d'environnement
```

Heuristique de sélection : si `api_url` contient `anthropic.com`, `AnthropicClient` est instancié. Sinon, `OpenAICompatibleClient`.

---

## 7. AnthropicClient — Feature `cloud`

Client HTTP natif via `reqwest` (sans SDK Anthropic officiel — format natif Messages API).

```toml
[[llm.backends]]
type = "api"
name = "anthropic"
api_url = "https://api.anthropic.com/v1"
model = "claude-haiku-4-5-20251001"
api_key_env = "ANTHROPIC_API_KEY"
```

Calcul du coût estimé (`cost_usd`) disponible pour les modèles haiku/sonnet/opus selon la table de prix compilée.

---

## 8. Observabilité — EventBus

Après chaque appel `complete_with_observability()`, Apollia OS émet automatiquement sur l'EventBus :

```rust
RuntimeEvent::LlmCallCompleted {
    backend:           String,   // "local" | "anthropic" | "gpt-4o-mini"
    prompt_tokens:     u32,
    completion_tokens: u32,
    latency_ms:        u64,
    cost_usd:          Option<f64>,   // None pour les backends locaux
}
```

Événements Supervisor lors du démarrage :
```rust
RuntimeEvent::LlmModelLoading { backend: String, model_path: String }
RuntimeEvent::LlmModelReady   { backend: String, model_id: String }
RuntimeEvent::LlmModelFailed  { backend: String, reason: String }
```

Si `observability.debug_log_prompt = true` dans `apollia.toml`, le prompt complet est loggé au niveau `TRACE`. **Ne jamais activer en production.**

---

## 9. Persistance des appels LLM *(Sprint 13)*

Le `LlmCallRepository` persiste chaque appel LLM dans `~/.apollia/llm_calls.db` (SQLite) pour l'observabilité et le suivi des coûts.

```sql
CREATE TABLE llm_calls (
    id               TEXT PRIMARY KEY,
    task_id          TEXT,
    step_id          TEXT,
    backend          TEXT NOT NULL,
    model            TEXT NOT NULL,
    prompt_tokens    INTEGER,
    completion_tokens INTEGER,
    cost_usd         REAL,
    latency_ms       INTEGER,
    prompt_text      TEXT,          -- NULL si debug_log_prompt = false (défaut)
    completion_text  TEXT,
    created_at       TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))
);
```

```rust
pub struct LlmCallRepository { /* ... */ }

impl LlmCallRepository {
    pub fn open(path: &Path) -> Result<Self, LlmRepositoryError>;
    pub fn save(&self, record: &LlmCallRecord) -> Result<(), LlmRepositoryError>;
    pub fn query_by_task(&self, task_id: &str) -> Result<Vec<LlmCallRecord>, LlmRepositoryError>;
    pub fn costs_by_backend_model_since(&self, since: &str) -> Result<Vec<LlmCostSummary>, LlmRepositoryError>;
}
```

**Intégration EventBus :** `spawn_subscriber()` souscrit à `RuntimeEvent::LlmCallCompleted` et persiste via `spawn_blocking`. Si `ObservabilityConfig.debug_log_prompt` est `false` (défaut), le champ `prompt_text` est `NULL` — conforme RGPD.

**Agrégation des coûts :** `costs_by_backend_model_since()` retourne les coûts agrégés par backend et modèle depuis une date donnée, utilisé par la Timeline API et le dashboard.

---

## 10. ToolCallHelper — Boucle ReAct automatique

`ToolCallHelper` implémente la boucle ReAct complète pour les agents qui veulent déléguer le raisonnement outil-par-outil au LLM.

```rust
pub struct ToolCallHelper {
    pub fn new(
        model: Arc<dyn CompletionModel>,
        invoker: Arc<dyn ToolInvoker>,
    ) -> Self;

    pub async fn run_tools(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolSpec>,
        max_iterations: u32,
        budget_view: &StepBudgetView,
    ) -> Result<CompletionResponse, LlmError>;
}

pub trait ToolInvoker: Send + Sync {
    async fn invoke(&self, name: &str, args: serde_json::Value)
        -> Result<String, String>;
}
```

La boucle `run_tools()` :
1. Appelle `model.complete()` avec les outils disponibles
2. Si `finish_reason == ToolCalls` → exécute les outils via `invoker.invoke()`
3. **Erreurs d'outil absorbées** : `invoker.invoke()` retourne `Result<String, String>` — une erreur devient un message `Role::Tool` avec le texte d'erreur (jamais fatale pour la boucle)
4. Ajoute les résultats comme messages `Role::Tool`
5. Répète jusqu'à `finish_reason == Stop` ou `max_iterations` atteint → `LlmError::MaxIterationsReached`
6. Si `budget_view.is_exhausted()` → `LlmError::BudgetExceeded` immédiat
7. Si `finish_reason` est ni `Stop` ni `ToolCalls` → `LlmError::InferenceError`

---

## 11. Intégration dans le Supervisor

Le `LlmRouter` est démarré à la **position 5** dans le Supervisor (avant `TaskRouter`) :

```
1. EventBus
2. AgentRegistry
3. ToolRegistry
4. MemoryEngine
5. LlmRouter ← nouveau Sprint 8
6. TaskRouter
7. APIServer
```

Si tous les backends échouent à démarrer → warning + runtime continue sans LLM (`ctx.llm` sera `None` pour les agents).

---

## 12. Exemple de mock pour les tests

```rust
use std::sync::Arc;
use apollia_llm::{CompletionModel, CompletionRequest, CompletionResponse, LlmError, LlmRouter};

pub struct MockModel {
    pub response: String,
}

#[async_trait::async_trait]
impl CompletionModel for MockModel {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            content: self.response.clone(),
            ..Default::default()
        })
    }
    // ...
}

let router = LlmRouter::with_backends(
    vec![Arc::new(MockModel { response: "ok".into() })],
    "mock".into(),
);
```

---

## Voir aussi

- [Agents RuntimeContext Guide](./Agents-RuntimeContext-Guide) — `ctx.llm` depuis Python
- [Briques ORIA Engine](./Briques-ORIA-Engine) — Reasoner LLM en Mode Orchestré
- [Config apollia.toml](./Config-apollia-toml) — section `[[llm.backends]]`
- [Ops Exploitation et Debug](./Ops-Exploitation-et-Debug) — `apollia-os llm status/ping/chat`
- [ADR-020](../adr/ADR-020-apollia-llm-moteur-embarque-modeles-externes-feature-flags) — décision feature flags
- [ADR-026](../adr/ADR-026-observabilite-complete-persistance-timeline-troncature) — observabilité complète et troncature
