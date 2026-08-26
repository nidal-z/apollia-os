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
//! # Client provisioning is asymmetric
//!
//! Microsoft 365 connects out of the box: Apollia ships the client id of its
//! own public client registration ([`MICROSOFT_DEFAULT_CLIENT_ID`]). Google
//! Workspace does not, because Google requires a verified consent screen
//! before an application may serve accounts outside its own project, so each
//! operator registers their own client. Either provider accepts an override.
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

    /// Environment variable that overrides the OAuth client ID resolved from
    /// the other two sources.
    ///
    /// Suits a shell session, a CI job, or a headless host. The interactive
    /// path is Settings → Integrations, which writes
    /// `~/.apollia/oauth-clients.toml` and survives a restart; an exported
    /// variable is only visible to processes launched from that same shell,
    /// which is a common way to think the client is configured when it is not.
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

    /// Build-time OAuth client ID compiled into the binary.
    ///
    /// The two providers deliberately differ.
    ///
    /// **Microsoft returns a real client id in every build.** Apollia
    /// registers one public client application against the Microsoft identity
    /// platform and ships its identifier as
    /// [`MICROSOFT_DEFAULT_CLIENT_ID`], so Microsoft 365 connects with no
    /// console detour. A native application's client id is public by
    /// construction (RFC 8252 section 8.5: anything in a distributed binary is
    /// extractable), which is why the registration carries no secret and PKCE
    /// carries the security instead.
    ///
    /// **Google returns an empty string in every build.** Google requires a
    /// verified OAuth consent screen before an application may serve accounts
    /// it does not own, so each operator brings their own client. The connect
    /// path refuses the handshake by name until they do.
    ///
    /// Both providers honour `APOLLIA_BUILD_*_CLIENT_ID` at compile time, the
    /// seam for anyone rebuilding Apollia from source against their own
    /// registered application. For Microsoft that overrides the shipped
    /// default; for Google it fills an otherwise empty slot.
    pub const fn default_client_id(self) -> &'static str {
        match self {
            Self::Google => env_or_empty(option_env!("APOLLIA_BUILD_GOOGLE_CLIENT_ID")),
            Self::Microsoft => env_or(
                option_env!("APOLLIA_BUILD_MICROSOFT_CLIENT_ID"),
                MICROSOFT_DEFAULT_CLIENT_ID,
            ),
        }
    }

    /// Build-time OAuth client secret. Empty in every Apollia build for both
    /// providers, present only for a source rebuild that sets
    /// `APOLLIA_BUILD_GOOGLE_CLIENT_SECRET`.
    ///
    /// Hardcoded empty for Microsoft, whose public clients carry no secret at
    /// all, and no build-time hook exists to change that: shipping a secret
    /// next to a shipped client id would be the one thing that turns a public
    /// registration into a leaked confidential one. Google needs a secret even
    /// under PKCE because its Installed App type requires it at the token
    /// endpoint, which is a further reason its client cannot be embedded.
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

    /// Build-time Google API key, which Picker requires alongside the OAuth
    /// token. Empty in every Apollia build, same posture as
    /// [`default_client_id`](Self::default_client_id). Microsoft returns empty
    /// because no Microsoft surface uses an API key yet.
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
    ///    `APOLLIA_MICROSOFT_CLIENT_ID`), for a shell session or CI.
    /// 2. `~/.apollia/oauth-clients.toml`, written by the Settings →
    ///    Integrations panel. **This is the path an operator takes**, and the
    ///    only one that survives a restart of the application.
    /// 3. Build-time constant, see
    ///    [`default_client_id`](Self::default_client_id). Microsoft resolves
    ///    here on a fresh install; Google does not, and stays empty until the
    ///    operator supplies a client through one of the first two layers.
    ///
    /// Returns `None` when all three are absent, so the UI can name what is
    /// missing instead of failing mid-handshake. On a fresh install that is
    /// Google's state, not Microsoft's.
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

/// Client id of the public client application Apollia registers against the
/// Microsoft identity platform, shipped in every build.
///
/// Not a secret. RFC 8252 section 8.5 states that a native application cannot
/// keep a credential confidential, so the registration is a public client with
/// no secret at all and PKCE carries the security. Publishing this constant
/// costs nothing an attacker could not recover with `strings` on the binary,
/// and it spares every operator a trip through the Azure portal.
///
/// The registration is multi-tenant and accepts personal Microsoft accounts,
/// which is why [`build_microsoft_provider`] must keep the `common` authority.
/// A tenant-scoped authority would restrict sign-in to the directory that owns
/// the registration and lock every other user out.
pub const MICROSOFT_DEFAULT_CLIENT_ID: &str = "c4f95bc5-8895-4550-8119-ed0e548fd941";

const fn env_or_empty(opt: Option<&'static str>) -> &'static str {
    env_or(opt, "")
}

/// Resolve a compile-time `option_env!` to `fallback` when absent or empty.
///
/// An empty `APOLLIA_BUILD_*` has to behave like an unset one: a build recipe
/// that exports the variable without a value would otherwise ship a blank
/// client id that reads as "configured" to every downstream check.
const fn env_or(opt: Option<&'static str>, fallback: &'static str) -> &'static str {
    match opt {
        Some(s) if !s.is_empty() => s,
        _ => fallback,
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
        // GIVEN a request for the MailSend scope alone
        // WHEN the Google provider config is built
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
        // GIVEN a request for the MailRead scope alone
        // WHEN the Microsoft provider config is built
        let cfg = build_microsoft_provider(&[MicrosoftScope::MailRead]);
        // THEN the resulting scope set includes User.Read + offline_access
        assert!(cfg.scopes.contains(&"User.Read"));
        assert!(cfg.scopes.contains(&"offline_access"));
        assert!(cfg.scopes.contains(&"Mail.Read"));
    }

    #[test]
    fn test_build_microsoft_provider_default_is_common_tenant() {
        // GIVEN a request that names no tenant
        // WHEN the Microsoft provider config is built
        let cfg = build_microsoft_provider(&[MicrosoftScope::MailRead]);
        // THEN both endpoints target the `common` tenant
        assert!(cfg.auth_url.contains("/common/"));
        assert!(cfg.token_url.contains("/common/"));
    }

    #[test]
    fn test_build_microsoft_provider_for_specific_tenant() {
        // GIVEN a tenant identifier
        let tenant = "00000000-0000-0000-0000-000000000000";
        // WHEN the provider config is built for that tenant
        let cfg = build_microsoft_provider_for_tenant(&[MicrosoftScope::MailRead], tenant);
        // THEN both endpoints carry it instead of `common`
        assert!(cfg.auth_url.contains(tenant));
        assert!(cfg.token_url.contains(tenant));
    }

    #[test]
    fn test_no_duplicate_scopes_when_profile_explicitly_requested() {
        // GIVEN a request naming Email explicitly, on top of MailSend
        // WHEN the Google provider config is built
        let cfg = build_google_provider(&[GoogleScope::MailSend, GoogleScope::Email]);
        // THEN "email" appears once, not twice
        let email_count = cfg.scopes.iter().filter(|s| **s == "email").count();
        assert_eq!(email_count, 1);
    }

    #[test]
    fn test_connector_provider_ids_are_stable() {
        // GIVEN the two connector providers the runtime knows
        // WHEN each is asked for its identifier
        // THEN the strings are the ones the token store persists
        assert_eq!(ConnectorProvider::Google.id(), "google");
        assert_eq!(ConnectorProvider::Microsoft.id(), "microsoft");
    }

    #[test]
    fn test_microsoft_ships_a_usable_client_id() {
        // GIVEN a build with no APOLLIA_BUILD_MICROSOFT_CLIENT_ID
        // WHEN the compiled default is read
        let compiled = ConnectorProvider::Microsoft.default_client_id();
        // THEN it carries the shipped registration, not an empty string, which
        // is what lets Microsoft 365 connect without a console detour
        assert_eq!(compiled, MICROSOFT_DEFAULT_CLIENT_ID);
        assert!(!compiled.is_empty());
    }

    #[test]
    fn test_microsoft_default_client_id_is_guid_shaped() {
        // GIVEN the shipped Microsoft client id
        // WHEN checked against the shape the identity platform issues
        let id = MICROSOFT_DEFAULT_CLIENT_ID;
        // THEN it is a 36-character GUID, the same shape `is_guid_like` in the
        // desktop credential check accepts, so a shipped build cannot fail the
        // very validation the Settings panel runs on operator input
        assert_eq!(id.len(), 36);
        let groups: Vec<&str> = id.split('-').collect();
        assert_eq!(
            groups.iter().map(|g| g.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12]
        );
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    }

    #[test]
    fn test_google_ships_no_client_id() {
        // GIVEN a build with no APOLLIA_BUILD_GOOGLE_CLIENT_ID
        // WHEN the compiled default is read
        // THEN it stays empty: Google needs a verified consent screen, so each
        // operator brings their own client and the connect path says so
        assert!(ConnectorProvider::Google.default_client_id().is_empty());
    }

    #[test]
    fn test_microsoft_ships_no_client_secret() {
        // GIVEN the shipped Microsoft registration
        // WHEN its compiled secret is read
        // THEN it is empty. A public client has no secret to keep, and
        // shipping one next to a shipped client id is the single change that
        // would turn a public registration into a leaked confidential one
        assert!(ConnectorProvider::Microsoft
            .default_client_secret()
            .is_empty());
    }

    #[test]
    fn test_shipped_microsoft_client_is_paired_with_a_multi_tenant_authority() {
        // GIVEN the provider built from the shipped registration
        let cfg = build_microsoft_provider(&[MicrosoftScope::MailRead]);
        // WHEN its authority is inspected
        // THEN it stays on `common`. The registration accepts personal
        // Microsoft accounts and any directory; a tenant-scoped authority
        // would lock sign-in to the directory that owns the registration and
        // shut every other user out, which is the failure a shipped client id
        // makes reachable and an empty one did not
        assert!(cfg.auth_url.contains("/common/"));
        assert!(cfg.token_url.contains("/common/"));
        assert!(!cfg.auth_url.contains(MICROSOFT_DEFAULT_CLIENT_ID));
        assert!(!cfg.client_id.is_empty());
    }

    #[test]
    fn test_env_or_treats_an_empty_build_variable_as_absent() {
        // GIVEN a build recipe that exports APOLLIA_BUILD_* with no value
        // WHEN the compile-time hook is resolved
        // THEN the fallback wins. An empty override would otherwise ship a
        // blank client id that every downstream check reads as configured
        assert_eq!(env_or(Some(""), "fallback"), "fallback");
        assert_eq!(env_or(None, "fallback"), "fallback");
        assert_eq!(env_or(Some("set"), "fallback"), "set");
    }
}
