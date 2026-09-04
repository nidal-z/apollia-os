//! Replay harness for connector operations.
//!
//! # What this measures
//!
//! The 51 connector operations were, before this harness, exercised nowhere.
//! The tests around them build a request and assert its shape; not one of them
//! ever handed the client an answer and checked what it read out. This module
//! closes that half: it serves a recorded upstream answer from a local mock
//! server, calls the real client method through
//! [`dispatch`](dispatch::dispatch), and compares what the method returned
//! against what the fixture says it must return.
//!
//! So the measured span is **upstream bytes to typed value**: status handling,
//! JSON decoding, the `serde` attributes on every response type, and the
//! flattening each client does on top (`flatten_body` in Docs, the `items`
//! unwrapping in Tasks and YouTube, the byte reads in Drive and OneDrive).
//!
//! # What this does not measure
//!
//! - **The bridge.** `apollia-runtime`'s `connectors_bridge` resolves the
//!   account, fetches the token, marshals the agent's JSON arguments, and
//!   reshapes the client's return value into the payload the agent finally
//!   sees (`gmail.send` returns `{"sent": true, "message_id", "thread_id"}`,
//!   not a `GmailMessageRef`). None of that lives in this crate, so none of it
//!   is replayed here.
//! - **The request.** The mock answers every path, so a fixture passing does
//!   not say the client asked the right question. The request side is what the
//!   pre-existing tests in each client module already assert.
//! - **The real API.** A fixture is only as true as its recording. A
//!   hand-written example proves the harness runs, and nothing about the shape
//!   Google or Microsoft actually returns; [`fixture::Origin`] keeps the two
//!   apart and `scripts/check_connector_fixtures.py` counts them separately.
//! - **Authentication, retries and backoff.** Tokens never leave the fixture,
//!   and a fixture may not declare a 429 or a 5xx: those drive the
//!   sleep-and-retry policy already covered by the tests in `http.rs`.
//!
//! # Adding a fixture
//!
//! See `crates/apollia-connectors/fixtures/README.md`.

mod dispatch;
mod fixture;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde_json::Value;
use wiremock::{matchers::any, Mock, MockServer, Request, Respond, ResponseTemplate};

use crate::error::ConnectorError;
use fixture::{Fixture, Origin};

/// Serves a fixture's bodies in the order the client asks for them.
///
/// A client that asks for more answers than the fixture holds gets a marked
/// status and raises [`Self::overrun`], so "the operation made more calls than
/// the fixture describes" is reported as that, rather than surfacing as a
/// confusing decode failure.
struct Sequence {
    status: u16,
    bodies: Mutex<VecDeque<Value>>,
    overrun: AtomicBool,
}

impl Sequence {
    fn next_template(&self) -> ResponseTemplate {
        let next = self
            .bodies
            .lock()
            .expect("fixture queue poisoned")
            .pop_front();
        match next {
            Some(Value::String(raw)) => ResponseTemplate::new(self.status).set_body_string(raw),
            Some(body) => ResponseTemplate::new(self.status).set_body_json(body),
            None => {
                self.overrun.store(true, Ordering::SeqCst);
                ResponseTemplate::new(418).set_body_string("fixture exhausted")
            }
        }
    }
}

/// What one replay produced.
struct Replayed {
    outcome: Result<Value, ConnectorError>,
    calls: usize,
    overrun: bool,
}

/// Serve `fixture`'s bodies and run its operation against them.
async fn replay(fixture: &Fixture) -> Replayed {
    let server = MockServer::start().await;
    let bodies = fixture.bodies();
    let responder = std::sync::Arc::new(Sequence {
        status: fixture.status,
        bodies: Mutex::new(bodies.into()),
        overrun: AtomicBool::new(false),
    });
    Mock::given(any())
        .respond_with(SharedResponder(responder.clone()))
        .mount(&server)
        .await;

    let outcome = dispatch::dispatch(&fixture.operation, &server.uri()).await;
    let calls = server
        .received_requests()
        .await
        .map(|r| r.len())
        .unwrap_or(0);
    Replayed {
        outcome,
        calls,
        overrun: responder.overrun.load(Ordering::SeqCst),
    }
}

/// `Respond` needs ownership; the harness needs to read the overrun flag after
/// the server is done. Sharing the responder gives both.
struct SharedResponder(std::sync::Arc<Sequence>);

impl Respond for SharedResponder {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        self.0.next_template()
    }
}

/// Judge one fixture, returning one line per defect.
fn judge(fixture: &Fixture, played: &Replayed) -> Vec<String> {
    let mut defects = Vec::new();
    let expected_calls = fixture.bodies().len();

    if played.overrun {
        defects.push(format!(
            "the operation made more upstream calls than the fixture describes \
             ({} served, {} made). Record the extra answers under `responses`, in \
             call order",
            expected_calls, played.calls
        ));
        return defects;
    }
    if played.calls != expected_calls {
        defects.push(format!(
            "the fixture describes {expected_calls} upstream call(s) and the \
             operation made {}. A fixture must describe every call the operation \
             makes, or it is asserting against an answer the client never asked for",
            played.calls
        ));
    }

    match (&fixture.expect, &fixture.expect_error) {
        (Some(expected), None) => match &played.outcome {
            Ok(actual) => {
                if actual != expected {
                    defects.push(format!(
                        "read\n      {}\n    but the fixture expects\n      {}",
                        serde_json::to_string(actual).unwrap_or_default(),
                        serde_json::to_string(expected).unwrap_or_default()
                    ));
                }
            }
            Err(e) => defects.push(format!(
                "expected a reading, but the operation failed with: {e}"
            )),
        },
        (None, Some(needle)) => match &played.outcome {
            Ok(actual) => defects.push(format!(
                "expected a failure containing {needle:?}, but the operation read \
                 {} instead",
                serde_json::to_string(actual).unwrap_or_default()
            )),
            Err(e) => {
                let shown = e.to_string();
                if !shown.contains(needle) {
                    defects.push(format!(
                        "failed with {shown:?}, which does not contain the expected \
                         {needle:?}"
                    ));
                }
            }
        },
        _ => defects.push("asserts neither a reading nor a failure".into()),
    }
    defects
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Every operation the two connectors declare.
    fn declared() -> BTreeSet<&'static str> {
        crate::google::operations()
            .iter()
            .chain(crate::microsoft::operations().iter())
            .map(|op| op.id)
            .collect()
    }

    #[test]
    fn test_dispatch_table_matches_the_operation_catalogue() {
        // GIVEN the operations the connectors declare, and the operations the
        // replay harness can drive plus the ones that reach no HTTP endpoint
        let declared = declared();
        let covered: BTreeSet<&str> = dispatch::DISPATCHABLE
            .iter()
            .chain(dispatch::NO_UPSTREAM_CALL.iter())
            .copied()
            .collect();

        // WHEN the two sides are crossed
        let missing: Vec<&&str> = declared.difference(&covered).collect();
        let orphan: Vec<&&str> = covered.difference(&declared).collect();

        // THEN every declared operation is accounted for, and the harness names
        // no operation the connectors do not declare
        assert!(
            missing.is_empty(),
            "operations declared with no replay arm: {missing:?}. Add an arm in \
             replay/dispatch.rs, or name it in NO_UPSTREAM_CALL if it reaches no API"
        );
        assert!(
            orphan.is_empty(),
            "the replay harness names operations no connector declares: {orphan:?}"
        );
    }

    #[test]
    fn test_dispatch_table_has_no_duplicate() {
        // GIVEN the dispatch table
        // WHEN its entries are deduplicated
        let unique: BTreeSet<&&str> = dispatch::DISPATCHABLE.iter().collect();
        // THEN none was listed twice, so the count it reports is the count it covers
        assert_eq!(unique.len(), dispatch::DISPATCHABLE.len());
    }

    #[tokio::test]
    async fn test_unknown_operation_is_refused_rather_than_silently_passing() {
        // GIVEN an operation id no arm handles
        // WHEN it is dispatched
        let outcome = dispatch::dispatch("nope.nothing", "http://127.0.0.1:1").await;
        // THEN the harness refuses it instead of reporting an empty success
        let err = outcome.unwrap_err().to_string();
        assert!(err.contains("no replay arm"), "got: {err}");
    }

    #[tokio::test]
    async fn test_every_fixture_replays_into_the_reading_it_declares() {
        // GIVEN every fixture on disk
        let fixtures = fixture::load_all();

        // A harness with nothing to replay measures nothing, and must never be
        // read as a pass.
        assert!(
            !fixtures.is_empty(),
            "no fixture found in {}. The replay harness measured nothing",
            fixture::fixtures_dir().display()
        );

        // WHEN each is served to its operation and the reading is compared
        let mut report: Vec<String> = Vec::new();
        let mut played = 0usize;
        for (path, parsed) in fixtures {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("<unnamed>")
                .to_owned();
            let fixture = match parsed {
                Ok(f) => f,
                Err(e) => {
                    report.push(format!("  {name}: {e}"));
                    continue;
                }
            };
            if let Err(e) = fixture.validate(&path) {
                report.push(format!("  {name}: {e}"));
                continue;
            }
            let outcome = replay(&fixture).await;
            for defect in judge(&fixture, &outcome) {
                report.push(format!("  {name}: {defect}"));
            }
            played += 1;
        }

        // THEN every fixture read exactly what it declares
        assert!(
            report.is_empty(),
            "{} fixture(s) replayed, {} defect(s):\n{}",
            played,
            report.len(),
            report.join("\n")
        );
        assert!(played > 0, "no fixture was replayable");
    }

    #[tokio::test]
    async fn test_a_fixture_that_lies_about_the_reading_is_caught() {
        // GIVEN a fixture whose `expect` does not match what Gmail's client reads
        let fixture: Fixture = serde_json::from_value(serde_json::json!({
            "operation": "gmail.send",
            "origin": "example",
            "note": "self-test of the harness, not a fixture on disk",
            "response": {"id": "msg-1", "threadId": "thr-1"},
            "expect": {"id": "WRONG", "threadId": "thr-1"}
        }))
        .expect("fixture");

        // WHEN it is replayed
        let played = replay(&fixture).await;
        let defects = judge(&fixture, &played);

        // THEN the harness reports the mismatch rather than passing
        assert_eq!(defects.len(), 1, "{defects:?}");
        assert!(
            defects[0].contains("but the fixture expects"),
            "{defects:?}"
        );
    }

    #[tokio::test]
    async fn test_a_fixture_that_describes_too_few_calls_is_caught() {
        // GIVEN a single-answer fixture for an operation that makes two calls
        // (Drive resolves the folder before it lists it)
        let fixture: Fixture = serde_json::from_value(serde_json::json!({
            "operation": "gdrive.workspace_list",
            "origin": "example",
            "note": "self-test of the harness, not a fixture on disk",
            "response": {"files": []},
            "expect": []
        }))
        .expect("fixture");

        // WHEN it is replayed
        let played = replay(&fixture).await;
        let defects = judge(&fixture, &played);

        // THEN the harness says the fixture is short rather than reporting a pass
        assert!(
            defects.iter().any(|d| d.contains("upstream call")),
            "{defects:?}"
        );
    }

    #[test]
    fn test_a_fixture_with_an_unknown_key_is_refused() {
        // GIVEN a fixture carrying a misspelled key
        let raw = serde_json::json!({
            "operation": "gmail.send",
            "origin": "example",
            "note": "self-test",
            "responze": {"id": "msg-1"},
            "expect": {}
        });
        // WHEN it is parsed
        let parsed = serde_json::from_value::<Fixture>(raw);
        // THEN it is refused, rather than silently measuring less than it claims
        assert!(parsed.is_err());
    }

    #[test]
    fn test_a_retryable_status_is_refused() {
        // GIVEN a fixture declaring a status that drives the backoff policy
        let fixture: Fixture = serde_json::from_value(serde_json::json!({
            "operation": "gmail.send",
            "origin": "example",
            "note": "self-test",
            "status": 503,
            "response": {},
            "expect": {}
        }))
        .expect("fixture");
        // WHEN it is validated
        let verdict = fixture.validate(std::path::Path::new("gmail.send.json"));
        // THEN it is refused, so the suite never sleeps through a retry ladder
        assert!(verdict.unwrap_err().contains("retry-and-backoff"));
    }

    #[test]
    fn test_every_fixture_declares_a_known_origin() {
        // GIVEN the fixtures currently on disk
        let fixtures = fixture::load_all();
        // WHEN each one's origin is read
        // THEN it parsed, so it names itself a capture or an example rather than
        // letting a hand-written body pass for a recording
        for (path, parsed) in &fixtures {
            let f = parsed
                .as_ref()
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(matches!(f.origin, Origin::Capture | Origin::Example));
        }
    }
}
