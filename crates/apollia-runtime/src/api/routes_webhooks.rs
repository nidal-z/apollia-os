//! Route `POST /webhooks/:id`, webhook reception with HMAC-SHA256 verification.
//!
//! Verification order (fail fast):
//! 1. TriggerEngine available? -> 503
//! 2. Trigger known and of webhook type? -> 404
//! 3. `X-Apollia-Signature` header present? -> 401
//! 4. HMAC-SHA256 signature correct? -> 401
//! 5. Forward to `TriggerEngineHandle::send_webhook_event()` -> 200

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use apollia_triggers::TriggerSourceConfig;

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

// ─── Handler ──────────────────────────────────────────────────────────────

/// Axum handler for `POST /webhooks/:id`.
///
/// Verifies the request HMAC-SHA256 signature before forwarding the event to
/// the [`TriggerEngineHandle`]. Returns 503 if the TriggerEngine is not started,
/// 404 if the trigger is unknown, 401 if the signature is absent or invalid, 200 otherwise.
#[utoipa::path(
    post,
    path = "/webhooks/{id}",
    tag = "webhooks",
    params(("id" = String, Path, description = "Webhook trigger id")),
    // The one operation outside the document-level Bearer requirement. An
    // outside sender has no API token, so `TokenAuthLayer` lets this path
    // through and the handler below authenticates the caller itself, with an
    // HMAC-SHA256 of the raw body under the per-trigger secret.
    security(()),
    request_body(content_type = "application/octet-stream", description = "Raw webhook payload, verified against the `X-Apollia-Signature` HMAC-SHA256 header"),
    responses(
        (status = 200, description = "Webhook accepted and forwarded"),
        (status = 401, description = "Missing or invalid signature"),
        (status = 404, description = "Unknown webhook trigger"),
        (status = 500, description = "Internal error"),
        (status = 503, description = "Trigger engine unavailable"),
    )
)]
pub async fn handle_webhook<B: ExecutionBackend + Clone>(
    Path(trigger_id): Path<String>,
    State(state): State<AppState<B>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // 0. TriggerEngine available?
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };

    // 1. Find the trigger definition (webhook only)
    let def = match engine.find_webhook(&trigger_id).await {
        Some(d) => d,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    // 2. Read the signature from the header
    let signature = match headers
        .get("X-Apollia-Signature")
        .and_then(|v| v.to_str().ok())
    {
        Some(s) => s.to_owned(),
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    // 3. Extract the secret from the trigger definition
    let secret = match &def.source {
        TriggerSourceConfig::Webhook { secret } => secret.clone(),
        _ => {
            // Unreachable: find_webhook already filters out non-webhook sources
            tracing::error!(trigger_id = %trigger_id, "trigger.webhook.source.mismatch");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // 4. Verify HMAC-SHA256 (constant-time)
    if !verify_hmac(&secret, &body, &signature) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // 5. Forward to TriggerEngine (fire-and-forget)
    let body_str = String::from_utf8_lossy(&body).into_owned();
    let hdrs = headers
        .iter()
        .filter_map(|(k, v)| {
            v.to_str()
                .ok()
                .map(|s| (k.as_str().to_owned(), s.to_owned()))
        })
        .collect();

    engine.send_webhook_event(trigger_id, body_str, hdrs).await;

    StatusCode::OK.into_response()
}

// ─── verify_hmac ──────────────────────────────────────────────────────────

/// Verifies the HMAC-SHA256 signature of a body.
///
/// The signature must be in the `"sha256=<hex>"` format.
/// Uses [`constant_time_eq::constant_time_eq`] to avoid timing attacks.
///
/// Returns `true` only if the signature is correct.
pub fn verify_hmac(secret: &str, body: &[u8], signature: &str) -> bool {
    let expected = match signature.strip_prefix("sha256=") {
        Some(s) => s,
        None => return false,
    };

    let mut mac = match Hmac::<Sha256>::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);
    let computed = hex::encode(mac.finalize().into_bytes());

    // Constant-time comparison, avoids timing attacks
    constant_time_eq::constant_time_eq(computed.as_bytes(), expected.as_bytes())
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Computes the HMAC-SHA256 signature of a body for the tests.
    fn compute_hmac(secret: &str, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn test_verify_hmac_correct_signature() {
        // GIVEN
        let secret = "mon-secret";
        let body = b"payload";
        let sig = compute_hmac(secret, body);
        // WHEN / THEN
        assert!(verify_hmac(secret, body, &sig));
    }

    #[test]
    fn test_verify_hmac_wrong_signature() {
        // GIVEN, signature of the same length but incorrect
        let sig = "sha256=0000000000000000000000000000000000000000000000000000000000000000";
        // WHEN / THEN
        assert!(!verify_hmac("secret", b"body", sig));
    }

    #[test]
    fn test_verify_hmac_missing_prefix() {
        // GIVEN, signature without the "sha256=" prefix
        let sig = "deadbeef";
        // WHEN / THEN
        assert!(!verify_hmac("secret", b"body", sig));
    }

    #[test]
    fn test_verify_hmac_wrong_body() {
        // GIVEN, signature computed over a different body
        let secret = "mon-secret";
        let sig = compute_hmac(secret, b"payload");
        // WHEN / THEN, different body
        assert!(!verify_hmac(secret, b"other-payload", &sig));
    }
}
