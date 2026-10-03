//! Client of the `/v1/embeddings` endpoint of the local engine.
//!
//! Turns texts into vectors through the `llama-server` instance the runtime
//! launches for embeddings. Three rules hold here rather than in each caller:
//!
//! - the prefix the registry entry declares is put before every text, the
//!   query prefix for a search, the passage prefix for a text to index, since
//!   a model compared without the prefix it was trained with ranks poorly;
//! - every vector comes back with a Euclidean norm of one, so a cosine is a dot
//!   product and two engines that do not normalise alike stay comparable;
//! - the endpoint is on this machine. A text to embed is the user's content,
//!   and the client refuses any address that is not the loopback, whatever the
//!   caller hands it.
//!
//! When no embeddings model is chosen, the caller gets
//! [`EmbeddingError::NoModel`], and search falls back to keywords alone
//! ([`EmbeddingError::falls_back_to_keywords`]).

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::recommend::{EmbeddingCatalogue, EmbeddingModel};

/// Texts sent in one request at most. A larger set goes out in several.
pub const MAX_BATCH: usize = 32;

/// Largest response body read, in bytes. A batch of [`MAX_BATCH`] vectors of
/// a few thousand floats written as JSON stays well under it.
const MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;

/// Deadline of one request. Embedding a batch on a CPU takes seconds, not
/// minutes; past this the engine is wedged.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// What a text is embedded for, which picks the prefix put before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmbedPurpose {
    /// A search query.
    Query,
    /// A passage to index.
    Passage,
}

/// Why texts could not be embedded.
#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub enum EmbeddingError {
    /// No embeddings model is chosen.
    #[error("no embeddings model is set")]
    NoModel,
    /// The setting naming the embeddings model could not be read.
    #[error("the embeddings setting could not be read: {0}")]
    Setting(String),
    /// The chosen identifier names no embeddings entry of the registry.
    #[error("{model} is not an embeddings model of the registry")]
    UnknownModel {
        /// The identifier that was asked for.
        model: String,
    },
    /// The chosen model has no file on this machine.
    #[error("the embeddings model {model} is not installed")]
    NotInstalled {
        /// Identifier of the registry entry.
        model: String,
    },
    /// No local engine can serve embeddings on this machine.
    #[error("no local engine can serve embeddings")]
    EngineUnavailable,
    /// The engine could not be started for the model.
    #[error("the embeddings engine could not start: {0}")]
    Engine(String),
    /// The address given is not on this machine.
    #[error("{url} is not a loopback address; embeddings stay on this machine")]
    NotLoopback {
        /// The address that was refused.
        url: String,
    },
    /// The request did not complete.
    #[error("the embeddings request failed: {0}")]
    Transport(String),
    /// The engine answered with an error status.
    #[error("the embeddings engine answered {status}: {body}")]
    Status {
        /// HTTP status code.
        status: u16,
        /// Start of the body, for the log.
        body: String,
    },
    /// The answer is not the shape `/v1/embeddings` returns.
    #[error("the embeddings answer is unreadable: {0}")]
    Decode(String),
    /// The engine returned another number of vectors than texts sent.
    #[error("{sent} text(s) sent, {received} vector(s) received")]
    CountMismatch {
        /// Texts in the request.
        sent: usize,
        /// Vectors in the answer.
        received: usize,
    },
    /// A vector has no direction and cannot be normalised.
    #[error("the vector of text {index} is zero")]
    ZeroVector {
        /// Position of the text in the request.
        index: usize,
    },
}

impl EmbeddingError {
    /// Whether search should go on with keywords alone rather than fail.
    ///
    /// True when there is nothing to embed with on this machine: no model
    /// chosen, a choice the registry no longer knows, a model not installed,
    /// or no engine. The other errors are failures of an engine that exists,
    /// and are reported as such.
    #[must_use]
    pub fn falls_back_to_keywords(&self) -> bool {
        matches!(
            self,
            Self::NoModel
                | Self::UnknownModel { .. }
                | Self::NotInstalled { .. }
                | Self::EngineUnavailable
        )
    }
}

/// The registry entry `default` names, or the typed error search reads as
/// "keywords alone".
///
/// # Errors
/// [`EmbeddingError::NoModel`] when `default` is `None`, and
/// [`EmbeddingError::UnknownModel`] when the catalogue has no such entry.
pub fn chosen_model<'a>(
    default: Option<&str>,
    catalogue: &'a EmbeddingCatalogue,
) -> Result<&'a EmbeddingModel, EmbeddingError> {
    let id = default.ok_or(EmbeddingError::NoModel)?;
    catalogue
        .get(id)
        .ok_or_else(|| EmbeddingError::UnknownModel {
            model: id.to_owned(),
        })
}

/// Whether `url` names this machine: `127.0.0.1` (or any `127/8` address),
/// `::1` or `localhost`, over HTTP.
#[must_use]
pub fn is_loopback_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "http" {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    match bare.parse::<std::net::IpAddr>() {
        Ok(ip) => ip.is_loopback(),
        Err(_) => bare.eq_ignore_ascii_case("localhost"),
    }
}

/// Scale `vector` to a Euclidean norm of one. `false` when it is zero (or
/// not finite) and has no direction to keep.
pub fn normalize(vector: &mut [f32]) -> bool {
    let norm = vector
        .iter()
        .map(|x| f64::from(*x) * f64::from(*x))
        .sum::<f64>()
        .sqrt();
    if norm == 0.0 || !norm.is_finite() {
        return false;
    }
    for x in vector.iter_mut() {
        *x = (f64::from(*x) / norm) as f32;
    }
    true
}

#[derive(Serialize)]
struct EmbeddingsRequest<'a> {
    model: &'a str,
    input: Vec<String>,
    encoding_format: &'static str,
}

#[derive(Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingItem>,
}

#[derive(Deserialize)]
struct EmbeddingItem {
    index: usize,
    embedding: Vec<f32>,
}

/// A client of the `/v1/embeddings` endpoint of one local engine, for one
/// registry entry.
#[derive(Debug, Clone)]
pub struct EmbeddingsClient {
    endpoint: String,
    model: EmbeddingModel,
    http: reqwest::Client,
}

impl EmbeddingsClient {
    /// A client of the engine at `base_url` (the OpenAI-compatible base, for
    /// example `http://127.0.0.1:8081/v1`) serving `model`.
    ///
    /// # Errors
    /// [`EmbeddingError::NotLoopback`] when `base_url` is not on this machine,
    /// and [`EmbeddingError::Transport`] when the HTTP client cannot be built.
    pub fn new(base_url: &str, model: EmbeddingModel) -> Result<Self, EmbeddingError> {
        if !is_loopback_url(base_url) {
            return Err(EmbeddingError::NotLoopback {
                url: base_url.to_owned(),
            });
        }
        let http = apollia_core::net::loopback_client_builder()
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| EmbeddingError::Transport(e.to_string()))?;
        Ok(Self {
            endpoint: format!("{}/embeddings", base_url.trim_end_matches('/')),
            model,
            http,
        })
    }

    /// The registry entry this client embeds with.
    #[must_use]
    pub fn model(&self) -> &EmbeddingModel {
        &self.model
    }

    /// One normalised vector per text, in the order of `texts`.
    ///
    /// The prefix of `purpose` is put before every text, and the texts go out
    /// in requests of [`MAX_BATCH`] at most.
    ///
    /// # Errors
    /// Any [`EmbeddingError`] of the request, the answer or a vector.
    pub async fn embed(
        &self,
        texts: &[String],
        purpose: EmbedPurpose,
    ) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let prefix = match purpose {
            EmbedPurpose::Query => &self.model.query_prefix,
            EmbedPurpose::Passage => &self.model.passage_prefix,
        };
        let mut vectors = Vec::with_capacity(texts.len());
        for (offset, batch) in (0..).step_by(MAX_BATCH).zip(texts.chunks(MAX_BATCH)) {
            let input = batch.iter().map(|t| format!("{prefix}{t}")).collect();
            let mut answered = self.request(input).await?;
            for (i, vector) in answered.iter_mut().enumerate() {
                if !normalize(vector) {
                    return Err(EmbeddingError::ZeroVector { index: offset + i });
                }
            }
            vectors.extend(answered);
        }
        Ok(vectors)
    }

    /// Send one batch, and return its vectors in the order sent.
    async fn request(&self, input: Vec<String>) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let sent = input.len();
        let body = EmbeddingsRequest {
            model: &self.model.id,
            input,
            encoding_format: "float",
        };
        let response = self
            .http
            .post(&self.endpoint)
            .json(&body)
            .send()
            .await
            .map_err(|e| EmbeddingError::Transport(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            let text = apollia_core::net::read_capped_text(response, 4096)
                .await
                .unwrap_or_default();
            tracing::event!(
                tracing::Level::WARN,
                status = status.as_u16(),
                model = %self.model.id,
                "llm.embeddings.status"
            );
            return Err(EmbeddingError::Status {
                status: status.as_u16(),
                body: text.chars().take(512).collect(),
            });
        }
        let mut parsed: EmbeddingsResponse =
            apollia_core::net::read_capped_json(response, MAX_RESPONSE_BYTES)
                .await
                .map_err(|e| EmbeddingError::Decode(e.to_string()))?;
        if parsed.data.len() != sent {
            return Err(EmbeddingError::CountMismatch {
                sent,
                received: parsed.data.len(),
            });
        }
        parsed.data.sort_by_key(|item| item.index);
        if parsed
            .data
            .iter()
            .enumerate()
            .any(|(i, item)| item.index != i)
        {
            return Err(EmbeddingError::Decode(
                "the indices of the vectors do not match the texts sent".to_owned(),
            ));
        }
        Ok(parsed.data.into_iter().map(|item| item.embedding).collect())
    }
}

#[cfg(test)]
mod tests;
