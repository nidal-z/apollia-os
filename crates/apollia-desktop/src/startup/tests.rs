use super::*;

/// Holds a listener on a loopback port and answers every connection with
/// `answer`, the way a process already serving the port would.
fn serve_once(answer: &'static str) -> SocketAddr {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(answer.as_bytes());
        }
    });
    addr
}

/// The answer the runtime gives, in a given version.
fn apollia_answer(version: &str) -> &'static str {
    let body = format!(
        r#"{{"status":"ok","product":"{}","version":"{version}"}}"#,
        apollia_runtime::api::PRODUCT_MARKER
    );
    Box::leak(
        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_boxed_str(),
    )
}

#[test]
fn test_nothing_listens_so_the_desktop_launches_and_holds_the_port() {
    // GIVEN a loopback address nothing listens on
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));

    // WHEN the decision is taken
    let decision = decide(addr, None);

    // THEN it is to launch, and the port is already held by this process: a
    // second bind of it fails, so no probe-then-bind window exists
    let StartupDecision::Launch(listener) = decision else {
        panic!("expected Launch, got {decision:?}");
    };
    let held = listener.local_addr().unwrap();
    let rebind = TcpListener::bind(held);
    assert_eq!(
        rebind.map_err(|e| e.kind()).err(),
        Some(std::io::ErrorKind::AddrInUse)
    );
}

#[test]
fn test_a_compatible_apollia_runtime_listens_so_the_desktop_attaches() {
    // GIVEN an Apollia runtime of the same version serving the port
    let addr = serve_once(apollia_answer(env!("CARGO_PKG_VERSION")));

    // WHEN the decision is taken
    let decision = decide(addr, Some("token"));

    // THEN it is to attach, naming the version that serves
    match decision {
        StartupDecision::Attach { version } => assert_eq!(version, env!("CARGO_PKG_VERSION")),
        other => panic!("expected Attach, got {other:?}"),
    }
}

#[test]
fn test_a_foreign_listener_so_the_desktop_refuses() {
    // GIVEN a listener that answers HTTP but is not Apollia
    let addr = serve_once("HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok");

    // WHEN the decision is taken
    let decision = decide(addr, Some("token"));

    // THEN it is a refusal that names a foreign listener on that port
    match decision {
        StartupDecision::Refuse(RefusalReason::ForeignListener { port }) => {
            assert_eq!(port, addr.port());
        }
        other => panic!("expected Refuse(ForeignListener), got {other:?}"),
    }
}

#[test]
fn test_a_silent_listener_is_refused_as_foreign() {
    // GIVEN a listener that accepts and closes without a word
    let addr = serve_once("");

    // WHEN the decision is taken
    let decision = decide(addr, None);

    // THEN it is refused as foreign rather than attached to
    assert!(
        matches!(
            decision,
            StartupDecision::Refuse(RefusalReason::ForeignListener { .. })
        ),
        "{decision:?}"
    );
}

#[test]
fn test_an_apollia_runtime_of_another_minor_version_is_refused() {
    // GIVEN an Apollia runtime whose version differs in its minor number
    let addr = serve_once(apollia_answer("9.99.0"));

    // WHEN the decision is taken
    let decision = decide(addr, None);

    // THEN it is refused as incompatible, with both versions named
    match decision {
        StartupDecision::Refuse(RefusalReason::IncompatibleRuntime { version, ours, .. }) => {
            assert_eq!(version, "9.99.0");
            assert_eq!(ours, env!("CARGO_PKG_VERSION"));
        }
        other => panic!("expected Refuse(IncompatibleRuntime), got {other:?}"),
    }
}

#[test]
fn test_a_listener_refusing_the_token_is_named_as_such() {
    // GIVEN a listener that answers 401
    let addr = serve_once("HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\n\r\n");

    // WHEN the decision is taken
    let decision = decide(addr, Some("stale"));

    // THEN the refusal says the token was refused, not that the holder is foreign
    assert!(
        matches!(
            decision,
            StartupDecision::Refuse(RefusalReason::TokenRefused { .. })
        ),
        "{decision:?}"
    );
}

#[test]
fn test_versions_compare_on_major_and_minor() {
    // GIVEN pairs of versions
    // WHEN they are compared
    // THEN only the major and minor numbers decide
    assert!(compatible("0.2.0-preview", "0.2.0-preview"));
    assert!(compatible("0.2.3", "0.2.0-preview"));
    assert!(!compatible("0.3.0", "0.2.0-preview"));
    assert!(!compatible("garbage", "0.2.0"));
}

#[test]
fn test_reading_the_token_never_creates_it() {
    // GIVEN an empty data directory
    let dir = tempfile::tempdir().unwrap();

    // WHEN the token is read
    let token = read_token(dir.path());

    // THEN nothing is found and nothing is written
    assert_eq!(token, None);
    assert!(!dir.path().join("api-token").exists());
}
