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
    assert!(compatible("0.3.0", "0.3.0"));
    assert!(compatible("0.3.2", "0.3.0"));
    assert!(compatible("0.3.0-1", "0.3.0"));
    assert!(!compatible("0.2.3", "0.3.0"));
    assert!(!compatible("0.2.0-preview", "0.3.0"));
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

#[test]
fn test_the_port_comes_from_the_file_then_the_default() {
    // GIVEN a data folder no daemon recorded a port in
    let dir = tempfile::tempdir().unwrap();

    // WHEN the port is resolved with a file naming 8123, without a file, and
    // with a refused file
    let from_file = resolve_port(dir.path(), Some("[api]\nport = 8123\n"));
    let without_file = resolve_port(dir.path(), None);
    let refused_file = resolve_port(dir.path(), Some("[api]\nport = \"x\"\n"));

    // THEN the file's port, then the default for both others
    assert_eq!(from_file, 8123);
    assert_eq!(without_file, 7771);
    assert_eq!(refused_file, 7771);
}

#[test]
fn test_a_port_recorded_by_a_listening_daemon_wins_over_the_file() {
    // GIVEN a daemon serving a port it recorded, and a file naming another
    let dir = tempfile::tempdir().unwrap();
    let daemon = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let recorded = daemon.local_addr().unwrap().port();
    apollia_runtime::api::write_port_file(dir.path(), recorded).unwrap();

    // WHEN the port is resolved
    let port = resolve_port(dir.path(), Some("[api]\nport = 8123\n"));

    // THEN the daemon's port
    assert_eq!(port, recorded);
}

#[test]
fn test_a_port_recorded_by_a_daemon_that_is_gone_is_ignored() {
    // GIVEN a record of a port nothing listens on any more
    let dir = tempfile::tempdir().unwrap();
    let gone = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let recorded = gone.local_addr().unwrap().port();
    drop(gone);
    apollia_runtime::api::write_port_file(dir.path(), recorded).unwrap();

    // WHEN the port is resolved
    let port = resolve_port(dir.path(), Some("[api]\nport = 8123\n"));

    // THEN the file's port
    assert_eq!(port, 8123);
}

// ── The runtime installed as a service ──────────────────────────────────

#[test]
fn test_the_service_step_follows_the_observed_service_and_the_port() {
    // GIVEN the combinations of an installed service, its process, and an
    // Apollia runtime already serving the port
    // WHEN the step is chosen for each
    // THEN no service decides at once, a stopped one is started, a running
    // one is awaited, and a runtime already serving is attached to as is
    assert_eq!(
        service::service_step(false, false, false),
        service::ServiceStep::Decide
    );
    assert_eq!(
        service::service_step(true, false, false),
        service::ServiceStep::StartThenAwait
    );
    assert_eq!(
        service::service_step(true, true, false),
        service::ServiceStep::Await
    );
    assert_eq!(
        service::service_step(true, false, true),
        service::ServiceStep::Decide
    );
}

#[test]
fn test_waiting_for_the_service_ends_when_an_apollia_runtime_answers() {
    // GIVEN an Apollia runtime serving a loopback port, and a data directory
    // without a token
    let addr = serve_once(apollia_answer(env!("CARGO_PKG_VERSION")));
    let data = tempfile::tempdir().unwrap();

    // WHEN the application waits for it
    let served = service::await_serving(addr, data.path(), Duration::ZERO);

    // THEN it answers, and the decision that follows attaches
    assert!(served);
    assert!(matches!(decide(addr, None), StartupDecision::Attach { .. }));
}

#[test]
fn test_waiting_for_a_service_that_never_answers_gives_up_at_the_bound() {
    // GIVEN port 0, on which no runtime can ever answer
    let addr = SocketAddr::from(([127, 0, 0, 1], 0));
    let data = tempfile::tempdir().unwrap();

    // WHEN the application waits for a runtime there, with no time left
    let served = service::await_serving(addr, data.path(), Duration::ZERO);

    // THEN it gives up and leaves the decision to the startup probe
    assert!(!served);
}
