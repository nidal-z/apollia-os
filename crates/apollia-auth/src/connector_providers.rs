//! OAuth2 connector providers (Google Workspace, Microsoft 365).
//!
//! This module is separate from [`crate::providers`] which configures the LLM
//! OAuth providers (Anthropic, OpenAI, Vertex AI). Connector providers expose:
//!
//! - A typed scope catalog so the UI can offer service-level toggles
//!   (Gmail / Calendar / Drive) without leaking raw OAuth scope strings.
//! - An [`build_connector_provider`] factory that resolves the catalog into a
//!   runtime [`ProviderConfig`] with the requested scopes, plus the
//!   discovery endpoints (`userinfo_url`).
//!
//! # Scope policy: v0.1.0 power user
//!
//! The default tier requests only scopes Google classifies as **sensitive** or
//! **non-sensitive** (free verification). Restricted scopes (`gmail.readonly`,
//! `gmail.modify`, `gmail.compose`, `drive.readonly`, `drive`) require a Google
//! CASA security assessment and would shift the cost model, so they are NOT in
//! the default set. Draft creation uses the sensitive `gmail.drafts.create`
//! scope; full draft management (`gmail.compose`, now restricted) and inbox
//! read stay behind Expert Mode (the user brings their own OAuth client and
//! consents to the restricted scopes).

use crate::providers::ProviderConfig;

// ─── Google scope catalog ────────────────────────────────────────────────────

/// User-facing scope groups for Google Workspace.
///
/// Each variant maps to one or more raw OAuth scopes; never expose the raw
/// strings in the UI, route through this enum so the policy stays in one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GoogleScope {
    /// Send mail via Gmail (`gmail.send`). Sensitive scope, free OAuth verification.
    MailSend,
    /// Create drafts (`gmail.drafts.create`). Sensitive, free OAuth
    /// verification. Default draft capability; cannot list or delete drafts.
    MailDraftsCreate,
    /// Create / list / delete drafts (`gmail.compose`). RESTRICTED scope:
    /// Google reclassified `gmail.compose` as restricted, so it now requires a
    /// CASA security assessment. Reserved for Expert Mode (the user's own OAuth
    /// client), never requested by the default tier.
    MailCompose,
    /// Read-only calendar access (`calendar.readonly`). Sensitive, free.
    CalendarRead,
    /// Read/write calendar events (`calendar.events`). Sensitive, free.
    CalendarWrite,
    /// Workspace-scoped Drive access (`drive.file`). Non-restricted, free.
    ///
    /// With this scope, an app can read and write only the files it has created
    /// itself OR that the user has explicitly opened with the app. Apollia uses
    /// this to back the Drive Workspace pattern (folder `Apollia/<agent>/`).
    DriveWorkspace,
    /// Google Sheets read+write (`spreadsheets`). Non-sensitive, free.
    /// Restricted to sheets the app creates or the user pickers; same
    /// pattern as `drive.file`.
    SheetsReadWrite,
    /// Google Docs read+write (`documents`). Non-sensitive, free. Same
    /// `drive.file`-style scoping.
    DocsReadWrite,
    /// Google Slides read+write (`presentations`). Non-sensitive, free.
    SlidesReadWrite,
    /// Google Forms read+write (`forms.body`). Non-sensitive, free.
    /// Limited to forms the app creates or the user opens with it.
    FormsReadWrite,
    /// Google Tasks full access (`tasks`). Non-sensitive, free.
    /// Covers all of the user's task lists; no per-resource gate exists.
    Tasks,
    /// YouTube read-only (`youtube.readonly`). Non-sensitive, free.
    /// Search, video metadata, channel listings.
    YouTubeReadOnly,
    /// OpenID Connect identity scope.
    OpenId,
    /// User email scope (returned by userinfo).
    Email,
    /// User profile scope (name, picture).
    Profile,
}

impl GoogleScope {
    /// Return the raw OAuth scope string for this scope group.
    pub const fn oauth_scope(self) -> &'static str {
        match self {
            Self::MailSend => "https://www.googleapis.com/auth/gmail.send",
            Self::MailDraftsCreate => "https://www.googleapis.com/auth/gmail.drafts.create",
            Self::MailCompose => "https://www.googleapis.com/auth/gmail.compose",
            Self::CalendarRead => "https://www.googleapis.com/auth/calendar.readonly",
            Self::CalendarWrite => "https://www.googleapis.com/auth/calendar.events",
            Self::DriveWorkspace => "https://www.googleapis.com/auth/drive.file",
            Self::SheetsReadWrite => "https://www.googleapis.com/auth/spreadsheets",
            Self::DocsReadWrite => "https://www.googleapis.com/auth/documents",
            Self::SlidesReadWrite => "https://www.googleapis.com/auth/presentations",
            Self::FormsReadWrite => "https://www.googleapis.com/auth/forms.body",
            Self::Tasks => "https://www.googleapis.com/auth/tasks",
            Self::YouTubeReadOnly => "https://www.googleapis.com/auth/youtube.readonly",
            Self::OpenId => "openid",
            Self::Email => "email",
            Self::Profile => "profile",
        }
    }

    /// Profile scopes always requested to resolve the connected account email.
    pub const fn default_profile() -> [Self; 3] {
        [Self::OpenId, Self::Email, Self::Profile]
    }
}

// ─── Microsoft scope catalog ─────────────────────────────────────────────────

/// User-facing scope groups for Microsoft 365 (Graph API).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MicrosoftScope {
    /// Read user mail (`Mail.Read`). No CASA equivalent on Microsoft side.
    MailRead,
    /// Send mail as the user (`Mail.Send`).
    MailSend,
    /// Read calendar events (`Calendars.Read`).
    CalendarRead,
    /// Create / update / delete calendar events (`Calendars.ReadWrite`).
    CalendarWrite,
    /// Read all files the user can access in OneDrive / SharePoint (`Files.Read.All`).
    FilesRead,
    /// Read / write files (`Files.ReadWrite`).
    FilesWrite,
    /// Sign-in + read basic profile (`User.Read`).
    Profile,
    /// Refresh tokens (`offline_access`), required to obtain a refresh token.
    Offline,
}

impl MicrosoftScope {
    /// Return the raw OAuth scope string.
    pub const fn oauth_scope(self) -> &'static str {
        match self {
            Self::MailRead => "Mail.Read",
            Self::MailSend => "Mail.Send",
            Self::CalendarRead => "Calendars.Read",
            Self::CalendarWrite => "Calendars.ReadWrite",
            Self::FilesRead => "Files.Read.All",
            Self::FilesWrite => "Files.ReadWrite",
            Self::Profile => "User.Read",
            Self::Offline => "offline_access",
        }
    }

    /// Scopes always requested to resolve account identity + refresh.
    pub const fn default_baseline() -> [Self; 2] {
        [Self::Profile, Self::Offline]
    }
}

// ─── Connector provider identifier ───────────────────────────────────────────

/// Identifier for a SaaS connector OAuth provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConnectorProvider {
    /// Google Workspace (Gmail, Calendar, Drive).
    Google,
    /// Microsoft 365 (Outlook, Calendar, OneDrive).
    Microsoft,
}

impl ConnectorProvider {
    /// Stable string identifier used in keyring namespacing.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Microsoft => "microsoft",
        }
    }

    /// Userinfo endpoint URL returning the connected account identity.
    pub const fn userinfo_url(self) -> &'static str {
        match self {
            Self::Google => "https://www.googleapis.com/oauth2/v3/userinfo",
            Self::Microsoft => "https://graph.microsoft.com/v1.0/me",
        }
    }

    /// Environment variable that overrides the compiled-in OAuth client ID.
    ///
    /// **Not part of the user-facing flow.** This override targets Expert
    /// Mode power users who run their own OAuth app (see the Expert Mode
    /// section of `connecter-google-workspace.md`) and developers running a
    /// dev build against a non-production AS. End users never set this; the
    /// compiled-in [`default_client_id`](Self::default_client_id) is used and
    /// shipped in the binary.
    pub const fn client_id_env_var(self) -> &'static str {
        match self {
            Self::Google => "APOLLIA_GOOGLE_CLIENT_ID",
            Self::Microsoft => "APOLLIA_MICROSOFT_CLIENT_ID",
        }
    }

    /// Environment variable that overrides the compiled-in OAuth client secret.
    /// Only meaningful for Google (Installed App type requires a secret at
    /// the token endpoint, see `ProviderConfig::client_secret`). Microsoft
    /// public clients leave this empty.
    pub const fn client_secret_env_var(self) -> &'static str {
        match self {
            Self::Google => "APOLLIA_GOOGLE_CLIENT_SECRET",
            Self::Microsoft => "APOLLIA_MICROSOFT_CLIENT_SECRET",
        }
    }

    /// Compiled-in default OAuth client ID shipped with Apollia.
    ///
    /// Apollia is a public OAuth client (PKCE, no client secret), so it is
    /// safe to embed the client ID directly in the binary. Released builds
    /// substitute the real values produced by the project's Google Cloud /
    /// Azure AD apps at build time.
    ///
    /// Returns an empty string in dev / unconfigured builds; the runtime
    /// surfaces a clear "OAuth client not configured" error in that case
    /// rather than panicking.
    pub const fn default_client_id(self) -> &'static str {
        match self {
            // Replaced at release build time with the production client IDs.
            // Dev / open-source builds default to empty; the runtime then
            // surfaces an explicit "OAuth client not configured" error.
            Self::Google => env_or_empty(option_env!("APOLLIA_BUILD_GOOGLE_CLIENT_ID")),
            Self::Microsoft => env_or_empty(option_env!("APOLLIA_BUILD_MICROSOFT_CLIENT_ID")),
        }
    }

    /// Compiled-in default OAuth client secret. Returns an empty string for
    /// Microsoft (public client, no secret per spec) and the build-time
    /// constant for Google (Installed App requires a secret per Google's
    /// non-standard implementation). Replaced at release build time.
    pub const fn default_client_secret(self) -> &'static str {
        match self {
            Self::Google => env_or_empty(option_env!("APOLLIA_BUILD_GOOGLE_CLIENT_SECRET")),
            Self::Microsoft => "",
        }
    }

    /// Runtime env var that overrides the compiled-in Google API key.
    /// Google-only, used by Google Picker. Microsoft picker equivalent
    /// (OneDrive File Picker) would follow the same pattern when added.
    pub const fn api_key_env_var(self) -> &'static str {
        match self {
            Self::Google => "APOLLIA_GOOGLE_API_KEY",
            Self::Microsoft => "APOLLIA_MICROSOFT_API_KEY",
        }
    }

    /// Compiled-in default Google API key shipped with Apollia (Picker
    /// requires it alongside the OAuth token). Replaced at release build
    /// time. Microsoft returns empty for now.
    pub const fn default_api_key(self) -> &'static str {
        match self {
            Self::Google => env_or_empty(option_env!("APOLLIA_BUILD_GOOGLE_API_KEY")),
            Self::Microsoft => "",
        }
    }

    /// Resolve the effective OAuth client ID at runtime.
    ///
    /// Priority order:
    /// 1. Runtime env var override (`APOLLIA_GOOGLE_CLIENT_ID` /
    ///    `APOLLIA_MICROSOFT_CLIENT_ID`), handy for one-off shell sessions
    ///    and CI.
    /// 2. User-editable `~/.apollia/oauth-clients.toml`, populated by the
    ///    Settings → Integrations panel, so a power user running a custom
    ///    build can plug in their own Google / Microsoft client IDs without
    ///    rebuilding the binary.
    /// 3. Build-time compiled default (`APOLLIA_BUILD_*_CLIENT_ID`), what
    ///    the official desktop release uses.
    ///
    /// Returns `None` when all sources are absent so the UI can surface a
    /// clear "OAuth not configured" message instead of failing mid-handshake.
    pub fn resolve_client_id(self) -> Option<String> {
        if let Ok(v) = std::env::var(self.client_id_env_var()) {
            if !v.is_empty() {
                return Some(v);
            }
        }
        if let Some(v) = crate::oauth_clients_file::lookup_client_id(self.id()) {
            return Some(v);
        }
        let compiled = self.default_client_id();
        if compiled.is_empty() {
            None
        } else {
            Some(compiled.to_owned())
        }
    }

    /// Resolve the effective Google API key at runtime: same 3-source
    /// priority chain as [`resolve_client_id`](Self::resolve_client_id).
    /// `None` is the normal case for Microsoft and for any dev build that
    /// hasn't been provisioned with a key. The Picker UI surfaces a clear
    /// "API key missing" error to direct the operator to Settings.
    pub fn resolve_api_key(self) -> Option<String> {
        if let Ok(v) = std::env::var(self.api_key_env_var()) {
            if !v.is_empty() {
                return Some(v);
            }
        }
        if let Some(v) = crate::oauth_clients_file::lookup_api_key(self.id()) {
            return Some(v);
        }
        let compiled = self.default_api_key();
        if compiled.is_empty() {
            None
        } else {
            Some(compiled.to_owned())
        }
    }

    /// Resolve the effective OAuth client secret at runtime: same 3-source
    /// priority chain as [`resolve_client_id`](Self::resolve_client_id).
    /// Returns `None` when no source provides a secret (the normal case for
    /// Microsoft, spec-compliant public clients don't carry one).
    pub fn resolve_client_secret(self) -> Option<String> {
        if let Ok(v) = std::env::var(self.client_secret_env_var()) {
            if !v.is_empty() {
                return Some(v);
            }
        }
        if let Some(v) = crate::oauth_clients_file::lookup_client_secret(self.id()) {
            return Some(v);
        }
        let compiled = self.default_client_secret();
        if compiled.is_empty() {
            None
        } else {
            Some(compiled.to_owned())
        }
    }
}

const fn env_or_empty(opt: Option<&'static str>) -> &'static str {
    match opt {
        Some(s) => s,
        None => "",
    }
}

// ─── Factory ─────────────────────────────────────────────────────────────────

/// Build a [`ProviderConfig`] for a Google connector with the requested scopes.
///
/// Profile scopes ([`GoogleScope::OpenId`], [`Email`](GoogleScope::Email),
/// [`Profile`](GoogleScope::Profile)) are added implicitly so the userinfo
/// endpoint resolves the account email.
pub fn build_google_provider(scopes: &[GoogleScope]) -> ProviderConfig {
    let mut all_scopes: Vec<&'static str> = scopes.iter().map(|s| s.oauth_scope()).collect();
    for s in GoogleScope::default_profile() {
        let raw = s.oauth_scope();
        if !all_scopes.contains(&raw) {
            all_scopes.push(raw);
        }
    }
    ProviderConfig {
        name: "google",
        auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
        token_url: "https://oauth2.googleapis.com/token",
        client_id: ConnectorProvider::Google
            .resolve_client_id()
            .unwrap_or_default(),
        client_secret: ConnectorProvider::Google.resolve_client_secret(),
        scopes: all_scopes,
    }
}

/// Build a [`ProviderConfig`] for a Microsoft connector with the requested scopes.
///
/// Baseline scopes ([`MicrosoftScope::Profile`], [`Offline`](MicrosoftScope::Offline))
/// are added implicitly. The auth endpoint is multi-tenant (`/common/`); for
/// single-tenant deployments, an override is provided through
/// [`build_microsoft_provider_for_tenant`].
pub fn build_microsoft_provider(scopes: &[MicrosoftScope]) -> ProviderConfig {
    build_microsoft_provider_for_tenant(scopes, "common")
}

/// Build a Microsoft [`ProviderConfig`] scoped to a specific Azure AD tenant.
///
/// Pass `"common"` for multi-tenant (any Microsoft / Azure AD work account).
/// Pass a tenant GUID for a locked-down enterprise deployment.
pub fn build_microsoft_provider_for_tenant(
    scopes: &[MicrosoftScope],
    tenant: &str,
) -> ProviderConfig {
    let mut all_scopes: Vec<&'static str> = scopes.iter().map(|s| s.oauth_scope()).collect();
    for s in MicrosoftScope::default_baseline() {
        let raw = s.oauth_scope();
        if !all_scopes.contains(&raw) {
            all_scopes.push(raw);
        }
    }
    // Microsoft auth/token URLs are tenant-scoped. The tenant segment is
    // injected via leaked strings since ProviderConfig stores &'static str.
    let auth_url: &'static str = Box::leak(
        format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize")
            .into_boxed_str(),
    );
    let token_url: &'static str = Box::leak(
        format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token").into_boxed_str(),
    );
    ProviderConfig {
        name: "microsoft",
        auth_url,
        token_url,
        client_id: ConnectorProvider::Microsoft
            .resolve_client_id()
            .unwrap_or_default(),
        // Spec-compliant public client, no secret. resolve_client_secret
        // returns None unless someone explicitly forced one via env var (rare).
        client_secret: ConnectorProvider::Microsoft.resolve_client_secret(),
        scopes: all_scopes,
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_google_scope_strings_are_correct() {
        // GIVEN the gmail.send scope group
        // WHEN resolved to oauth scope
        // THEN matches the Google API documented value
        assert_eq!(
            GoogleScope::MailSend.oauth_scope(),
            "https://www.googleapis.com/auth/gmail.send"
        );
        assert_eq!(
            GoogleScope::DriveWorkspace.oauth_scope(),
            "https://www.googleapis.com/auth/drive.file"
        );
        assert_eq!(
            GoogleScope::CalendarWrite.oauth_scope(),
            "https://www.googleapis.com/auth/calendar.events"
        );
    }

    #[test]
    fn test_microsoft_scope_strings_are_correct() {
        // GIVEN the Mail.Read scope group
        // WHEN resolved
        // THEN matches Microsoft Graph documented value
        assert_eq!(MicrosoftScope::MailRead.oauth_scope(), "Mail.Read");
        assert_eq!(MicrosoftScope::Offline.oauth_scope(), "offline_access");
    }

    #[test]
    fn test_build_google_provider_includes_profile_scopes_implicitly() {
        // GIVEN a request for only MailSend
        let cfg = build_google_provider(&[GoogleScope::MailSend]);
        // THEN the resulting scope set includes openid + email + profile
        assert!(cfg.scopes.contains(&"openid"));
        assert!(cfg.scopes.contains(&"email"));
        assert!(cfg.scopes.contains(&"profile"));
        assert!(cfg
            .scopes
            .contains(&"https://www.googleapis.com/auth/gmail.send"));
    }

    #[test]
    fn test_build_microsoft_provider_includes_baseline_scopes_implicitly() {
        // GIVEN a request for only MailRead
        let cfg = build_microsoft_provider(&[MicrosoftScope::MailRead]);
        // THEN the resulting scope set includes User.Read + offline_access
        assert!(cfg.scopes.contains(&"User.Read"));
        assert!(cfg.scopes.contains(&"offline_access"));
        assert!(cfg.scopes.contains(&"Mail.Read"));
    }

    #[test]
    fn test_build_microsoft_provider_default_is_common_tenant() {
        let cfg = build_microsoft_provider(&[MicrosoftScope::MailRead]);
        assert!(cfg.auth_url.contains("/common/"));
        assert!(cfg.token_url.contains("/common/"));
    }

    #[test]
    fn test_build_microsoft_provider_for_specific_tenant() {
        let tenant = "00000000-0000-0000-0000-000000000000";
        let cfg = build_microsoft_provider_for_tenant(&[MicrosoftScope::MailRead], tenant);
        assert!(cfg.auth_url.contains(tenant));
        assert!(cfg.token_url.contains(tenant));
    }

    #[test]
    fn test_no_duplicate_scopes_when_profile_explicitly_requested() {
        // GIVEN a request that explicitly includes Email
        let cfg = build_google_provider(&[GoogleScope::MailSend, GoogleScope::Email]);
        // THEN "email" appears once, not twice
        let email_count = cfg.scopes.iter().filter(|s| **s == "email").count();
        assert_eq!(email_count, 1);
    }

    #[test]
    fn test_connector_provider_ids_are_stable() {
        assert_eq!(ConnectorProvider::Google.id(), "google");
        assert_eq!(ConnectorProvider::Microsoft.id(), "microsoft");
    }
}
