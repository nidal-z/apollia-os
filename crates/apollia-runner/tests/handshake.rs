#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integration test: spawn the binary, parse READY <port>, curl /handshake.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn spawn_runner_and_handshake() {
    // Locate the compiled binary.
    // GIVEN the runner binary, spawned with its stdout piped
    let bin = env!("CARGO_BIN_EXE_apollia-runner");
    let mut child = Command::new(bin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn runner");

    // Parse READY <port>\n on stdout.
    // WHEN the announced port is read, then handshake, health and shutdown are called
    let stdout = child.stdout.take().expect("stdout pipe");
    let mut reader = BufReader::new(stdout);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).expect("read READY line");

    let port: u16 = first_line
        .strip_prefix("READY ")
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or_else(|| panic!("malformed READY line: {first_line:?}"));

    // Curl /handshake.
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client");
    let resp = client
        .get(format!("http://127.0.0.1:{port}/handshake"))
        .send()
        .expect("handshake request");
    // THEN the handshake answers with the protocol version
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().expect("parse JSON");
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"]["protocol_version"], "1.0");

    // Curl /health.
    let resp = client
        .get(format!("http://127.0.0.1:{port}/health"))
        .send()
        .expect("health request");
    assert_eq!(resp.status(), 200);

    // POST /shutdown -> the binary must exit.
    let resp = client
        .post(format!("http://127.0.0.1:{port}/shutdown"))
        .send()
        .expect("shutdown request");
    assert_eq!(resp.status(), 200);

    // The binary must exit within about one second.
    // THEN the process exits on its own, rather than having to be killed
    let exit = child
        .wait_timeout_or_kill(Duration::from_secs(3))
        .expect("wait runner exit");
    assert!(exit.success(), "runner did not exit cleanly");
}

// Small helper trait to wait on a child with a timeout (std::process::Child
// has no wait_timeout on stable).
trait WaitTimeout {
    fn wait_timeout_or_kill(
        &mut self,
        timeout: Duration,
    ) -> Result<std::process::ExitStatus, std::io::Error>;
}

impl WaitTimeout for std::process::Child {
    fn wait_timeout_or_kill(
        &mut self,
        timeout: Duration,
    ) -> Result<std::process::ExitStatus, std::io::Error> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(status);
            }
            if std::time::Instant::now() > deadline {
                let _ = self.kill();
                return self.wait();
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
