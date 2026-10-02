//! A fake IMAP server on Tokio, for the client tests: implicit TLS with the
//! test authority's certificate, one scripted mailbox, and the record of every
//! command it received.
//!
//! It answers the commands a reading client sends, and also `SELECT`, so a
//! client that sent one would succeed and be caught by the record rather than
//! by an error.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio_rustls::rustls::pki_types::pem::PemObject;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::{RootCertStore, ServerConfig};
use tokio_rustls::TlsAcceptor;

use super::tls_test_material::{TEST_CA_PEM, TEST_SERVER_CERT_PEM, TEST_SERVER_KEY_PEM};
use super::{ImapAccount, ImapClient};

/// Password the fake mailbox accepts.
pub(super) const GOOD_PASSWORD: &str = "abcd efgh ijkl mnop";

/// One message of the fake mailbox.
pub(super) struct FakeMessage {
    pub uid: u32,
    pub seen: bool,
    pub raw: &'static str,
}

/// Folders of the fake mailbox; `Archive` only holds other folders.
pub(super) const FOLDERS: &[(&str, bool)] = &[("INBOX", false), ("Archive", true)];

/// The messages of the fake inbox.
pub(super) fn inbox() -> Vec<FakeMessage> {
    vec![
        FakeMessage {
            uid: 3,
            seen: true,
            raw: "From: Bob <bob@example.org>\r\n\
                  To: Ana <ana@example.org>\r\n\
                  Date: Thu, 01 Oct 2026 09:00:00 +0000\r\n\
                  Subject: Lunch\r\n\
                  \r\n\
                  Noon?\r\n",
        },
        FakeMessage {
            uid: 7,
            seen: false,
            raw: "From: =?UTF-8?Q?Zo=C3=AB?= <zoe@example.org>\r\n\
                  To: ana@example.org\r\n\
                  Cc: Carl <carl@example.org>\r\n\
                  Date: Fri, 02 Oct 2026 08:30:00 +0200\r\n\
                  Subject: =?UTF-8?Q?Invoice_for_Zo=C3=AB?=\r\n\
                  MIME-Version: 1.0\r\n\
                  Content-Type: multipart/mixed; boundary=\"b1\"\r\n\
                  \r\n\
                  --b1\r\n\
                  Content-Type: text/plain; charset=utf-8\r\n\
                  \r\n\
                  The invoice is attached.\r\n\
                  --b1\r\n\
                  Content-Type: application/pdf\r\n\
                  Content-Disposition: attachment; filename=\"invoice.pdf\"\r\n\
                  Content-Transfer-Encoding: base64\r\n\
                  \r\n\
                  JVBERi0xLjQK\r\n\
                  --b1--\r\n",
        },
    ]
}

/// A running fake server.
pub(super) struct FakeImap {
    pub addr: SocketAddr,
    commands: Arc<Mutex<Vec<String>>>,
}

impl FakeImap {
    /// Start a fake server on a free loopback port.
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(server_config()));
        let commands = Arc::new(Mutex::new(Vec::new()));
        let record = commands.clone();
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                let record = record.clone();
                tokio::spawn(async move {
                    if let Ok(tls) = acceptor.accept(tcp).await {
                        serve(tls, record).await;
                    }
                });
            }
        });
        Self { addr, commands }
    }

    /// Every command received, tag removed, the password of `LOGIN` masked.
    pub fn commands(&self) -> Vec<String> {
        self.commands.lock().unwrap().clone()
    }

    /// The account that reaches this server.
    pub fn account(&self) -> ImapAccount {
        ImapAccount {
            email: "ana@example.org".to_owned(),
            host: "localhost".to_owned(),
            port: self.addr.port(),
            username: "ana@example.org".to_owned(),
        }
    }
}

/// A client that trusts the test authority only.
pub(super) fn test_client() -> ImapClient {
    client_trusting(TEST_CA_PEM)
}

/// A client that trusts the authority `ca_pem` only.
pub(super) fn client_trusting(ca_pem: &str) -> ImapClient {
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from_pem_slice(ca_pem.as_bytes()).unwrap())
        .unwrap();
    ImapClient::with_roots(roots).unwrap()
}

fn server_config() -> ServerConfig {
    let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
    let cert = CertificateDer::from_pem_slice(TEST_SERVER_CERT_PEM.as_bytes()).unwrap();
    let key = PrivateKeyDer::from_pem_slice(TEST_SERVER_KEY_PEM.as_bytes()).unwrap();
    ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .unwrap()
}

async fn serve<S>(stream: S, record: Arc<Mutex<Vec<String>>>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (read, mut write) = tokio::io::split(stream);
    let mut lines = BufReader::new(read).lines();
    if write.write_all(b"* OK fake IMAP ready\r\n").await.is_err() {
        return;
    }
    let inbox = inbox();
    while let Ok(Some(line)) = lines.next_line().await {
        let Some((tag, command)) = line.split_once(' ') else {
            continue;
        };
        let upper = command.to_ascii_uppercase();
        let shown = if upper.starts_with("LOGIN ") {
            "LOGIN".to_owned()
        } else {
            command.to_owned()
        };
        record.lock().unwrap().push(shown);
        let reply = answer(tag, command, &upper, &inbox);
        if write.write_all(reply.as_bytes()).await.is_err() {
            return;
        }
        let _ = write.flush().await;
        if upper == "LOGOUT" {
            return;
        }
    }
}

fn answer(tag: &str, command: &str, upper: &str, inbox: &[FakeMessage]) -> String {
    if upper == "CAPABILITY" {
        return format!("* CAPABILITY IMAP4rev1\r\n{tag} OK done\r\n");
    }
    if upper.starts_with("LOGIN ") {
        let quoted: Vec<&str> = command.split('"').skip(1).step_by(2).collect();
        return if quoted.get(1) == Some(&GOOD_PASSWORD) {
            format!("{tag} OK logged in\r\n")
        } else {
            format!("{tag} NO [AUTHENTICATIONFAILED] invalid credentials\r\n")
        };
    }
    if let Some(folder) = upper
        .strip_prefix("EXAMINE ")
        .or_else(|| upper.strip_prefix("SELECT "))
    {
        let folder = folder.trim_matches('"');
        return if folder == "INBOX" {
            format!(
                "* {} EXISTS\r\n* 0 RECENT\r\n* FLAGS (\\Seen)\r\n{tag} OK [READ-ONLY] opened\r\n",
                inbox.len()
            )
        } else {
            format!("{tag} NO no such folder\r\n")
        };
    }
    if upper.starts_with("LIST ") {
        let mut out = String::new();
        for (name, no_select) in FOLDERS {
            let attrs = if *no_select {
                "\\Noselect"
            } else {
                "\\HasNoChildren"
            };
            out.push_str(&format!("* LIST ({attrs}) \"/\" \"{name}\"\r\n"));
        }
        return out + &format!("{tag} OK listed\r\n");
    }
    if let Some(criteria) = upper.strip_prefix("UID SEARCH ") {
        let uids: Vec<String> = inbox
            .iter()
            .filter(|m| !criteria.contains("UNSEEN") || !m.seen)
            .map(|m| m.uid.to_string())
            .collect();
        return format!("* SEARCH {}\r\n{tag} OK searched\r\n", uids.join(" "));
    }
    if let Some(rest) = upper.strip_prefix("UID FETCH ") {
        let (set, items) = rest.split_once(' ').unwrap_or((rest, ""));
        let wanted: Vec<u32> = set.split(',').filter_map(|u| u.parse().ok()).collect();
        let mut out = String::new();
        for (seq, message) in inbox.iter().enumerate() {
            if !wanted.contains(&message.uid) {
                continue;
            }
            let flags = if message.seen { "\\Seen" } else { "" };
            let (section, bytes) = if items.contains("BODY.PEEK[HEADER]") {
                let end = message
                    .raw
                    .find("\r\n\r\n")
                    .map_or(message.raw.len(), |i| i + 4);
                ("BODY[HEADER]", &message.raw[..end])
            } else {
                ("BODY[]", message.raw)
            };
            out.push_str(&format!(
                "* {} FETCH (UID {} FLAGS ({flags}) {section} {{{}}}\r\n{bytes})\r\n",
                seq + 1,
                message.uid,
                bytes.len()
            ));
        }
        return out + &format!("{tag} OK fetched\r\n");
    }
    if upper == "LOGOUT" {
        return format!("* BYE\r\n{tag} OK bye\r\n");
    }
    format!("{tag} BAD unknown command\r\n")
}
