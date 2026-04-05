# apollia-llm — Moteur LLM Embarqué et Clients Cloud

> *Inférence locale in-process ou cloud via HTTP — même interface, même traçabilité, zéro dépendance externe obligatoire.*

---

## 1. Rôle et architecture

`apollia-llm` est le crate d'inférence d'Apollia OS. Il expose un trait unifié `CompletionModel` implémenté par trois types de backends :

| Backend | Feature flag | Protocole | Souveraineté |
|---|---|---|---|
| `EmbeddedBackend` | `local` | In-process (llama.cpp via whisper-rs) | ✅ 100% local |
| `OpenAICompatibleClient` | `cloud` | HTTP REST (async-openai) | ❌ cloud |
| `AnthropicClient` | `cloud` | HTTP REST (reqwest) | ❌ cloud |

Depuis le Sprint 28 (ADR-047), la configuration des backends est **persistée dans SQLite** (`~/.apollia/system.db`) via `LlmBackendRepository` dans `apollia-core`. Le `LlmRouter` charge les backends au démarrage depuis ce registre. Chaque agent peut déclarer le backend qu'il souhaite utiliser via le champ `llm_backend` de son manifest.

**Principe fondamental :** le modèle `.gguf` n'est jamais compilé dans le binaire — c'est un fichier de données dans `~/.apollia/models/`. Le moteur d'inférence est compilé via `[feature = "local"]`. Les clients cloud sont compilés via `[feature = "cloud"]` (activé par défaut).

**Principe fondamental :** le modèle `.gguf` n'est jamais compilé dans le binaire — c'est un fichier de données dans `~/.apollia/models/`. Le moteur d'inférence est compilé via `[feature = "local"]`. Les clients cloud sont compilés via `[feature = "cloud"]` (activé par défaut).

---

## 2. Trait `CompletionModel`

Toute la crate repose sur ce trait. Implémenter ce trait suffit pour créer un backend custom ou un mock de test.

```rust
#[async_trait]
pub trait CompletionModel: Send + Sync {
    /// Envoie une requête d'inférence et retourne la réponse complète.
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError>;

    /// Retourne un stream de tokens et d'appels d'outils.
    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>;

    /// Indique si le backend est prêt à accepter des requêtes.
    fn is_available(&self) -> bool;

    /// Nom logique du backend tel que configuré dans le registre SQLite.
    fn backend_name(&self) -> &str;

    /// Identifiant du modèle chargé (ex. `llama3.2-3b-q4`, `claude-haiku-4-5-20251001`).
    fn model_id(&self) -> &str;
}
```

---

## 3. Types fondamentaux

```rust
pub struct CompletionRequest {
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolSpec>,          // outils disponibles pour le modèle (vide = pas de tool calling)
    pub model: Option<String>,         // override ponctuel du modèle
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

pub struct CompletionResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub finish_reason: FinishReason,
    pub latency_ms: u64,
}

pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub cost_usd: Option<f64>,         // None pour les backends locaux
}

pub struct BackendInfo {
    pub name: String,
    pub model_id: String,
    pub available: bool,
}

pub enum StreamChunk {
    /// Token textuel incrémental.
    Text(String),
    /// Appel d'outil demandé par le LLM.
    ToolCall(ToolCall),
}

pub struct ChatMessage {
    pub role: Role,
    pub content: MessageContent,
}

pub enum Role { System, User, Assistant, Tool }

pub enum MessageContent {
    Text(String),
    ToolResult { tool_call_id: String, content: String },
    WithToolCalls { text: String, tool_calls: Vec<ToolCall> },
}

pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,  // JSON Schema
}

pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

pub enum FinishReason { Stop, ToolCalls, Length, Error }

pub enum LlmError {
    BackendUnavailable { backend: String, reason: String },
    ModelNotFound { path: PathBuf },
    InferenceError(String),
    HttpError { status: u16, body: String },
    ApiKeyMissing { var: String },
    BudgetExceeded,
    MaxIterationsReached { iterations: u32 },
    MaxTokensReached,
    ParseError(String),
    UnsupportedModel { architecture: String },
    DeviceNotAvailable { device: String, hint: String },
}
```

---

## 4. LlmBackendRepository — Registre SQLite *(Sprint 28, ADR-047)*

La configuration des backends LLM est désormais persistée dans `~/.apollia/system.db` (table `llm_backends`). `LlmBackendRepository` est défini dans `apollia-core` et suit le même pattern que `TriggerDefinitionRepository`.

```rust
// crates/apollia-core/src/llm_backend.rs

/// Configuration d'un backend LLM enregistré dans system.db.
pub struct LlmBackendConfig {
    pub name: String,               // identifiant unique, ex. "local-code", "mistral-small"
    pub provider: LlmProvider,
    pub model: String,              // nom du modèle ou chemin GGUF absolu
    pub config_json: serde_json::Value,  // paramètres provider-spécifiques (peut contenir "${VAR}")
    pub enabled: bool,              // false = non chargé au démarrage
    pub is_default: bool,           // un seul défaut à la fois (unicité enforced par le repo)
}

pub enum LlmProvider {
    LlamaCpp,   // backend llama.cpp embarqué (GGUF local)
    OpenAi,     // API OpenAI ou compatible (LM Studio, vLLM)
    Mistral,    // API Mistral AI
    Anthropic,  // API Anthropic
    Ollama,     // Ollama local
}

pub struct LlmBackendRepository { /* conn: RefCell<Connection> */ }

impl LlmBackendRepository {
    /// Ouvre system.db et applique la migration idempotente.
    pub fn open(path: &Path) -> Result<Self, LlmBackendError>;

    /// Crée ou met à jour un backend. Si is_default=true, démarcate l'ancien défaut.
    pub fn save(&self, config: &LlmBackendConfig) -> Result<(), LlmBackendError>;

    /// Retourne tous les backends triés par nom.
    pub fn list(&self) -> Result<Vec<LlmBackendConfig>, LlmBackendError>;

    /// Trouve un backend par nom exact.
    pub fn find_by_name(&self, name: &str) -> Result<Option<LlmBackendConfig>, LlmBackendError>;

    /// Retourne le backend marqué is_default=true, ou None si aucun.
    pub fn find_default(&self) -> Result<Option<LlmBackendConfig>, LlmBackendError>;

    /// Marque name comme défaut (démarcate l'ancien atomiquement).
    pub fn set_default(&self, name: &str) -> Result<(), LlmBackendError>;

    /// Supprime un backend (interdit sur le backend par défaut).
    pub fn delete(&self, name: &str) -> Result<(), LlmBackendError>;
}
```

**Schéma SQLite :**
```sql
CREATE TABLE IF NOT EXISTS llm_backends (
    name         TEXT PRIMARY KEY,   -- [a-z0-9_-]+
    provider     TEXT NOT NULL,      -- llama-cpp | openai | mistral | anthropic | ollama
    model        TEXT NOT NULL,
    config_json  TEXT NOT NULL DEFAULT '{}',
    enabled      INTEGER NOT NULL DEFAULT 1,
    is_default   INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (provider IN ('llama-cpp', 'openai', 'mistral', 'anthropic', 'ollama'))
);
```

`config_json` contient les paramètres provider-spécifiques :
- `llama-cpp` : `{ "model_path": "/path/model.gguf", "n_gpu_layers": 35 }`
- `openai` : `{ "base_url": "https://api.openai.com/v1", "api_key": "${OPENAI_API_KEY}" }`
- `anthropic` : `{ "api_key": "${ANTHROPIC_API_KEY}" }`
- `ollama` : `{ "base_url": "http://localhost:11434" }`

Les secrets `${VAR}` sont résolus au démarrage depuis les variables d'environnement (jamais stockés en clair).

---

## 5. LlmRouter — Multi-backend

Le `LlmRouter` est le point d'entrée unique pour les requêtes LLM. Il charge les backends depuis `LlmBackendRepository` au démarrage.

```rust
pub struct LlmRouter { /* HashMap<String, Arc<dyn CompletionModel>> + default: String */ }

impl LlmRouter {
    /// Résout le backend par nom (None = backend par défaut du registre SQLite).
    pub fn get(&self, name: Option<&str>) -> Option<Arc<dyn CompletionModel>>;

    /// Liste tous les backends instanciés.
    pub fn list(&self) -> Vec<BackendInfo>;

    /// Appel avec observabilité intégrée (log tokens, latence, coût, EventBus).
    pub async fn complete_with_observability(
        &self,
        name: Option<&str>,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, LlmError>;
}
```

**Routing par agent :** quand un agent déclare `"llm_backend": "local-code"` dans son manifest, le runtime appelle `router.get(Some("local-code"))`. Si le backend est introuvable, un warning est émis et le runtime utilise le défaut (jamais d'erreur fatale pour l'agent).

---

## 6. EmbeddedBackend — Feature `local`

Inférence in-process via `llama.cpp` (ADR-042). Le modèle `.gguf` est chargé depuis `~/.apollia/models/`.

Depuis Sprint 28, la configuration du backend est dans `system.db` (voir section 4). Exemple de `config_json` pour un backend llama-cpp :

```json
{
  "model_path": "~/.apollia/models/llama3.2-3B-q4_K_M.gguf",
  "n_gpu_layers": 0
}
```

```rust
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

## 7. OpenAICompatibleClient — Feature `cloud`

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

## 8. AnthropicClient — Feature `cloud`

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

## 9. Observabilité — EventBus

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

## 10. Persistance des appels LLM *(Sprint 13)*

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

## 11. Tarification des backends cloud

> **Note :** Tarifs indicatifs en USD par million de tokens (MTok) au moment de la rédaction (avril 2026). Vérifier les sources officielles avant de budgéter.

### Backends cloud facturés

| Backend | Modèle | Input (USD/MTok) | Output (USD/MTok) | Remarques |
|---|---|---|---|---|
| `anthropic` | claude-haiku-4-5 | $0.80 | $4.00 | Modèle rapide, idéal pour tâches légères |
| `anthropic` | claude-sonnet-4-6 | $3.00 | $15.00 | Équilibre performance/coût |
| `anthropic` | claude-opus-4-6 | $15.00 | $75.00 | Modèle le plus capable |
| `openai` | gpt-4o-mini | $0.15 | $0.60 | Alternative économique |
| `openai` | gpt-4o | $2.50 | $10.00 | Polyvalent OpenAI |
| `openai` | o1-mini | $1.10 | $4.40 | Raisonnement avancé, latence élevée |

### Backends locaux (gratuits)

| Backend | Modèle | Coût | Remarques |
|---|---|---|---|
| `local` (llama.cpp) | Llama 3, Mistral, Qwen, etc. | **Gratuit** | CPU/GPU local, latence dépend du hardware |
| `ollama` | Tout modèle Ollama | **Gratuit** | Requiert Ollama installé localement |

### Table de pricing compilée *(Sprint 34 — STORY-436)*

Le calcul de `cost_usd` utilise une table lookup robuste dans `crates/apollia-llm/src/pricing.rs` (`default_pricing()`) avec correspondance exacte ou par préfixe de modèle :

```rust
// apollia-llm/src/pricing.rs

pub struct PricingTier {
    pub input_per_mtok: f64,   // USD par million de tokens en entrée
    pub output_per_mtok: f64,  // USD par million de tokens en sortie
}

/// Cherche le pricing par correspondance exacte, puis par préfixe.
/// "claude-sonnet-4-5-20261015" → pricing de "claude-sonnet-4-5".
pub fn lookup_pricing<'a>(
    model_id: &str,
    table: &'a HashMap<&str, PricingTier>,
    overrides: &'a HashMap<String, PricingTier>,
) -> Option<&'a PricingTier>;
```

Les surcharges opérateur sont configurables dans `apollia.toml` sous `[llm.pricing_overrides]` pour les modèles non reconnus ou les tarifs négociés.

### Suivi des coûts

Le champ `cost_usd` dans `RuntimeEvent::LlmCallCompleted` est calculé à partir de la table de
lookup compilée. Si le modèle n'est pas dans la table et qu'aucun override n'existe,
`cost_usd = None`. La CLI `apollia-os llm costs` agrège les coûts depuis `~/.apollia/llm_calls.db`.

Sources officielles :
- Anthropic : https://www.anthropic.com/pricing
- OpenAI : https://openai.com/api/pricing

---

## 12. ToolCallHelper — Boucle ReAct automatique

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

## 12. Intégration dans le Supervisor

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

## 13. Exemple de mock pour les tests

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

## 14. Prompt Caching *(Sprint 35, ADR-057)*

`AnthropicClient` envoie systématiquement l'en-tête `anthropic-beta: prompt-caching-2024-07-31` et pose trois breakpoints `cache_control: { type: "ephemeral" }` dans chaque requête :

1. **System prompt** — stable pour toute la session
2. **Liste des outils (`tools`)** — change rarement
3. **3ème message depuis la fin** — breakpoint glissant, maximise le hit-rate sur l'historique stable

**Champs ajoutés dans `TokenUsage` :**

```rust
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub cache_read_input_tokens: u32,    // Tokens lus depuis le cache Anthropic
    pub cache_write_input_tokens: u32,   // Tokens écrits dans le cache lors de cet appel
    pub cost_usd: Option<f64>,
}
```

Les backends `OpenAICompatibleClient` et `OllamaClient` ne supportent pas le prompt caching : les champs `cache_*` restent à 0, sans régression.

**Impact estimé :** −80% de coût sur les tokens en entrée pour les sessions répétant le même contexte (système + outils stables).

**Monitoring :** `~/.apollia/session_costs.jsonl` trace `cache_read_input_tokens` et `cache_write_input_tokens` par session pour vérifier le hit-rate.

> **Référence technique :** [ADR-057](../adr/ADR-057-prompt-caching-strategy.md)

---

## 15. Retry Policy *(Sprint 35, ADR-057)*

Tous les backends cloud implémentent une politique de retry exponentiel avec jitter. La `RetryPolicy` est définie dans `crates/apollia-llm/src/retry.rs` :

```rust
pub struct RetryPolicy {
    pub max_attempts: u32,    // Défaut : 3
    pub base_delay_ms: u64,   // Défaut : 500ms
    pub max_delay_ms: u64,    // Défaut : 30 000ms
    pub jitter: bool,         // Défaut : true — évite les retry storms
}
```

**Trait `IsRetryable` :**

```rust
pub trait IsRetryable {
    /// Retourne true si l'erreur est transitoire et peut être retentée.
    fn is_retryable(&self) -> bool;
}

impl IsRetryable for LlmError {
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            LlmError::HttpError { status, .. } if matches!(status, 429 | 500 | 502 | 503 | 504)
        )
    }
}
```

**Codes HTTP retryables :** 429 (rate limit), 500/502/503/504 (erreurs serveur transitoires).

**`CancellationToken` pendant le délai :** la boucle de retry utilise `tokio::select!` pour interrompre immédiatement le délai d'attente si le token d'annulation est déclenché :

```rust
tokio::select! {
    _ = tokio::time::sleep(delay) => { /* retry */ }
    _ = cancellation_token.cancelled() => { return Err(LlmError::Cancelled); }
}
```

**Backends couverts :** `AnthropicClient`, `OpenAICompatibleClient`, `OllamaClient`.

> **Référence technique :** `crates/apollia-llm/src/retry.rs`

---

## 16. TokenBudget *(Sprint 35)*

`TokenBudget` agrège les tokens consommés sur toute la durée d'une session ou d'une tâche. Il est accumulé par `LlmRouter` et affiché en fin de tâche par la CLI.

```rust
pub struct TokenBudget {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub total_cost_usd: f64,
    pub ttft_ms: Option<u64>,    // Time To First Token — uniquement en mode streaming
    pub wall_ms: u64,            // Durée totale de la tâche
}

impl TokenBudget {
    /// Résumé formaté pour la CLI.
    /// Ex: "Tokens: 1 234 input / 456 output / 789 cache-read — $0.0023 USD (TTFT: 312ms, wall: 4.2s)"
    pub fn format_summary(&self) -> String;
}
```

**Affichage CLI en fin de tâche :**
```
✓ Tâche terminée en 4.2s
  Tokens: 1 234 input / 456 output / 789 cache-read — $0.0023 USD (TTFT: 312ms, wall: 4.2s)
```

**Persistance :** chaque tâche terminée appende une ligne dans `~/.apollia/session_costs.jsonl` :
```json
{"session_id":"s-001","task_id":"t-042","input":1234,"output":456,"cache_read":789,"cost_usd":0.0023,"ttft_ms":312,"wall_ms":4200,"ts":"2026-04-04T12:00:00Z"}
```

---

## Routing LLM par niveau de précision — Sprint 36

Depuis le Sprint 36 (STORY-469), `LlmRouter` expose deux méthodes de routing basées sur le tradeoff coût/latence/qualité documenté dans les scaling laws (Kaplan et al., 2020).

### `LlmRoutingLevel`

```rust
/// Deux niveaux de routing — déductibles du tradeoff coût/latence/qualité.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmRoutingLevel {
    /// Tâches de raisonnement : planification, analyse complexe, jugement.
    Precise,
    /// Tâches d'extraction : métadonnées, résumés, classification, parsing.
    Fast,
}
```

### Méthodes ajoutées à `LlmRouter`

```rust
impl LlmRouter {
    /// Route vers le backend pour les tâches de raisonnement profond.
    pub fn route_precise(&self) -> &dyn LlmBackend { ... }

    /// Route vers le backend pour les tâches d'extraction légère.
    pub fn route_fast(&self) -> &dyn LlmBackend { ... }
}
```

**Fail-fast :** si `[llm.routing]` est absent de `apollia.toml`, `LlmRouter::new()` retourne `Err(RoutingConfigMissing)` au démarrage.

### Configuration

```toml
[llm.routing]
# Modèle pour le raisonnement (ORIA planner, analyse).
precise = "claude-opus-4-6"

# Modèle pour l'extraction légère (file paths, résumés).
fast = "claude-haiku-4-5-20251001"
```

### Callsites dans le codebase

| Callsite | Niveau | Justification |
|---|---|---|
| `apollia-oria/src/reasoner.rs` | `route_precise()` | Planification ReAct — erreur à fort impact |
| `apollia-workspace/src/style_detector.rs` | `route_fast()` | Extraction conventions — déterministe |
| `apollia-tools/src/executors/bash_executor.rs` | `route_fast()` | Extraction file paths — résultat vérifiable |
| `apollia-memory/src/compactor.rs` | `route_fast()` | Résumé contexte — faible coût d'erreur |

### `TokenBudgetUpdated` — Event enrichi (STORY-473)

```rust
/// Émis après chaque appel LLM — alimente le widget coût desktop.
TokenBudgetUpdated {
    session_cost_usd: f64,
    total_input_tokens: u64,
    total_output_tokens: u64,
    total_cache_read_tokens: u64,
    threshold_usd: f64,
    /// true si session_cost_usd > threshold_usd
    threshold_exceeded: bool,
},
```

Configuration seuil d'alerte :

```toml
[llm]
cost_alert_threshold_usd = 0.50
```

---

## 9. Google Vertex AI — Sprint 37

Depuis le Sprint 37 (STORY-495, ADR-068), Apollia supporte Google Vertex AI comme backend LLM via `aiplatform.googleapis.com`. L'authentification utilise Application Default Credentials (ADC) avec cache mémoire et refresh automatique.

### `VertexConfig`

```rust
// crates/apollia-core/src/config.rs

pub struct VertexConfig {
    /// Activer ce backend (false par défaut).
    #[serde(default)]
    pub enabled: bool,
    /// ID du projet GCP (ex: "my-gcp-project").
    pub project_id: String,
    /// Région Vertex AI (ex: "us-east5", "europe-west1").
    pub location: String,
    /// ID du modèle Anthropic publié sur Vertex (ex: "claude-sonnet-4-6@20251001").
    pub model_id: String,
}
```

```toml
# apollia.toml
[llm.vertex]
enabled = true
project_id = "my-gcp-project"
location = "us-east5"
model_id = "claude-sonnet-4-6@20251001"
```

### `VertexClient`

```rust
// crates/apollia-llm/src/backends/vertex.rs

/// Client Vertex AI — implémente CompletionModel.
/// Auth via ADC (authorized_user credentials gcloud).
pub struct VertexClient {
    config: VertexConfig,
    http_client: reqwest::Client,
    token_cache: Arc<Mutex<Option<GoogleToken>>>,
    retry_policy: RetryPolicy,
    cancel: CancellationToken,
}

impl VertexClient {
    /// Construit le client et vérifie la présence du fichier ADC.
    /// Retourne LlmError::Unauthorized si le fichier ADC est absent.
    pub fn new(config: &VertexConfig, cancel: CancellationToken) -> Result<Self, LlmError>;
}
```

**Résolution du fichier ADC (ordre de priorité) :**
1. Variable d'environnement `GOOGLE_APPLICATION_CREDENTIALS`
2. `~/.config/gcloud/application_default_credentials.json`

Seul le type `authorized_user` (credentials `gcloud auth application-default login`) est supporté. Les clés de service JSON sont hors périmètre — voir [ADR-068](../adr/ADR-068-vertex-adc-vs-service-account.md).

**Refresh automatique :** le token ADC est rafraîchi via `https://oauth2.googleapis.com/token` 60 secondes avant expiration. Le cache est en mémoire (`Arc<Mutex<Option<GoogleToken>>>`).

**Comportement HTTP :**
- `401` → `LlmError::Unauthorized` (pas de retry)
- `429` → retry selon `RetryPolicy` existant
- Corps de requête : identique à l'API Anthropic Messages (`anthropic-version: vertex-2023-10-16`)

> **Voir aussi :** [ADR-068](../adr/ADR-068-vertex-adc-vs-service-account.md) — justification ADC vs clé de service

---

## 10. AWS Bedrock — Sprint 37

Depuis le Sprint 37 (STORY-494, ADR-067), Apollia supporte AWS Bedrock comme backend LLM via SigV4 natif (sans le SDK AWS complet).

### `BedrockConfig`

```rust
// crates/apollia-core/src/config.rs

pub struct BedrockConfig {
    /// Activer ce backend (false par défaut).
    #[serde(default)]
    pub enabled: bool,
    /// Région AWS (ex: "us-east-1", "eu-west-1").
    pub region: String,
    /// ARN ou ID du modèle Bedrock (ex: "anthropic.claude-sonnet-4-6-20251001-v1:0").
    pub model_id: String,
}
```

```toml
# apollia.toml
[llm.bedrock]
enabled = true
region = "us-east-1"
model_id = "anthropic.claude-sonnet-4-6-20251001-v1:0"
```

**Credentials AWS :** résolus via la chaîne standard AWS (`AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` / `AWS_SESSION_TOKEN` ou `~/.aws/credentials`). La signature SigV4 est calculée nativement sans dépendance au SDK AWS.

> **Voir aussi :** [ADR-067](../adr/ADR-067-bedrock-sigv4-vs-sdk.md) — justification aws-sigv4 natif vs SDK complet

---

## Voir aussi

- [Agents RuntimeContext Guide](./Agents-RuntimeContext-Guide) — `ctx.llm` depuis Python
- [Briques ORIA Engine](./Briques-ORIA-Engine) — Reasoner LLM en Mode Orchestré
- [Briques AIP Specification](./Briques-AIP-Specification) — champ `llm_backend` dans `AgentManifest`
- [API HTTP Reference](./API-HTTP-Reference) — endpoints CRUD `/api/v1/llm/backends`
- [Config apollia.toml](./Config-apollia-toml) — section `[llm.observability]`
- [Ops Exploitation et Debug](./Ops-Exploitation-et-Debug) — `apollia-os llm status/ping/chat`
- [ADR-068](../adr/ADR-068-vertex-adc-vs-service-account.md) — Google Vertex AI : ADC vs clé de service
- [ADR-067](../adr/ADR-067-bedrock-sigv4-vs-sdk.md) — AWS Bedrock : aws-sigv4 natif vs SDK complet
- [ADR-057](../adr/ADR-057-prompt-caching-strategy.md) — Prompt Caching Strategy (Sprint 35)
- [ADR-047](../adr/ADR-047-multi-llm-backend-registry.md) — Multi-LLM Backend Registry (SQLite, Sprint 28)
- [ADR-042](../adr/ADR-042-remplacement-mistralrs-par-llamacpp-statique.md) — remplacement mistral-rs par llama.cpp
- [ADR-020](../adr/ADR-020-apollia-llm-moteur-embarque-modeles-externes-feature-flags.md) — feature flags LLM
- [ADR-026](../adr/ADR-026-observabilite-complete-persistance-timeline-troncature.md) — observabilité complète
