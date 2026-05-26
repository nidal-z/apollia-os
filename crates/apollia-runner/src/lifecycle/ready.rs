//! Annonce du port retenu au parent (daemon) via stdout.
//!
//! Le runner bind sur `127.0.0.1:0` (port choisi par l'OS), puis écrit
//! `READY <port>\n` sur stdout. Le daemon parse cette ligne avec un timeout
//! de 10 secondes côté `RunnerSupervisor`.
//!
//! **Stdout est dédié à ce canal uniquement.** Tout log applicatif va sur
//! stderr (cf. [observability::logs]).

use std::io::{self, Write};

/// Émet `READY <port>\n` sur le stdout standard puis flush.
///
/// Retourne `Ok(())` si l'écriture et le flush ont réussi. En cas d'échec,
/// le daemon hit son timeout `RUNNER_HANDSHAKE_TIMEOUT` et tue le runner.
pub fn announce(port: u16) -> io::Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    announce_to(&mut handle, port)
}

/// Version testable : écrit dans un writer arbitraire.
///
/// Utilisée par les tests unitaires avec un `Vec<u8>` mock.
pub fn announce_to<W: Write>(writer: &mut W, port: u16) -> io::Result<()> {
    writeln!(writer, "READY {port}")?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announce_writes_correct_format() {
        let mut buf = Vec::new();
        announce_to(&mut buf, 38492).unwrap();
        assert_eq!(buf, b"READY 38492\n");
    }

    #[test]
    fn announce_handles_max_port() {
        let mut buf = Vec::new();
        announce_to(&mut buf, u16::MAX).unwrap();
        assert_eq!(buf, b"READY 65535\n");
    }
}
