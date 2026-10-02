//! `apollia-os connector imap`: a mailbox reached over IMAP with an
//! application password.
//!
//! `test` logs in and opens the inbox read-only, and stores nothing; `add`
//! does the same and then stores the connection, the password in the secret
//! store; `list` reads the connections of this machine. The password is read
//! from standard input (`--password-stdin`) or from `APOLLIA_IMAP_PASSWORD`,
//! never from the command line, where every process of the session could read
//! it.
//!
//! Like the other `connector` verbs, this runs without the runtime.

use std::io::BufRead;

use clap::{Args, Subcommand};

use apollia_connectors::imap::{
    ImapAccount, ImapAccountInput, ImapAccountStore, ImapClient, ImapError,
};

use crate::exit_codes;
use crate::note;

/// Environment variable read for the password when `--password-stdin` is not
/// given.
pub const PASSWORD_ENV: &str = "APOLLIA_IMAP_PASSWORD";

/// Subcommands of `apollia-os connector imap`.
#[derive(Debug, Subcommand)]
pub enum ImapCommand {
    /// Log in to a mailbox and open its inbox read-only; store nothing.
    ///
    /// For a known provider (Gmail, iCloud, Yahoo, Orange, Free, La Poste)
    /// the address is enough; the server and port are filled in.
    Test(ImapArgs),

    /// Verify a mailbox like `test`, then connect it: the password goes to
    /// the secret store, the rest to `~/.apollia/imap-accounts.json`.
    Add(ImapArgs),

    /// List the mailboxes connected on this machine.
    List,
}

/// What describes a mailbox on the command line.
#[derive(Debug, Args)]
pub struct ImapArgs {
    /// Address of the mailbox.
    pub email: String,

    /// IMAP server; filled from the address for a known provider.
    #[arg(long)]
    pub host: Option<String>,

    /// IMAP port, implicit TLS; the provider's, else 993.
    #[arg(long)]
    pub port: Option<u16>,

    /// Identifier sent at login; the address when absent.
    #[arg(long)]
    pub username: Option<String>,

    /// Read the application password from the first line of standard input,
    /// instead of `APOLLIA_IMAP_PASSWORD`.
    #[arg(long)]
    pub password_stdin: bool,
}

/// Entry point for `apollia-os connector imap <verb>`.
pub async fn run_imap(cmd: &ImapCommand, json: bool) -> i32 {
    match cmd {
        ImapCommand::Test(args) => run_verify(args, false, json).await,
        ImapCommand::Add(args) => run_verify(args, true, json).await,
        ImapCommand::List => run_list(json),
    }
}

async fn run_verify(args: &ImapArgs, store_it: bool, json: bool) -> i32 {
    let account = match ImapAccount::from_input(&ImapAccountInput {
        email: args.email.clone(),
        host: args.host.clone(),
        port: args.port,
        username: args.username.clone(),
    }) {
        Ok(account) => account,
        Err(e) => return imap_error(&e, json),
    };
    let password = match read_password(args.password_stdin) {
        Ok(password) => password,
        Err(message) => {
            return crate::output::emit_error(
                json,
                exit_codes::GENERAL_ERROR,
                &format!("missing_password: {message}"),
            )
        }
    };
    let client = match ImapClient::with_system_roots() {
        Ok(client) => client,
        Err(e) => return imap_error(&e, json),
    };
    let result = if store_it {
        match open_store() {
            Ok(store) => store
                .add_verified(account, &password, &client)
                .await
                .map(|a| (a, true)),
            Err(e) => Err(e),
        }
    } else {
        client
            .check(&account, &password)
            .await
            .map(|()| (account, false))
    };
    match result {
        Ok((account, stored)) => render_verified(&account, stored, json),
        Err(e) => imap_error(&e, json),
    }
}

fn run_list(json: bool) -> i32 {
    let Some(data_dir) = apollia_core::paths::data_dir() else {
        return crate::output::emit_error(
            json,
            exit_codes::GENERAL_ERROR,
            "the home directory cannot be resolved",
        );
    };
    let accounts = match ImapAccountStore::accounts_in(&data_dir) {
        Ok(accounts) => accounts,
        Err(e) => return imap_error(&e, json),
    };
    if json {
        let rows: Vec<serde_json::Value> = accounts.iter().map(account_json).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).unwrap_or_default()
        );
    } else if accounts.is_empty() {
        println!("No mailbox connected.");
        println!("  -> apollia-os connector imap add <address> --password-stdin");
    } else {
        note!("  Connected mailboxes:");
        for a in &accounts {
            println!("  * {}  {}:{}  ({})", a.email, a.host, a.port, a.username);
        }
    }
    exit_codes::SUCCESS
}

fn open_store() -> Result<ImapAccountStore, ImapError> {
    let data_dir = apollia_core::paths::data_dir().ok_or_else(|| ImapError::Index {
        action: "read",
        detail: "the home directory cannot be resolved".to_owned(),
    })?;
    ImapAccountStore::open(&data_dir)
}

/// The password, from the first line of standard input or the environment.
fn read_password(from_stdin: bool) -> Result<String, String> {
    let password = if from_stdin {
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .map_err(|e| format!("standard input could not be read: {e}"))?;
        line.trim_end_matches(['\r', '\n']).to_owned()
    } else {
        std::env::var(PASSWORD_ENV).map_err(|_| {
            format!("give the application password with --password-stdin or {PASSWORD_ENV}")
        })?
    };
    if password.is_empty() {
        return Err("the password is empty".to_owned());
    }
    Ok(password)
}

fn account_json(account: &ImapAccount) -> serde_json::Value {
    serde_json::json!({
        "email": account.email,
        "host": account.host,
        "port": account.port,
        "username": account.username,
    })
}

fn render_verified(account: &ImapAccount, stored: bool, json: bool) -> i32 {
    if json {
        let mut body = account_json(account);
        body["ok"] = serde_json::Value::Bool(true);
        body["stored"] = serde_json::Value::Bool(stored);
        println!(
            "{}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
    } else if stored {
        println!(
            "Mailbox {} connected ({}:{}).",
            account.email, account.host, account.port
        );
    } else {
        println!(
            "Mailbox {} answers ({}:{}); nothing was stored.",
            account.email, account.host, account.port
        );
    }
    exit_codes::SUCCESS
}

/// Report a typed failure: its stable code leads the message.
fn imap_error(err: &ImapError, json: bool) -> i32 {
    crate::output::emit_error(
        json,
        exit_codes::GENERAL_ERROR,
        &format!("{}: {err}", err.code()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: ImapCommand,
    }

    #[test]
    fn parses_add_with_every_option() {
        // GIVEN an add naming a server, a port, an identifier and stdin
        // WHEN clap parses the argument line
        let cli = TestCli::parse_from([
            "x",
            "add",
            "ana@example.org",
            "--host",
            "mail.example.org",
            "--port",
            "1993",
            "--username",
            "ana",
            "--password-stdin",
        ]);
        // THEN every option lands, and no argument carries a password
        match cli.cmd {
            ImapCommand::Add(args) => {
                assert_eq!(args.email, "ana@example.org");
                assert_eq!(args.host.as_deref(), Some("mail.example.org"));
                assert_eq!(args.port, Some(1993));
                assert_eq!(args.username.as_deref(), Some("ana"));
                assert!(args.password_stdin);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_test_and_list() {
        // GIVEN "imap test <address>" and "imap list"
        // WHEN clap parses each argument line
        let test = TestCli::parse_from(["x", "test", "ana@gmail.com"]);
        let list = TestCli::parse_from(["x", "list"]);
        // THEN the test reads the password from the environment by default,
        // and the list takes no argument
        match test.cmd {
            ImapCommand::Test(args) => {
                assert_eq!(args.email, "ana@gmail.com");
                assert!(!args.password_stdin);
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert!(matches!(list.cmd, ImapCommand::List));
    }

    #[test]
    fn password_is_not_an_argument() {
        // GIVEN a command line trying to pass the password as an option
        // WHEN clap parses it
        let parsed = TestCli::try_parse_from(["x", "test", "ana@gmail.com", "--password", "p"]);
        // THEN it is refused: the password never travels on the command line
        assert!(parsed.is_err());
    }

    #[tokio::test]
    async fn unknown_domain_without_server_fails_before_any_connection() {
        // GIVEN an address on a domain no provider is listed for, no server
        let args = ImapArgs {
            email: "ana@example.test".into(),
            host: None,
            port: None,
            username: None,
            password_stdin: false,
        };
        // WHEN it is tested
        let code = run_verify(&args, false, true).await;
        // THEN the command stops on a general error
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }
}
