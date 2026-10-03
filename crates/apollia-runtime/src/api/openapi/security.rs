//! The security scheme the contract declares, written into the document by a
//! [`Modify`] because the handler annotations say nothing about authentication.

use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::Modify;

/// Name of the single security scheme the contract declares.
///
/// Generated clients derive an identifier from this string, so it is part of
/// the published surface and is not renamed without regenerating them.
pub(super) const BEARER_SCHEME: &str = "bearerAuth";

/// Declares the Bearer token the TCP listener enforces.
///
/// `utoipa` builds the document from the handler annotations, which say nothing
/// about authentication; this modifier is where the transport-level rule of
/// `crates/apollia-runtime/src/api/middleware.rs` enters the spec. Without it
/// the document describes an API that needs no credential, which is what every
/// generated client and the published reference then say.
pub(super) struct BearerToken;

impl Modify for BearerToken {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            BEARER_SCHEME,
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some(
                        "Token read from `<data_dir>/api-token`, generated on first start \
                         and sent as `Authorization: Bearer <token>`. Enforced on the TCP \
                         listener by `TokenAuthLayer`, on every route except two: \
                         `GET /api/v1/health`, which reveals the product marker and the \
                         version only, and `POST /webhooks/{id}`, which authenticates its \
                         caller with an HMAC-SHA256 signature instead. Requests arriving on the Unix \
                         socket bypass the check: the socket file is mode 0600, so its \
                         permissions carry the same rule. Setting `[api] require_token = \
                         false` in `apollia.toml` removes the layer from the TCP listener \
                         altogether.",
                    ))
                    .build(),
            ),
        );
    }
}
