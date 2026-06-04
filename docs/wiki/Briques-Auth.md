# apollia-auth - OAuth2 PKCE et Keyring Multi-Plateforme

> *Authentification interactive auprès des providers LLM cloud via OAuth2 PKCE (RFC 7636) avec stockage sécurisé dans le keyring OS natif. Crate introduite (ADR-016).*

---

## 1. Rôle dans l'architecture

La crate `apollia-auth` centralise la gestion des tokens OAuth2 pour les providers LLM cloud. Elle remplace la gestion manuelle des API keys via variables d'environnement par un flow PKCE interactif avec serveur callback local.

**Responsabilités :**
- Générer les pairs `code_verifier` / `code_challenge` conformes RFC 7636
- Lancer un serveur HTTP local éphémère pour recevoir le callback OAuth2
- Échanger le code d'autorisation contre un token d'accès
- Persister les tokens dans le keyring OS (macOS Keychain / Linux Secret Service / Windows Credential Store)
- Exposer les sous-commandes CLI `apollia auth login/status/logout`

**Principe(s) architectural(aux) :**
- Principe #1 - Local-first : les tokens ne quittent jamais la machine
- Principe #4 - Fail fast : provider inconnu → `AuthError::UnknownProvider` immédiat

**Providers supportés :** `anthropic`, `openai`, `vertex`

---

## 2. Structure de la crate

```
crates/apollia-auth/src/
├── lib.rs          ← exports publics
├── pkce.rs         ← OAuth2PkceFlow, generate_code_verifier(), generate_code_challenge()
├── callback.rs     ← bind_ephemeral_port(), wait_for_callback()
├── token.rs        ← StoredToken, exchange_code(), refresh_token()
├── storage.rs      ← KeyringStorage (macOS Keychain / Linux Secret Service / Windows Credential Store)
├── providers.rs    ← ProviderConfig, get_provider(), SUPPORTED_PROVIDERS
└── error.rs        ← AuthError (thiserror)
```

---

## 3. Types publics

### `OAuth2PkceFlow`

```rust
/// Flow OAuth2 PKCE complet - RFC 7636.
#[derive(Debug, Clone)]
pub struct OAuth2PkceFlow {
    pub code_verifier: String,    // base64url, 43 chars, random
    pub code_challenge: String,   // SHA-256(code_verifier), base64url
    pub state: String,            // nonce CSRF, random
    pub redirect_uri: String,     // "http://localhost:{port}/callback"
}

impl OAuth2PkceFlow {
    /// Crée un nouveau flow avec un port de callback éphémère.
    pub fn new(redirect_port: u16) -> Self;
}

pub fn generate_code_verifier() -> String;
pub fn generate_code_challenge(verifier: &str) -> String;
```

**Conformité RFC 7636 :** le `code_challenge` est calculé par `BASE64URL(SHA-256(code_verifier))` sans padding `=`.

### `StoredToken`

```rust
/// Token OAuth2 persisté dans le keyring OS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub scopes: Vec<String>,
}

impl StoredToken {
    /// Retourne true si le token est expiré ou absent d'`expires_at`.
    pub fn is_expired(&self) -> bool;
}

pub async fn exchange_code(
    provider: &ProviderConfig,
    flow: &OAuth2PkceFlow,
    code: &str,
) -> Result<StoredToken, AuthError>;

pub async fn refresh_token(
    provider: &ProviderConfig,
    stored: &StoredToken,
) -> Result<StoredToken, AuthError>;
```

### `KeyringStorage`

```rust
/// Stockage des tokens dans le keyring OS natif.
pub struct KeyringStorage;

impl KeyringStorage {
    /// Stocke le token sous la clé `apollia-auth:{provider_name}`.
    pub fn store(provider_name: &str, token: &StoredToken) -> Result<(), AuthError>;

    /// Charge le token depuis le keyring. Retourne None si absent.
    pub fn load(provider_name: &str) -> Result<Option<StoredToken>, AuthError>;

    /// Supprime le token du keyring.
    pub fn delete(provider_name: &str) -> Result<(), AuthError>;
}
```

**Implémentation multi-plateforme :**

| Plateforme | Stockage |
|---|---|
| macOS | Keychain (via crate `keyring` v3) |
| Linux | Secret Service (libsecret / GNOME Keyring) |
| Windows | Windows Credential Store |

### `ProviderConfig`

```rust
/// Configuration OAuth2 d'un provider LLM cloud.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub name: &'static str,
    pub auth_url: &'static str,
    pub token_url: &'static str,
    pub client_id: String,
    pub scopes: Vec<&'static str>,
}

/// Retourne la config OAuth2 d'un provider par son nom.
/// Retourne None si le provider n'est pas supporté.
pub fn get_provider(name: &str) -> Option<ProviderConfig>;

pub const SUPPORTED_PROVIDERS: &[&str] = &["anthropic", "openai", "vertex"];
```

### Serveur callback

```rust
/// Ouvre un port TCP éphémère sur 127.0.0.1.
pub async fn bind_ephemeral_port() -> Result<(TcpListener, u16), AuthError>;

/// Attend le callback OAuth2 sur le listener fourni.
/// Vérifie le state pour la protection CSRF.
/// Retourne le code d'autorisation.
pub async fn wait_for_callback(
    listener: TcpListener,
    expected_state: &str,
) -> Result<String, AuthError>;
```

### `AuthError`

```rust
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("state mismatch - possible CSRF")]
    StateMismatch,
    #[error("code manquant dans le callback")]
    MissingCode,
    #[error("provider error: {0}")]
    ProviderError(String),
    #[error("token exchange failed: {0}")]
    TokenExchangeFailed(String),
    #[error("erreur HTTP: {0}")]
    HttpError(String),
    #[error("serveur callback: {0}")]
    CallbackServer(String),
    #[error("keyring: {0}")]
    Keyring(String),
    #[error("sérialisation: {0}")]
    Serialization(String),
    #[error("pas de refresh token disponible")]
    NoRefreshToken,
    #[error("provider inconnu: {0}")]
    UnknownProvider(String),
}
```

---

## 4. Flow complet - `apollia auth login`

```
1. bind_ephemeral_port()        → port = 54123
2. OAuth2PkceFlow::new(port)    → code_verifier, code_challenge, state, redirect_uri
3. build_auth_url(provider, flow) → URL avec code_challenge + state
4. open::that(url)              → ouvre le browser
5. wait_for_callback(listener, state) → reçoit code d'autorisation
6. exchange_code(provider, flow, code) → StoredToken
7. KeyringStorage::store(provider, token)
```

**Protection CSRF :** le `state` généré aléatoirement dans `OAuth2PkceFlow::new` est comparé au `state` retourné dans le callback. Toute divergence retourne `AuthError::StateMismatch`.

---

## 5. CLI - `apollia auth`

```bash
# Login interactif - ouvre le browser, attend le callback
$ apollia auth login anthropic
  → Ouverture du browser sur https://anthropic.com/oauth/authorize?...
  → En attente du callback sur http://localhost:54123/callback ...
  ✔ Token stocké dans le keyring (anthropic)

$ apollia auth login openai
$ apollia auth login vertex

# Statut de tous les providers
$ apollia auth status
  PROVIDERS
  ─────────────────────────────────────────────────────────
  PROVIDER    ÉTAT              EXPIRE
  anthropic   ✔ configuré       2026-05-04T10:32:00Z
  openai      ○ non configuré   -
  vertex      ✔ configuré       2026-04-20T08:00:00Z (expiré)

# Logout - supprime le token du keyring
$ apollia auth logout anthropic
  ✔ Token anthropic supprimé du keyring

$ apollia auth status --json
```

---

## 6. Ce que cette crate N'implémente PAS

- L'intégration des tokens dans le `LlmRouter` (sprint futur)
- Le refresh automatique en tâche de fond
- L'UI desktop pour le login OAuth2
- Les providers OAuth2 non-LLM (GitHub, Slack, etc.)

---

## 7. Décision architecturale

> **Voir aussi :** [ADR-016](../adr/ADR-016-secrets-keyring-api-auth.md) - OAuth2 PKCE : Keyring Multi-Plateforme vs Fichier Chiffré

| Décision | Raison |
|---|---|
| `keyring` v3 plutôt qu'un fichier chiffré | Délègue au keyring OS natif - zéro gestion de clé de chiffrement côté Apollia. Principe #1 : local-first sans complexité ajoutée |
| Serveur callback local (port éphémère) | Ne nécessite pas d'URI de redirection fixe - compatible avec tous les providers OAuth2 |
| Pas de stockage en clair dans `apollia.toml` | Les tokens sensibles ne doivent jamais être dans un fichier de configuration versionnable |
