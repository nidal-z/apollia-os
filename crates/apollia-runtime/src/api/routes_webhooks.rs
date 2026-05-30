//! Route `POST /webhooks/:id`, réception de webhooks avec vérification HMAC-SHA256.
//!
//! **Ordre des vérifications (Principe #4, Fail fast) :**
//! 1. TriggerEngine disponible ? → 503
//! 2. Trigger connu et de type webhook ? → 404
//! 3. Header `X-Apollia-Signature` présent ? → 401
//! 4. Signature HMAC-SHA256 correcte ? → 401
//! 5. Forward vers `TriggerEngineHandle::send_webhook_event()` → 200

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

/// Handler axum pour `POST /webhooks/:id`.
///
/// Vérifie la signature HMAC-SHA256 de la requête avant de forwarder l'événement
/// au [`TriggerEngineHandle`]. Retourne 503 si le TriggerEngine n'est pas démarré,
/// 404 si le trigger est inconnu, 401 si la signature est absente ou invalide, 200 sinon.
pub async fn handle_webhook<B: ExecutionBackend + Clone>(
    Path(trigger_id): Path<String>,
    State(state): State<AppState<B>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // 0. TriggerEngine disponible ?
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };

    // 1. Trouver la définition du trigger (webhook uniquement)
    let def = match engine.find_webhook(&trigger_id).await {
        Some(d) => d,
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    // 2. Récupérer la signature du header
    let signature = match headers
        .get("X-Apollia-Signature")
        .and_then(|v| v.to_str().ok())
    {
        Some(s) => s.to_owned(),
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    // 3. Extraire le secret depuis la définition du trigger
    let secret = match &def.source {
        TriggerSourceConfig::Webhook { secret } => secret.clone(),
        _ => {
            // Ne peut pas arriver : find_webhook filtre déjà les non-webhook
            tracing::error!(trigger_id = %trigger_id, "source non-webhook retournée par find_webhook");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // 4. Vérifier HMAC-SHA256 (constant-time)
    if !verify_hmac(&secret, &body, &signature) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    // 5. Forwarder vers TriggerEngine (fire-and-forget)
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

/// Vérifie la signature HMAC-SHA256 d'un body.
///
/// La signature doit être au format `"sha256=<hex>"`.
/// Utilise [`constant_time_eq::constant_time_eq`] pour éviter les timing attacks.
///
/// Retourne `true` uniquement si la signature est correcte.
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

    // Comparaison constante-time, évite les timing attacks
    constant_time_eq::constant_time_eq(computed.as_bytes(), expected.as_bytes())
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Calcule la signature HMAC-SHA256 d'un body pour les tests.
    fn compute_hmac(secret: &str, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn test_ac5_verify_hmac_correct_signature() {
        // GIVEN
        let secret = "mon-secret";
        let body = b"payload";
        let sig = compute_hmac(secret, body);
        // WHEN / THEN
        assert!(verify_hmac(secret, body, &sig));
    }

    #[test]
    fn test_ac5_verify_hmac_wrong_signature() {
        // GIVEN, signature de même longueur mais incorrecte
        let sig = "sha256=0000000000000000000000000000000000000000000000000000000000000000";
        // WHEN / THEN
        assert!(!verify_hmac("secret", b"body", sig));
    }

    #[test]
    fn test_ac5_verify_hmac_missing_prefix() {
        // GIVEN, signature sans le préfixe "sha256="
        let sig = "deadbeef";
        // WHEN / THEN
        assert!(!verify_hmac("secret", b"body", sig));
    }

    #[test]
    fn test_ac5_verify_hmac_wrong_body() {
        // GIVEN, signature calculée sur un body différent
        let secret = "mon-secret";
        let sig = compute_hmac(secret, b"payload");
        // WHEN / THEN, body différent
        assert!(!verify_hmac(secret, b"other-payload", &sig));
    }
}
