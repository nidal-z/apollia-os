//! Apollia OS — sidecar runner pour l'inférence LLM et STT locale.
//!
//! Voir [ADR-113](../../docs/adr/ADR-113-multi-runner-sidecar-architecture.md)
//! pour le contexte et [IPC-PROTOCOL](../../docs/internal/architecture/IPC-PROTOCOL.md)
//! pour la spec du protocole.
//!
//! Ce crate produit un binaire `apollia-runner` (renommé en `apollia-runner-{backend}`
//! lors du packaging) qui :
//!
//! 1. Bind un serveur axum sur `127.0.0.1:0` (port choisi par l'OS).
//! 2. Annonce le port au parent via stdout (`READY <port>\n`).
//! 3. Sert les endpoints `/handshake`, `/health`, `/llm/*`, `/stt/*`, `/shutdown`.
//! 4. Charge `llama-cpp-2` et `whisper-rs` avec un seul backend GPU compilé.

pub mod ipc;
pub mod lifecycle;
pub mod observability;
pub mod server;

#[cfg(feature = "local-cpu")]
pub mod backends;
