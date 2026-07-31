//! OAuth2 provider configurations supported by Apollia auth.

// ─── Types ────────────────────────────────────────────────────────────────────

/// OAuth2 provider configuration for a supported LLM cloud service.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// Short identifier used as keyring account name and display label.
    pub name: &'static str,
    /// Authorization endpoint URL.
    pub auth_url: &'static str,
    /// Token exchange endpoint URL.
    pub token_url: &'static str,
    /// OAuth2 client identifier, read from the environment at runtime.
    pub client_id: String,
    /// Optional OAuth client secret. Sent at the token endpoint when present.
    ///
    /// **Why optional**: Apollia is an OAuth 2.1 public client and uses PKCE,
    /// which per spec replaces the client secret. **However** Google's
    /// "Installed app" / "Desktop app" type requires sending a `client_secret`
    /// at the token endpoint anyway. Google explicitly documents that this
    /// secret "is obviously not treated as a secret" for native apps
    /// (developers.google.com/identity/protocols/oauth2/native-app). Microsoft
    /// public clients honour the spec and leave this empty.
    pub client_secret: Option<String>,
    /// Requested OAuth2 scopes.
    pub scopes: Vec<&'static str>,
}

// ─── Registry ─────────────────────────────────────────────────────────────────

/// Provider identifiers for the LLM-provider OAuth table.
///
/// The `apollia-os auth` command group that consumed this table was removed:
/// it stored a token under its own keyring service that no backend ever read.
/// The router resolves a cloud key from `config_json["api_key"]` only. This
/// module is kept for an embedder that wires its own provider flow.
pub const SUPPORTED_PROVIDERS: &[&str] = &["anthropic", "openai", "vertex"];

/// Return the [`ProviderConfig`] for the named provider, or `None` if unknown.
///
/// The `client_id` is read from the corresponding environment variable:
/// - `anthropic` → `ANTHROPIC_CLIENT_ID`
/// - `openai`    → `OPENAI_CLIENT_ID`
/// - `vertex`    → `GOOGLE_CLIENT_ID`
pub fn get_provider(name: &str) -> Option<ProviderConfig> {
    match name {
        "anthropic" => Some(ProviderConfig {
            name: "anthropic",
            auth_url: "https://api.anthropic.com/oauth/authorize",
            token_url: "https://api.anthropic.com/oauth/token",
            client_id: std::env::var("ANTHROPIC_CLIENT_ID").unwrap_or_default(),
            client_secret: None,
            scopes: vec!["api"],
        }),
        "openai" => Some(ProviderConfig {
            name: "openai",
            auth_url: "https://platform.openai.com/oauth/authorize",
            token_url: "https://platform.openai.com/oauth/token",
            client_id: std::env::var("OPENAI_CLIENT_ID").unwrap_or_default(),
            client_secret: None,
            scopes: vec!["api"],
        }),
        "vertex" => Some(ProviderConfig {
            name: "vertex",
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
            token_url: "https://oauth2.googleapis.com/token",
            client_id: std::env::var("GOOGLE_CLIENT_ID").unwrap_or_default(),
            client_secret: None,
            scopes: vec!["https://www.googleapis.com/auth/cloud-platform"],
        }),
        _ => None,
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unknown_provider_returns_none() {
        // GIVEN a provider name that is not supported
        // WHEN
        let result = get_provider("github");
        // THEN None
        assert!(result.is_none());
    }

    #[test]
    fn test_supported_providers_list() {
        // GIVEN the SUPPORTED_PROVIDERS constant
        // WHEN inspected
        // THEN contains exactly ["anthropic", "openai", "vertex"]
        assert_eq!(SUPPORTED_PROVIDERS, &["anthropic", "openai", "vertex"]);
    }

    #[test]
    fn test_known_providers_return_some() {
        for name in SUPPORTED_PROVIDERS {
            assert!(get_provider(name).is_some(), "expected Some for '{name}'");
        }
    }
}
