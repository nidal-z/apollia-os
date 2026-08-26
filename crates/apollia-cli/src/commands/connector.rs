//! `apollia-os connector` subcommands: manage native SaaS connectors.
//!
//! Operates directly on `apollia-auth::AuthManager` (multi-account keyring)
//! and `apollia-connectors::{GoogleConnector, MicrosoftConnector}` without
//! requiring the runtime to be running. This is the local-first counterpart
//! to the Desktop `integrations` Tauri commands.

use std::sync::Arc;

use clap::Subcommand;

use apollia_auth::{AuthManager, ConnectorProvider};
use apollia_connectors::{ConnectorRegistry, GoogleConnector, MicrosoftConnector};

use crate::exit_codes;

mod accounts;
mod drive;
mod secrets;

use accounts::{run_accounts, run_list, run_revoke, run_test};
use drive::run_drive;
use secrets::{run_api_key, run_client_id, run_client_secret};

/// Subcommands of `apollia-os connector`.
#[derive(Debug, Subcommand)]
pub enum ConnectorCommand {
    /// List all native SaaS connectors registered in this build.
    ///
    /// Output covers the connector id, display name, publisher, and the
    /// services it exposes (e.g. `gmail`, `gcal`, `gdrive`).
    List,

    /// List OAuth-connected accounts for one or all providers.
    Accounts {
        /// Filter by provider: `google` or `microsoft`. Omit to list both.
        #[arg(long)]
        provider: Option<String>,
    },

    /// Probe the connector for an account by calling the userinfo endpoint.
    ///
    /// Returns the live identity claim and the scopes the upstream Authorization
    /// Server reports as granted, the same shape used by `connector.check()`
    /// inside the runtime.
    Test {
        /// Provider id: `google` or `microsoft`.
        provider: String,
        /// Account identifier (email when supplied during OAuth login).
        account: String,
    },

    /// Revoke the stored token for `(provider, account)`.
    ///
    /// Only the local keyring entry is cleared, the upstream Authorization
    /// Server is not notified. Use the provider's web revocation page for a
    /// server-side revocation.
    Revoke {
        /// Provider id.
        provider: String,
        /// Account id to revoke.
        account: String,
        /// Skip the confirmation prompt (required for scripts).
        #[arg(long)]
        confirm: bool,
    },

    /// Manage OAuth client_id overrides in `~/.apollia/oauth-clients.toml`.
    ///
    /// Power-user / Expert Mode: lets a CLI operator plug in their own Google
    /// or Microsoft client_id without rebuilding the binary. Resolution chain
    /// per provider is `env var > oauth-clients.toml > compiled default`.
    #[command(name = "client-id", subcommand)]
    ClientId(ClientIdCommand),

    /// Manage OAuth client_secret overrides in `~/.apollia/oauth-clients.toml`.
    ///
    /// Required by Google (Installed App needs a secret) and a no-op for
    /// Microsoft (public client per spec). File is created on demand with
    /// `0o600` permissions on Unix.
    #[command(name = "client-secret", subcommand)]
    ClientSecret(ClientSecretCommand),

    /// Manage API key overrides (Google Picker) in `~/.apollia/oauth-clients.toml`.
    ///
    /// Google-only today. Microsoft slot is reserved for the OneDrive File
    /// Picker if added later.
    #[command(name = "api-key", subcommand)]
    ApiKey(ApiKeyCommand),

    /// Manage per-account Google Drive folder preferences.
    ///
    /// Operates on `~/.apollia/drive-prefs.toml` and is independent of the
    /// runtime. The `picked` sub-group lists folders captured via the Desktop
    /// Picker (the CLI cannot pick, no UI, but can review and remove them).
    Drive {
        /// Drive subcommand.
        #[command(subcommand)]
        command: DriveCommand,
    },
}

/// Subcommands of `apollia-os connector client-id`.
#[derive(Debug, Subcommand)]
pub enum ClientIdCommand {
    /// List every provider's effective client_id + source + override.
    List,

    /// Set the client_id override for `<provider>`.
    ///
    /// Pass an empty string (`""`) to clear the override.
    Set {
        /// Provider id: `google` or `microsoft`.
        provider: String,
        /// New client_id value. Empty string clears the override.
        client_id: String,
    },
}

/// Subcommands of `apollia-os connector client-secret`.
#[derive(Debug, Subcommand)]
pub enum ClientSecretCommand {
    /// Set the client_secret override for `<provider>`.
    ///
    /// Pass an empty string (`""`) to clear the override. The CLI does not
    /// echo the secret back, but it is written to `~/.apollia/oauth-clients.toml`.
    Set {
        /// Provider id: `google` or `microsoft`.
        provider: String,
        /// New client_secret value. Empty string clears the override.
        client_secret: String,
    },
}

/// Subcommands of `apollia-os connector api-key`.
#[derive(Debug, Subcommand)]
pub enum ApiKeyCommand {
    /// Set the API key override for `<provider>`.
    ///
    /// Pass an empty string (`""`) to clear the override.
    Set {
        /// Provider id: `google` or `microsoft`.
        provider: String,
        /// New API key value. Empty string clears the override.
        api_key: String,
    },
}

/// Subcommands of `apollia-os connector drive`.
#[derive(Debug, Subcommand)]
pub enum DriveCommand {
    /// Manage the per-account Drive root folder path.
    Folder {
        /// Folder subcommand.
        #[command(subcommand)]
        command: DriveFolderCommand,
    },
}

/// Subcommands of `apollia-os connector drive folder`.
#[derive(Debug, Subcommand)]
pub enum DriveFolderCommand {
    /// List the folder override + effective path for every Google account.
    List,

    /// Set the folder path override for `<account>`.
    Set {
        /// Account id (typically the Google email).
        account: String,
        /// New folder path (e.g. `Apollia/Workspace`).
        path: String,
    },

    /// Reset the folder override for `<account>` (falls back to the default).
    Reset {
        /// Account id.
        account: String,

        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },

    /// Manage the picked-folder list captured via the Desktop Drive Picker.
    Picked {
        /// Picked-folder subcommand.
        #[command(subcommand)]
        command: PickedFolderCommand,
    },
}

/// Subcommands of `apollia-os connector drive folder picked`.
#[derive(Debug, Subcommand)]
pub enum PickedFolderCommand {
    /// List the picked Drive folders persisted for `<account>`.
    List {
        /// Account id.
        account: String,
    },

    /// Remove a picked folder from the persisted list.
    Remove {
        /// Account id.
        account: String,
        /// Drive folder id (the same id surfaced by `picked list`).
        folder_id: String,

        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        confirm: bool,
    },
}

/// Entry point for `apollia-os connector <verb>`.
pub async fn run(cmd: &ConnectorCommand, json: bool) -> i32 {
    match cmd {
        ConnectorCommand::List => run_list(json).await,
        ConnectorCommand::Accounts { provider } => run_accounts(provider.as_deref(), json).await,
        ConnectorCommand::Test { provider, account } => run_test(provider, account, json).await,
        ConnectorCommand::Revoke {
            provider,
            account,
            confirm,
        } => run_revoke(provider, account, *confirm, json).await,
        ConnectorCommand::ClientId(sub) => run_client_id(sub, json),
        ConnectorCommand::ClientSecret(sub) => run_client_secret(sub, json),
        ConnectorCommand::ApiKey(sub) => run_api_key(sub, json),
        ConnectorCommand::Drive { command } => run_drive(command, json).await,
    }
}

/// Build a [`ConnectorRegistry`] populated with the bundled connectors.
async fn build_registry(auth: Arc<AuthManager>) -> Result<ConnectorRegistry, String> {
    let registry = ConnectorRegistry::new();
    let google = GoogleConnector::new(auth.clone())
        .map_err(|e| format!("failed to build google connector: {e}"))?;
    registry.register(google).await;
    let microsoft = MicrosoftConnector::new(auth)
        .map_err(|e| format!("failed to build microsoft connector: {e}"))?;
    registry.register(microsoft).await;
    Ok(registry)
}

/// Parse a CLI-provided provider id into a strongly-typed [`ConnectorProvider`].
fn parse_provider(id: &str) -> Result<ConnectorProvider, String> {
    match id {
        "google" => Ok(ConnectorProvider::Google),
        "microsoft" => Ok(ConnectorProvider::Microsoft),
        other => Err(format!(
            "unknown provider '{other}' (expected: google, microsoft)"
        )),
    }
}

fn open_auth_manager(json: bool) -> Option<Arc<AuthManager>> {
    match AuthManager::new() {
        Ok(m) => Some(Arc::new(m)),
        Err(e) => {
            emit_error(format!("failed to open auth storage: {e}"), json);
            None
        }
    }
}

fn emit_error(msg: String, json: bool) {
    let _ = crate::output::emit_error(json, exit_codes::GENERAL_ERROR, &msg);
}
#[cfg(test)]
mod tests {
    use super::accounts::revoke_and_report;
    use super::drive::run_drive_folder_set;
    use super::secrets::mask_secret;
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: ConnectorCommand,
    }

    #[test]
    fn parse_provider_known_ids() {
        // GIVEN the two provider identifiers the command accepts
        // WHEN each is parsed
        // THEN it maps to its provider
        assert_eq!(parse_provider("google").unwrap(), ConnectorProvider::Google);
        assert_eq!(
            parse_provider("microsoft").unwrap(),
            ConnectorProvider::Microsoft
        );
    }

    #[test]
    fn parse_provider_rejects_unknown() {
        // GIVEN a provider name Apollia has no connector for
        // WHEN it is parsed
        // THEN the command refuses it instead of reaching the network
        assert!(parse_provider("notion").is_err());
    }

    #[test]
    fn parses_list() {
        // GIVEN "connector list"
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "list"]);
        // THEN the list subcommand is selected
        assert!(matches!(cli.cmd, ConnectorCommand::List));
    }

    #[test]
    fn parses_accounts_no_filter() {
        // GIVEN "connector accounts", with no --provider
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "accounts"]);
        // THEN no provider filter is set, so every account is listed
        match cli.cmd {
            ConnectorCommand::Accounts { provider } => assert!(provider.is_none()),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_accounts_with_provider() {
        // GIVEN the same command line with --provider google
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "accounts", "--provider", "google"]);
        // THEN the filter carries the provider
        match cli.cmd {
            ConnectorCommand::Accounts { provider } => {
                assert_eq!(provider.as_deref(), Some("google"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_test() {
        // GIVEN "connector test google alice@example.com"
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "test", "google", "alice@example.com"]);
        // THEN both positional arguments land in the right order
        match cli.cmd {
            ConnectorCommand::Test { provider, account } => {
                assert_eq!(provider, "google");
                assert_eq!(account, "alice@example.com");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_revoke_with_confirm() {
        // GIVEN a revoke carrying --confirm
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "revoke", "google", "alice@example.com", "--confirm"]);
        // THEN the account, the provider and the confirmation are all captured
        match cli.cmd {
            ConnectorCommand::Revoke {
                provider,
                account,
                confirm,
            } => {
                assert_eq!(provider, "google");
                assert_eq!(account, "alice@example.com");
                assert!(confirm);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[tokio::test]
    async fn revoke_without_confirm_returns_error() {
        // GIVEN a revoke asked for without --confirm
        // WHEN it runs
        let code = run_revoke("google", "x@example.com", false, true).await;
        // THEN it stops on an error rather than dropping the credentials
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[tokio::test]
    async fn revoke_absent_account_exits_success() {
        // GIVEN an auth manager over an isolated index file and the mock
        // keyring (process-global builder; no other test in this binary
        // touches a keyring entry, so no ordering dependence), holding no
        // token for the account
        keyring::set_default_credential_builder(keyring::mock::default_credential_builder());
        let dir = tempfile::tempdir().unwrap();
        let storage = apollia_auth::multi_account::MultiAccountStorage::with_index_path(
            dir.path().join("idx.json"),
        );
        let auth = AuthManager::with_storage(storage);

        // WHEN revoking an account that was never connected
        let code = revoke_and_report(
            &auth,
            ConnectorProvider::Google,
            "absent@example.invalid",
            true,
        )
        .await;

        // THEN the documented idempotent contract decides the exit code
        // ("Returns Ok(()) even if the token was already gone",
        // MultiAccountStorage::delete): success, not an error
        assert_eq!(code, exit_codes::SUCCESS);
    }

    #[tokio::test]
    async fn test_with_unknown_provider_errors() {
        // GIVEN a connectivity test asked for a provider Apollia has no connector for
        // WHEN it runs
        let code = run_test("dropbox", "x@example.com", true).await;
        // THEN it stops on an error
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn parses_client_id_list() {
        // GIVEN "connector client-id list"
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "client-id", "list"]);
        // THEN the nested client-id list subcommand is selected
        assert!(matches!(
            cli.cmd,
            ConnectorCommand::ClientId(ClientIdCommand::List)
        ));
    }

    #[test]
    fn parses_client_id_set() {
        // GIVEN "connector client-id set google <id>"
        // WHEN clap parses the argument line
        let cli =
            TestCli::parse_from(["x", "client-id", "set", "google", "abc123.apps.example.com"]);
        // THEN the provider and the identifier land in the right order
        match cli.cmd {
            ConnectorCommand::ClientId(ClientIdCommand::Set {
                provider,
                client_id,
            }) => {
                assert_eq!(provider, "google");
                assert_eq!(client_id, "abc123.apps.example.com");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_client_secret_set() {
        // GIVEN "connector client-secret set google <secret>"
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "client-secret", "set", "google", "GOCSPX-xxx"]);
        // THEN the provider and the secret land in the right order
        match cli.cmd {
            ConnectorCommand::ClientSecret(ClientSecretCommand::Set {
                provider,
                client_secret,
            }) => {
                assert_eq!(provider, "google");
                assert_eq!(client_secret, "GOCSPX-xxx");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_api_key_set() {
        // GIVEN "connector api-key set google <key>"
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "api-key", "set", "google", "AIza..."]);
        // THEN the provider and the key land in the right order
        match cli.cmd {
            ConnectorCommand::ApiKey(ApiKeyCommand::Set { provider, api_key }) => {
                assert_eq!(provider, "google");
                assert_eq!(api_key, "AIza...");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_drive_folder_list() {
        // GIVEN "connector drive folder list"
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "drive", "folder", "list"]);
        // THEN the three nesting levels are walked down to the list subcommand
        assert!(matches!(
            cli.cmd,
            ConnectorCommand::Drive {
                command: DriveCommand::Folder {
                    command: DriveFolderCommand::List,
                },
            }
        ));
    }

    #[test]
    fn parses_drive_folder_set() {
        // GIVEN a drive folder set carrying an account and a folder path
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from([
            "x",
            "drive",
            "folder",
            "set",
            "alice@example.com",
            "Apollia/Workspace",
        ]);
        // THEN both land in the right order under the nested subcommand
        match cli.cmd {
            ConnectorCommand::Drive {
                command:
                    DriveCommand::Folder {
                        command: DriveFolderCommand::Set { account, path },
                    },
            } => {
                assert_eq!(account, "alice@example.com");
                assert_eq!(path, "Apollia/Workspace");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_drive_folder_reset() {
        // GIVEN a drive folder reset with no --confirm
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from(["x", "drive", "folder", "reset", "alice@example.com"]);
        // THEN the account is captured and the confirmation stays down by default
        match cli.cmd {
            ConnectorCommand::Drive {
                command:
                    DriveCommand::Folder {
                        command: DriveFolderCommand::Reset { account, confirm },
                    },
            } => {
                assert_eq!(account, "alice@example.com");
                assert!(!confirm, "the confirmation is opt-in, never the default");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_drive_folder_picked_list() {
        // GIVEN "connector drive folder picked list <account>"
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from([
            "x",
            "drive",
            "folder",
            "picked",
            "list",
            "alice@example.com",
        ]);
        // THEN the four nesting levels are walked down and the account is captured
        match cli.cmd {
            ConnectorCommand::Drive {
                command:
                    DriveCommand::Folder {
                        command:
                            DriveFolderCommand::Picked {
                                command: PickedFolderCommand::List { account },
                            },
                    },
            } => assert_eq!(account, "alice@example.com"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_drive_folder_picked_remove() {
        // GIVEN a picked folder removal carrying an account and a folder identifier
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from([
            "x",
            "drive",
            "folder",
            "picked",
            "remove",
            "alice@example.com",
            "1abcDEFghi",
        ]);
        // THEN both are captured and the confirmation stays down by default
        match cli.cmd {
            ConnectorCommand::Drive {
                command:
                    DriveCommand::Folder {
                        command:
                            DriveFolderCommand::Picked {
                                command:
                                    PickedFolderCommand::Remove {
                                        account,
                                        folder_id,
                                        confirm,
                                    },
                            },
                    },
            } => {
                assert_eq!(account, "alice@example.com");
                assert_eq!(folder_id, "1abcDEFghi");
                assert!(!confirm, "the confirmation is opt-in, never the default");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn drive_folder_set_rejects_empty_path() {
        // GIVEN a drive folder path made of spaces
        // WHEN the folder is set
        let code = run_drive_folder_set("alice@example.com", "   ", true);
        // THEN the command stops on an error rather than storing a blank path
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn mask_secret_short_input_is_fully_redacted() {
        // GIVEN an empty secret, then one too short to keep any edge
        // WHEN each is masked for display
        // THEN nothing of the secret shows
        assert_eq!(mask_secret(""), "<empty>");
        assert_eq!(mask_secret("short"), "********");
    }

    #[test]
    fn mask_secret_long_input_preserves_edges() {
        // GIVEN a secret long enough to keep its edges
        // WHEN it is masked for display
        let masked = mask_secret("AIzaSyAbcdef1234567890");
        // THEN only the first and last characters show, with an ellipsis between them
        assert!(masked.starts_with("AIza"));
        assert!(masked.ends_with("90"));
        assert!(masked.contains("..."));
    }
}
