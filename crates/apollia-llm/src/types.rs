//! Types fondamentaux et trait `CompletionModel` pour `apollia-llm`.
//!
//! Ce module est la fondation du crate : tous les backends et le routeur
//! dépendent de ces types. Aucun backend n'est importé ici — les types
//! sont disponibles quelle que soit la feature activée.

use std::path::PathBuf;
use std::pin::Pin;

use futures::Stream;

// ─────────────────────────────────────────────
// Trait principal
// ─────────────────────────────────────────────

/// Trait unifié pour tout backend LLM — local embarqué ou cloud HTTP.
///
/// Implémenté par `EmbeddedBackend` (`feature = "local"`),
/// `OpenAICompatibleClient` et `AnthropicClient` (`feature = "cloud"`).
/// Stocké via `Arc<dyn CompletionModel>` dans le `LlmRouter`.
#[async_trait::async_trait]
pub trait CompletionModel: Send + Sync {
    /// Envoie une requête d'inférence et retourne la réponse complète.
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError>;

    /// Retourne un stream de [`StreamChunk`]s (tokens texte et/ou appels d'outils).
    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>;

    /// Indique si le backend est prêt à accepter des requêtes.
    fn is_available(&self) -> bool;

    /// Nom logique du backend tel que configuré dans `apollia.toml`.
    fn backend_name(&self) -> &str;

    /// Identifiant du modèle chargé (ex. `llama3.2-3b-q4`, `claude-haiku-4-5-20251001`).
    fn model_id(&self) -> &str;
}

// ─────────────────────────────────────────────
// Requête / Réponse
// ─────────────────────────────────────────────

/// Requête d'inférence unifiée pour tous les backends.
///
/// Dérive `Default` pour permettre la syntaxe `..Default::default()`
/// lors de la construction partielle.
#[derive(Debug, Clone, Default)]
pub struct CompletionRequest {
    /// Historique de la conversation à envoyer au modèle.
    pub messages: Vec<ChatMessage>,
    /// Outils (fonctions) exposés au LLM pour le tool calling.
    pub tools: Vec<ToolSpec>,
    /// Override ponctuel du modèle (sinon le backend utilise son défaut).
    pub model: Option<String>,
    /// Température de sampling (0.0 = déterministe, 1.0 = créatif).
    pub temperature: Option<f32>,
    /// Nombre maximum de tokens à générer.
    pub max_tokens: Option<u32>,
}

/// Réponse d'inférence unifiée retournée par tous les backends.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    /// Contenu textuel généré par le modèle.
    pub content: String,
    /// Appels d'outils demandés par le modèle (vide si `finish_reason != ToolCalls`).
    pub tool_calls: Vec<ToolCall>,
    /// Statistiques de consommation de tokens.
    pub usage: TokenUsage,
    /// Raison pour laquelle la génération s'est arrêtée.
    pub finish_reason: FinishReason,
    /// Latence totale de l'appel en millisecondes.
    pub latency_ms: u64,
}

// ─────────────────────────────────────────────
// Tokens & coût
// ─────────────────────────────────────────────

/// Statistiques de consommation de tokens et coût estimatif.
///
/// `cost_usd` est `None` pour les backends locaux (coût = 0).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenUsage {
    /// Nombre de tokens dans le prompt (entrée).
    pub prompt_tokens: u32,
    /// Nombre de tokens générés (sortie).
    pub completion_tokens: u32,
    /// Coût estimatif en USD — `None` pour `EmbeddedBackend`.
    pub cost_usd: Option<f64>,
}

// ─────────────────────────────────────────────
// Raison de fin
// ─────────────────────────────────────────────

/// Raison pour laquelle la génération du modèle s'est arrêtée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishReason {
    /// Génération terminée naturellement (token EOS atteint).
    Stop,
    /// Le modèle a demandé l'exécution d'outils.
    ToolCalls,
    /// Limite de tokens atteinte avant la fin naturelle.
    Length,
    /// Le backend a retourné une erreur.
    Error,
}

// ─────────────────────────────────────────────
// Messages de conversation
// ─────────────────────────────────────────────

/// Message dans un historique de conversation multi-tour.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Rôle de l'émetteur du message.
    pub role: Role,
    /// Contenu du message (texte, résultat d'outil, ou texte + appels d'outils).
    pub content: MessageContent,
}

impl ChatMessage {
    /// Construit un message système (instructions globales de comportement).
    pub fn system(text: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: MessageContent::Text(text.into()),
        }
    }

    /// Construit un message utilisateur.
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: MessageContent::Text(text.into()),
        }
    }

    /// Construit un message assistant sans appels d'outils.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: MessageContent::Text(text.into()),
        }
    }

    /// Construit un message assistant avec des appels d'outils inclus.
    pub fn assistant_with_calls(text: &str, calls: &[ToolCall]) -> Self {
        Self {
            role: Role::Assistant,
            content: MessageContent::WithToolCalls {
                text: text.to_owned(),
                tool_calls: calls.to_vec(),
            },
        }
    }

    /// Construit un message contenant le résultat d'un appel d'outil.
    pub fn tool_result(call_id: &str, content: &str) -> Self {
        Self {
            role: Role::Tool,
            content: MessageContent::ToolResult {
                tool_call_id: call_id.to_owned(),
                content: content.to_owned(),
            },
        }
    }
}

/// Rôle de l'émetteur d'un `ChatMessage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// Instructions système (comportement global de l'assistant).
    System,
    /// Message de l'utilisateur humain.
    User,
    /// Réponse de l'assistant LLM.
    Assistant,
    /// Résultat d'un appel d'outil retourné au modèle.
    Tool,
}

/// Contenu d'un `ChatMessage`.
#[derive(Debug, Clone)]
pub enum MessageContent {
    /// Contenu purement textuel.
    Text(String),
    /// Résultat d'un appel d'outil (rôle `Tool`).
    ToolResult {
        /// Identifiant de l'appel d'outil correspondant.
        tool_call_id: String,
        /// Contenu retourné par l'outil.
        content: String,
    },
    /// Réponse assistant combinant texte et appels d'outils.
    WithToolCalls {
        /// Texte accompagnant les appels d'outils (peut être vide).
        text: String,
        /// Liste des appels d'outils demandés par le modèle.
        tool_calls: Vec<ToolCall>,
    },
}

// ─────────────────────────────────────────────
// Tool calling
// ─────────────────────────────────────────────

/// Spécification d'un outil transmise au LLM au format JSON Schema.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolSpec {
    /// Nom de l'outil tel qu'il sera invoqué par le modèle.
    pub name: String,
    /// Description fonctionnelle de l'outil pour guider le modèle.
    pub description: String,
    /// Schéma JSON des paramètres acceptés par l'outil.
    pub parameters: serde_json::Value,
}

/// Appel d'outil demandé par le LLM dans une `CompletionResponse`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCall {
    /// Identifiant unique de cet appel (pour corréler avec le `ToolResult`).
    pub id: String,
    /// Nom de l'outil à invoquer.
    pub name: String,
    /// Arguments passés à l'outil (objet JSON).
    pub arguments: serde_json::Value,
}

// ─────────────────────────────────────────────
// Streaming
// ─────────────────────────────────────────────

/// A chunk emitted by a streaming LLM response.
///
/// The stream yields a sequence of `Text` chunks (tokens), optionally
/// followed by one or more `ToolCall` chunks if the model requests
/// tool invocations.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Incremental text token for progressive display.
    Text(String),
    /// Tool call requested by the LLM (emitted when tool calling is detected in the stream).
    ToolCall(ToolCall),
}

// ─────────────────────────────────────────────
// Info backend (pour LlmRouter::list())
// ─────────────────────────────────────────────

/// Informations synthétiques sur un backend — retournées par `LlmRouter::list()`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BackendInfo {
    /// Nom logique du backend (clé de configuration).
    pub name: String,
    /// Identifiant du modèle chargé.
    pub model_id: String,
    /// `true` si le backend est prêt à accepter des requêtes.
    pub available: bool,
}

// ─────────────────────────────────────────────
// Erreurs
// ─────────────────────────────────────────────

/// Erreurs unifiées du crate `apollia-llm`.
///
/// Chaque variant couvre un mode d'échec distinct pour permettre
/// un traitement différencié par l'appelant (retry, degraded, abort...).
#[derive(thiserror::Error, Debug)]
pub enum LlmError {
    /// Le backend demandé n'est pas disponible.
    #[error("backend '{backend}' unavailable: {reason}")]
    BackendUnavailable {
        /// Nom du backend concerné.
        backend: String,
        /// Raison de l'indisponibilité.
        reason: String,
    },

    /// Le fichier de modèle (.gguf) est introuvable.
    #[error("model file not found: {path}")]
    ModelNotFound {
        /// Chemin vers le fichier attendu.
        path: PathBuf,
    },

    /// Erreur interne du moteur d'inférence.
    #[error("inference error: {0}")]
    InferenceError(String),

    /// Erreur HTTP retournée par un backend cloud.
    #[error("HTTP error {status}: {body}")]
    HttpError {
        /// Code HTTP (ex. 401, 429, 500).
        status: u16,
        /// Corps de la réponse d'erreur.
        body: String,
    },

    /// Variable d'environnement contenant la clé API absente.
    #[error("API key missing: env var '{var}' not set")]
    ApiKeyMissing {
        /// Nom de la variable d'environnement attendue.
        var: String,
    },

    /// Le `StepBudget` de l'agent a été épuisé pendant la boucle ReAct.
    #[error("step budget exhausted during tool loop")]
    BudgetExceeded,

    /// Le nombre maximum d'itérations de la boucle ReAct a été atteint.
    #[error("max tool iterations reached ({iterations})")]
    MaxIterationsReached {
        /// Nombre d'itérations configuré.
        iterations: u32,
    },

    /// La limite de tokens de génération a été atteinte.
    #[error("max tokens reached")]
    MaxTokensReached,

    /// Impossible de parser la réponse du backend.
    #[error("response parse error: {0}")]
    ParseError(String),

    /// Le modèle GGUF utilise une architecture non supportée par le moteur d'inférence.
    ///
    /// L'utilisateur doit choisir un modèle compatible (Llama, Mistral, Qwen2, Phi, etc.).
    #[error("unsupported model architecture '{architecture}' — try a Llama, Mistral, Qwen2, or Phi model instead")]
    UnsupportedModel {
        /// Nom de l'architecture GGUF non reconnue (ex. `"qwen35moe"`).
        architecture: String,
    },

    /// L'accélérateur demandé n'est pas compilé dans ce binaire.
    ///
    /// Recompiler avec la feature indiquée dans `hint` pour activer ce device.
    #[error("device '{device}' not available — recompile with --features {hint}")]
    DeviceNotAvailable {
        /// Nom du device demandé (ex. `"cuda"`, `"metal"`).
        device: String,
        /// Feature Cargo à activer (ex. `"local-cuda"`, `"local-metal"`).
        hint: String,
    },
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN a CompletionRequest built with only `messages`
    // WHEN optional fields are accessed
    // THEN they are None / empty
    #[test]
    fn test_ac2_completion_request_defaults() {
        let req = CompletionRequest {
            messages: vec![ChatMessage::user("hello")],
            ..Default::default()
        };

        assert!(req.model.is_none());
        assert!(req.temperature.is_none());
        assert!(req.max_tokens.is_none());
        assert!(req.tools.is_empty());
    }

    // GIVEN a BackendUnavailable error
    // WHEN formatted with Display
    // THEN message matches the #[error(...)] template
    #[test]
    fn test_ac3_llm_error_display_backend_unavailable() {
        let err = LlmError::BackendUnavailable {
            backend: "local".into(),
            reason: "model not loaded".into(),
        };
        assert_eq!(
            format!("{err}"),
            "backend 'local' unavailable: model not loaded"
        );
    }

    // GIVEN an ApiKeyMissing error
    // WHEN formatted
    // THEN message is correct
    #[test]
    fn test_ac3_llm_error_display_api_key_missing() {
        let err = LlmError::ApiKeyMissing {
            var: "ANTHROPIC_API_KEY".into(),
        };
        assert_eq!(
            format!("{err}"),
            "API key missing: env var 'ANTHROPIC_API_KEY' not set"
        );
    }

    // GIVEN a MaxIterationsReached error with 5 iterations
    // WHEN formatted
    // THEN message includes the count
    #[test]
    fn test_ac3_llm_error_display_max_iterations() {
        let err = LlmError::MaxIterationsReached { iterations: 5 };
        assert_eq!(format!("{err}"), "max tool iterations reached (5)");
    }

    // GIVEN text for system and user roles
    // WHEN ChatMessage helpers are called
    // THEN role and content match
    #[test]
    fn test_ac4_chat_message_helpers() {
        let sys = ChatMessage::system("tu es utile");
        let usr = ChatMessage::user("bonjour");
        let ast = ChatMessage::assistant("réponse");

        assert_eq!(sys.role, Role::System);
        assert_eq!(usr.role, Role::User);
        assert_eq!(ast.role, Role::Assistant);

        assert!(matches!(
            sys.content,
            MessageContent::Text(ref t) if t == "tu es utile"
        ));
        assert!(matches!(
            usr.content,
            MessageContent::Text(ref t) if t == "bonjour"
        ));
    }

    // GIVEN a tool_result message
    // WHEN constructed
    // THEN role is Tool and content is ToolResult
    #[test]
    fn test_ac4_chat_message_tool_result() {
        let msg = ChatMessage::tool_result("call_01", "fichier créé");
        assert_eq!(msg.role, Role::Tool);
        assert!(matches!(
            msg.content,
            MessageContent::ToolResult { ref tool_call_id, ref content }
            if tool_call_id == "call_01" && content == "fichier créé"
        ));
    }

    // GIVEN an assistant_with_calls message
    // WHEN constructed
    // THEN role is Assistant and content is WithToolCalls
    #[test]
    fn test_ac4_chat_message_assistant_with_calls() {
        let calls = vec![ToolCall {
            id: "c1".into(),
            name: "file_io".into(),
            arguments: serde_json::json!({}),
        }];
        let msg = ChatMessage::assistant_with_calls("je lis le fichier", &calls);
        assert_eq!(msg.role, Role::Assistant);
        assert!(matches!(
            msg.content,
            MessageContent::WithToolCalls { ref text, ref tool_calls }
            if text == "je lis le fichier" && tool_calls.len() == 1
        ));
    }

    // GIVEN a TokenUsage with no cost_usd
    // WHEN serialized to JSON
    // THEN cost_usd is "null" (not absent)
    #[test]
    fn test_ac5_token_usage_cost_usd_null_in_json() {
        let usage = TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 20,
            cost_usd: None,
        };
        let json = serde_json::to_string(&usage).expect("serialization must succeed");
        assert!(json.contains("\"cost_usd\":null"));
    }

    // GIVEN a ToolCall
    // WHEN serialized then deserialized
    // THEN fields are preserved
    #[test]
    fn test_tool_call_serde_roundtrip() {
        let call = ToolCall {
            id: "call_01".into(),
            name: "file_io".into(),
            arguments: serde_json::json!({"path": "/tmp/test.txt"}),
        };
        let json = serde_json::to_string(&call).expect("serialization must succeed");
        let back: ToolCall = serde_json::from_str(&json).expect("deserialization must succeed");
        assert_eq!(back.id, "call_01");
        assert_eq!(back.name, "file_io");
    }

    // GIVEN all LlmError variants
    // WHEN formatted
    // THEN none panics and messages are non-empty
    #[test]
    fn test_ac3_all_error_variants_display() {
        let errors: Vec<LlmError> = vec![
            LlmError::BackendUnavailable {
                backend: "b".into(),
                reason: "r".into(),
            },
            LlmError::ModelNotFound {
                path: std::path::PathBuf::from("/tmp/model.gguf"),
            },
            LlmError::InferenceError("engine crash".into()),
            LlmError::HttpError {
                status: 429,
                body: "rate limited".into(),
            },
            LlmError::ApiKeyMissing { var: "KEY".into() },
            LlmError::BudgetExceeded,
            LlmError::MaxIterationsReached { iterations: 3 },
            LlmError::MaxTokensReached,
            LlmError::ParseError("invalid json".into()),
            LlmError::UnsupportedModel {
                architecture: "qwen35moe".into(),
            },
            LlmError::DeviceNotAvailable {
                device: "cuda".into(),
                hint: "local-cuda".into(),
            },
        ];
        for err in &errors {
            assert!(
                !format!("{err}").is_empty(),
                "error display must not be empty: {err:?}"
            );
        }
    }
}
