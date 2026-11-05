//! `engine install`, `import` and `remove` against a fake daemon on a Unix
//! socket whose answers follow one another, as an installation does.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clap::Parser;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

use super::install::{self, choose_variant, step_of, Follower, Job, Step, Wanted};
use super::remove::{after_text, removal_route};
use super::{EngineCommand, InstallEngine};
use crate::client::RuntimeClient;
use crate::exit_codes;
use crate::{Cli, Commands};

const POLL: Duration = Duration::from_millis(1);

fn parse(args: &[&str]) -> Cli {
    Cli::parse_from(args)
}

fn engine_command(cli: Cli) -> EngineCommand {
    let Commands::Engine { command } = cli.command else {
        panic!("expected engine");
    };
    command
}

/// A body of `GET /api/v1/engines` rating CUDA 12 recommended and Metal
/// incompatible.
fn engines_body() -> Value {
    json!({
        "engines": [],
        "channel": "stable",
        "ratings": [
            {"variant": "cuda-12", "verdict": "recommended", "reason": null},
            {"variant": "vulkan", "verdict": "compatible", "reason": null},
            {"variant": "metal", "verdict": "incompatible", "reason": {"code": "not_built_for_platform"}}
        ]
    })
}

/// One route of the fake: its method, its path with the query, and the
/// answers it gives in turn, the last one repeated.
type Route = (&'static str, String, Vec<(u16, Value)>);

/// A route of the fake as it serves: the answers not given yet.
type Served = (&'static str, String, VecDeque<(u16, Value)>);

struct Fake {
    routes: Mutex<Vec<Served>>,
    received: Mutex<Vec<(String, String)>>,
}

impl Fake {
    fn lines(&self) -> Vec<String> {
        self.received
            .lock()
            .unwrap()
            .iter()
            .map(|(line, _)| line.clone())
            .collect()
    }

    fn body_of(&self, prefix: &str) -> Option<Value> {
        self.received
            .lock()
            .unwrap()
            .iter()
            .find(|(line, _)| line.starts_with(prefix))
            .map(|(_, body)| serde_json::from_str(body).unwrap_or(Value::Null))
    }

    fn count(&self, prefix: &str) -> usize {
        self.lines()
            .iter()
            .filter(|l| l.starts_with(prefix))
            .count()
    }
}

async fn read_request(stream: &mut tokio::net::UnixStream) -> (String, String) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk).await.unwrap();
        if n == 0 {
            return (String::new(), String::new());
        }
        buf.extend_from_slice(&chunk[..n]);
        let text = String::from_utf8_lossy(&buf).to_string();
        let Some(end) = text.find("\r\n\r\n") else {
            continue;
        };
        let length = text[..end]
            .lines()
            .find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse::<usize>().unwrap())
            })
            .unwrap_or(0);
        if buf.len() >= end + 4 + length {
            let first = text.lines().next().unwrap_or_default().to_string();
            let body = String::from_utf8_lossy(&buf[end + 4..end + 4 + length]).to_string();
            return (first, body);
        }
    }
}

fn serve(dir: &Path, routes: Vec<Route>) -> (PathBuf, Arc<Fake>) {
    let fake = Arc::new(Fake {
        routes: Mutex::new(
            routes
                .into_iter()
                .map(|(m, p, a)| (m, p, VecDeque::from(a)))
                .collect(),
        ),
        received: Mutex::new(Vec::new()),
    });
    let socket = dir.join("runtime.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let shared = fake.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let fake = shared.clone();
            tokio::spawn(async move {
                let (first, body) = read_request(&mut stream).await;
                fake.received.lock().unwrap().push((first.clone(), body));
                let (status, answer) = {
                    let mut routes = fake.routes.lock().unwrap();
                    routes
                        .iter_mut()
                        .find(|(method, path, _)| first.starts_with(&format!("{method} {path} ")))
                        .and_then(|(_, _, answers)| {
                            if answers.len() > 1 {
                                answers.pop_front()
                            } else {
                                answers.front().cloned()
                            }
                        })
                        .unwrap_or((404, json!({"error": "no route"})))
                };
                let text = if status == 204 {
                    String::new()
                } else {
                    answer.to_string()
                };
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
                    text.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
                stream.shutdown().await.ok();
            });
        }
    });
    (socket, fake)
}

fn route(method: &'static str, path: &str, answers: Vec<(u16, Value)>) -> Route {
    (method, path.to_owned(), answers)
}

fn wanted(engine: InstallEngine, variant: Option<&str>) -> Wanted<'_> {
    Wanted {
        engine,
        variant,
        detach: false,
        force: false,
    }
}

/// A JSON follower of `client` reading again every `poll`.
fn at(client: &RuntimeClient, poll: Duration) -> Follower<'_> {
    Follower {
        client,
        json: true,
        poll,
    }
}

/// An interruption that never comes.
fn never() -> std::future::Pending<()> {
    std::future::pending()
}

fn downloading(received: u64) -> Value {
    json!({"state": "downloading", "kind": "install", "received": received, "total": 100})
}

// ─── parsing ─────────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_engine_install_with_and_without_a_variant() {
    // GIVEN the install forms of llama-server and Node.js
    // WHEN the parser reads them
    let plain = engine_command(parse(&["apollia-os", "engine", "install", "llama-server"]));
    let named = engine_command(parse(&[
        "apollia-os",
        "engine",
        "install",
        "llama-server",
        "cuda-13",
        "--detach",
        "--force",
    ]));
    let node = engine_command(parse(&["apollia-os", "engine", "install", "nodejs"]));
    // THEN the engine, the variant and the flags are read
    assert!(matches!(
        plain,
        EngineCommand::Install {
            engine: InstallEngine::LlamaServer,
            variant: None,
            detach: false,
            force: false
        }
    ));
    assert!(matches!(
        named,
        EngineCommand::Install { engine: InstallEngine::LlamaServer, variant: Some(ref v), detach: true, force: true } if v == "cuda-13"
    ));
    assert!(matches!(
        node,
        EngineCommand::Install {
            engine: InstallEngine::Nodejs,
            variant: None,
            ..
        }
    ));
}

#[test]
fn test_cli_refuses_engine_install_of_an_unknown_engine_or_variant() {
    // GIVEN an engine and a variant the channel does not have
    // WHEN the parser reads them
    let engine = Cli::try_parse_from(["apollia-os", "engine", "install", "ollama"]);
    let variant =
        Cli::try_parse_from(["apollia-os", "engine", "install", "llama-server", "opencl"]);
    // THEN clap refuses both, which main maps to exit 1
    assert!(engine.is_err());
    assert!(variant.is_err());
}

#[test]
fn test_cli_parses_engine_import_and_remove() {
    // GIVEN "engine import <dir> --detach" and "engine remove <engine> <tag> <variant> --confirm"
    // WHEN the parser reads them
    let import = engine_command(parse(&[
        "apollia-os",
        "engine",
        "import",
        "/mnt/engines",
        "--detach",
    ]));
    let remove = engine_command(parse(&[
        "apollia-os",
        "engine",
        "remove",
        "llama-server",
        "b12064",
        "cpu",
        "--confirm",
    ]));
    let unconfirmed = engine_command(parse(&[
        "apollia-os",
        "engine",
        "remove",
        "nodejs",
        "v24.22.2",
        "cpu",
    ]));
    // THEN the directory, the build and the flags are read
    assert!(matches!(
        import,
        EngineCommand::Import { ref dir, detach: true } if dir == Path::new("/mnt/engines")
    ));
    assert!(matches!(
        remove,
        EngineCommand::Remove { ref engine, ref tag, ref variant, confirm: true }
            if engine == "llama-server" && tag == "b12064" && variant == "cpu"
    ));
    assert!(matches!(
        unconfirmed,
        EngineCommand::Remove { confirm: false, .. }
    ));
}

// ─── the choice of the variant and the steps ─────────────────────────────────

#[test]
fn test_choose_variant_takes_the_recommended_one_when_none_is_named() {
    // GIVEN a machine rating CUDA 12 recommended
    // WHEN no variant is named
    let chosen = choose_variant(&engines_body(), None, false);
    // THEN CUDA 12 is installed
    assert_eq!(chosen, Ok("cuda-12".to_owned()));
}

#[test]
fn test_choose_variant_says_so_when_nothing_is_recommended() {
    // GIVEN a machine whose cards are not rated yet
    // WHEN no variant is named
    let chosen = choose_variant(&json!({"ratings": []}), None, false);
    // THEN it is refused, pointing at the list
    assert!(chosen.unwrap_err().contains("engine list --all"));
}

#[test]
fn test_choose_variant_refuses_an_incompatible_variant_without_force() {
    // GIVEN Metal rated incompatible here
    // WHEN it is named without --force, then with it
    let refused = choose_variant(&engines_body(), Some("metal"), false);
    let forced = choose_variant(&engines_body(), Some("metal"), true);
    // THEN it is refused with its reason and --force, then accepted
    let message = refused.unwrap_err();
    assert!(message.contains("not built for this platform"), "{message}");
    assert!(message.contains("--force"), "{message}");
    assert_eq!(forced, Ok("metal".to_owned()));
}

#[test]
fn test_step_of_reads_the_states_of_both_routes() {
    // GIVEN the states of the installation route and of the Node.js route
    // WHEN each is read
    // THEN each becomes its step
    assert_eq!(
        step_of(
            Job::Download,
            &json!({"state": "verifying", "kind": "install"})
        ),
        Step::Working("verifying the signature")
    );
    assert_eq!(
        step_of(Job::Download, &downloading(5)),
        Step::Downloading {
            received: 5,
            total: 100
        }
    );
    assert_eq!(
        step_of(
            Job::Import,
            &json!({"state": "installing", "kind": "import"})
        ),
        Step::Working("installing")
    );
    assert_eq!(
        step_of(
            Job::Download,
            &json!({"state": "installed", "kind": "install"})
        ),
        Step::Done
    );
    assert_eq!(
        step_of(
            Job::Import,
            &json!({"state": "failed", "kind": "import", "code": "signature_unknown_key", "error": "signed by another key", "archive_requested": false})
        ),
        Step::Failed("signed by another key".to_owned())
    );
    assert_eq!(
        step_of(Job::Download, &json!({"state": "idle"})),
        Step::Gone
    );
    assert_eq!(
        step_of(
            Job::Node,
            &json!({"state": "ready", "tag": "v24.22.2", "path": "/s"})
        ),
        Step::Done
    );
    assert_eq!(
        step_of(
            Job::Node,
            &json!({"state": "failed", "error": "mirror down"})
        ),
        Step::Failed("mirror down".to_owned())
    );
}

#[test]
fn test_removal_route_encodes_each_segment_and_the_confirmation() {
    // GIVEN a build and a segment holding a slash
    // WHEN its route is built with and without the confirmation
    // THEN each segment stays one segment and confirm=true is the query
    assert_eq!(
        removal_route(["llama-server", "b12064", "cpu"], false),
        "/api/v1/engines/llama-server/b12064/cpu"
    );
    assert_eq!(
        removal_route(["llama-server", "b12064", "cpu"], true),
        "/api/v1/engines/llama-server/b12064/cpu?confirm=true"
    );
    assert_eq!(
        removal_route(["a/b", "t", "v"], false),
        "/api/v1/engines/a%2Fb/t/v"
    );
}

#[test]
fn test_after_text_says_what_the_choice_comes_down_to() {
    // GIVEN the two answers `after` carries
    // WHEN each is put in words
    // THEN auto says the automatic choice, a tag says the build taking the place
    assert!(after_text("auto").contains("back to auto"));
    assert!(after_text("b12100").contains("b12100"));
}

// ─── against a fake daemon ───────────────────────────────────────────────────

#[tokio::test]
async fn test_install_without_a_variant_installs_the_recommended_one_and_follows_it() {
    // GIVEN a runtime rating CUDA 12 recommended, whose installation downloads then ends
    let dir = tempfile::tempdir().unwrap();
    let (socket, fake) = serve(
        dir.path(),
        vec![
            route("GET", "/api/v1/engines", vec![(200, engines_body())]),
            route(
                "POST",
                install::INSTALLATION_ROUTE,
                vec![(200, json!({"state": "verifying", "kind": "install"}))],
            ),
            route(
                "GET",
                install::INSTALLATION_ROUTE,
                vec![
                    (200, downloading(10)),
                    (200, downloading(90)),
                    (200, json!({"state": "installed", "kind": "install"})),
                ],
            ),
        ],
    );
    let client = RuntimeClient::new(socket);
    // WHEN llama-server is installed without a variant
    let code = install::install_with(
        at(&client, POLL),
        wanted(InstallEngine::LlamaServer, None),
        never(),
    )
    .await;
    // THEN it exits 0, having asked for CUDA 12 and read the installation until it ended
    assert_eq!(code, exit_codes::SUCCESS);
    assert_eq!(
        fake.body_of("POST /api/v1/engines/installation "),
        Some(json!({"engine": "llama-server", "variant": "cuda-12"}))
    );
    assert_eq!(fake.count("GET /api/v1/engines/installation "), 3);
}

#[tokio::test]
async fn test_install_of_a_named_variant_asks_for_that_one() {
    // GIVEN a runtime that installs at once
    let dir = tempfile::tempdir().unwrap();
    let (socket, fake) = serve(
        dir.path(),
        vec![
            route("GET", "/api/v1/engines", vec![(200, engines_body())]),
            route(
                "POST",
                install::INSTALLATION_ROUTE,
                vec![(200, json!({"state": "installed", "kind": "install"}))],
            ),
        ],
    );
    let client = RuntimeClient::new(socket);
    // WHEN Vulkan is named
    let code = install::install_with(
        at(&client, POLL),
        wanted(InstallEngine::LlamaServer, Some("vulkan")),
        never(),
    )
    .await;
    // THEN Vulkan is the one asked for
    assert_eq!(code, exit_codes::SUCCESS);
    assert_eq!(
        fake.body_of("POST /api/v1/engines/installation ").unwrap()["variant"],
        "vulkan"
    );
}

#[tokio::test]
async fn test_install_that_fails_exits_3() {
    // GIVEN a runtime whose download fails on the digest
    let dir = tempfile::tempdir().unwrap();
    let (socket, _) = serve(
        dir.path(),
        vec![
            route("GET", "/api/v1/engines", vec![(200, engines_body())]),
            route(
                "POST",
                install::INSTALLATION_ROUTE,
                vec![(200, downloading(0))],
            ),
            route(
                "GET",
                install::INSTALLATION_ROUTE,
                vec![(
                    200,
                    json!({"state": "failed", "kind": "install", "code": "digest_mismatch", "error": "the archive does not match its digest", "archive_requested": true}),
                )],
            ),
        ],
    );
    let client = RuntimeClient::new(socket);
    // WHEN the recommended variant is installed
    let code = install::install_with(
        at(&client, POLL),
        wanted(InstallEngine::LlamaServer, None),
        never(),
    )
    .await;
    // THEN it exits 3
    assert_eq!(code, exit_codes::TASK_FAILED);
}

#[tokio::test]
async fn test_install_of_an_incompatible_variant_needs_force() {
    // GIVEN a runtime rating Metal incompatible
    for (force, expected, posts) in [
        (false, exit_codes::GENERAL_ERROR, 0),
        (true, exit_codes::SUCCESS, 1),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (socket, fake) = serve(
            dir.path(),
            vec![
                route("GET", "/api/v1/engines", vec![(200, engines_body())]),
                route(
                    "POST",
                    install::INSTALLATION_ROUTE,
                    vec![(200, json!({"state": "installed", "kind": "install"}))],
                ),
            ],
        );
        let client = RuntimeClient::new(socket);
        let mut asked = wanted(InstallEngine::LlamaServer, Some("metal"));
        asked.force = force;
        // WHEN Metal is installed, without then with --force
        let code = install::install_with(at(&client, POLL), asked, never()).await;
        // THEN it is refused with exit 1 and nothing posted, then installed
        assert_eq!(code, expected, "force={force}");
        assert_eq!(
            fake.count("POST /api/v1/engines/installation "),
            posts,
            "force={force}"
        );
    }
}

#[tokio::test]
async fn test_install_detach_returns_once_accepted() {
    // GIVEN a runtime that accepts the installation
    let dir = tempfile::tempdir().unwrap();
    let (socket, fake) = serve(
        dir.path(),
        vec![
            route("GET", "/api/v1/engines", vec![(200, engines_body())]),
            route(
                "POST",
                install::INSTALLATION_ROUTE,
                vec![(200, json!({"state": "verifying", "kind": "install"}))],
            ),
        ],
    );
    let client = RuntimeClient::new(socket);
    let mut asked = wanted(InstallEngine::LlamaServer, None);
    asked.detach = true;
    // WHEN it is installed with --detach
    let code = install::install_with(at(&client, POLL), asked, never()).await;
    // THEN it exits 0 without reading the installation
    assert_eq!(code, exit_codes::SUCCESS);
    assert_eq!(fake.count("GET /api/v1/engines/installation "), 0);
}

#[tokio::test]
async fn test_interrupting_an_install_cancels_it_and_exits_5() {
    // GIVEN a runtime downloading the archive, which a cancellation brings back to idle
    let dir = tempfile::tempdir().unwrap();
    let (socket, fake) = serve(
        dir.path(),
        vec![
            route("GET", "/api/v1/engines", vec![(200, engines_body())]),
            route(
                "POST",
                install::INSTALLATION_ROUTE,
                vec![(200, downloading(10))],
            ),
            route(
                "GET",
                install::INSTALLATION_ROUTE,
                vec![(200, downloading(20))],
            ),
            route(
                "DELETE",
                install::INSTALLATION_ROUTE,
                vec![(200, json!({"state": "idle"}))],
            ),
        ],
    );
    let client = RuntimeClient::new(socket);
    // WHEN the interruption Ctrl-C resolves arrives during the download
    let code = install::install_with(
        at(&client, Duration::from_secs(60)),
        wanted(InstallEngine::LlamaServer, None),
        std::future::ready(()),
    )
    .await;
    // THEN the installation is cancelled and the exit is 5
    assert_eq!(code, exit_codes::INTERRUPTED);
    assert_eq!(fake.count("DELETE /api/v1/engines/installation "), 1);
}

#[tokio::test]
async fn test_install_nodejs_goes_through_its_own_route() {
    // GIVEN a runtime downloading Node.js, then holding it
    let dir = tempfile::tempdir().unwrap();
    let (socket, fake) = serve(
        dir.path(),
        vec![
            route(
                "POST",
                install::NODE_ROUTE,
                vec![(
                    200,
                    json!({"state": "downloading", "received": 0, "total": 0}),
                )],
            ),
            route(
                "GET",
                install::NODE_ROUTE,
                vec![(
                    200,
                    json!({"state": "ready", "tag": "v24.22.2", "path": "/s/nodejs"}),
                )],
            ),
        ],
    );
    let client = RuntimeClient::new(socket);
    // WHEN Node.js is installed
    let code = install::install_with(
        at(&client, POLL),
        wanted(InstallEngine::Nodejs, None),
        never(),
    )
    .await;
    // THEN it posts the Node.js route, never the installation route, and exits 0
    assert_eq!(code, exit_codes::SUCCESS);
    assert_eq!(fake.count("POST /api/v1/engines/nodejs "), 1);
    assert_eq!(fake.count("POST /api/v1/engines/installation "), 0);
    assert_eq!(fake.count("GET /api/v1/engines "), 0);
}

#[tokio::test]
async fn test_install_nodejs_with_a_variant_exits_1() {
    // GIVEN a socket nothing listens on
    let dir = tempfile::tempdir().unwrap();
    let client = RuntimeClient::new(dir.path().join("absent.sock"));
    // WHEN Node.js is installed with a variant
    let code = install::install_with(
        at(&client, POLL),
        wanted(InstallEngine::Nodejs, Some("cpu")),
        never(),
    )
    .await;
    // THEN it is refused before any call
    assert_eq!(code, exit_codes::GENERAL_ERROR);
}

#[tokio::test]
async fn test_import_hands_the_absolute_directory_and_follows_it() {
    // GIVEN a runtime that verifies, extracts then installs the directory
    let dir = tempfile::tempdir().unwrap();
    let (socket, fake) = serve(
        dir.path(),
        vec![
            route(
                "POST",
                install::IMPORT_ROUTE,
                vec![(200, json!({"state": "verifying", "kind": "import"}))],
            ),
            route(
                "GET",
                install::INSTALLATION_ROUTE,
                vec![
                    (200, json!({"state": "installing", "kind": "import"})),
                    (200, json!({"state": "installed", "kind": "import"})),
                ],
            ),
        ],
    );
    let client = RuntimeClient::new(socket);
    // WHEN a relative directory is imported
    let code = install::import_with(
        at(&client, POLL),
        Path::new("engines-stable"),
        false,
        never(),
    )
    .await;
    // THEN the runtime is handed its absolute path and the exit is 0
    assert_eq!(code, exit_codes::SUCCESS);
    let path = fake
        .body_of("POST /api/v1/engines/installation/import ")
        .unwrap()["path"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(Path::new(&path).is_absolute(), "{path}");
    assert!(path.ends_with("engines-stable"), "{path}");
}

#[tokio::test]
async fn test_import_of_a_refused_directory_exits_2() {
    // GIVEN a runtime refusing a directory with no manifest, then one signed by another key
    for (code, error) in [
        ("storage", "engines.json: No such file or directory"),
        (
            "signature_unknown_key",
            "the manifest is signed by another key than the distribution key",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (socket, _) = serve(
            dir.path(),
            vec![
                route(
                    "POST",
                    install::IMPORT_ROUTE,
                    vec![(200, json!({"state": "verifying", "kind": "import"}))],
                ),
                route(
                    "GET",
                    install::INSTALLATION_ROUTE,
                    vec![(
                        200,
                        json!({"state": "failed", "kind": "import", "code": code, "error": error, "archive_requested": false}),
                    )],
                ),
            ],
        );
        let client = RuntimeClient::new(socket);
        // WHEN the directory is imported
        let exit = install::import_with(at(&client, POLL), dir.path(), false, never()).await;
        // THEN it exits 2
        assert_eq!(exit, exit_codes::RUNTIME_ERROR, "{code}");
    }
}

#[tokio::test]
async fn test_import_detach_returns_once_accepted() {
    // GIVEN a runtime that accepts the import
    let dir = tempfile::tempdir().unwrap();
    let (socket, fake) = serve(
        dir.path(),
        vec![route(
            "POST",
            install::IMPORT_ROUTE,
            vec![(200, json!({"state": "verifying", "kind": "import"}))],
        )],
    );
    let client = RuntimeClient::new(socket);
    // WHEN it is imported with --detach
    let code = install::import_with(at(&client, POLL), dir.path(), true, never()).await;
    // THEN it exits 0 without reading the installation
    assert_eq!(code, exit_codes::SUCCESS);
    assert_eq!(fake.count("GET /api/v1/engines/installation "), 0);
}

async fn run_remove(socket: PathBuf, confirm: bool) -> i32 {
    let cmd = EngineCommand::Remove {
        engine: "llama-server".to_owned(),
        tag: "b12064".to_owned(),
        variant: "cpu".to_owned(),
        confirm,
    };
    super::run(&cmd, Some(socket), true).await
}

const BUILD: &str = "/api/v1/engines/llama-server/b12064/cpu";

#[tokio::test]
async fn test_remove_of_the_chosen_build_confirms_and_exits_0() {
    // GIVEN a runtime refusing the chosen build without confirmation, `after` auto
    let dir = tempfile::tempdir().unwrap();
    let confirmed = format!("{BUILD}?confirm=true");
    let (socket, fake) = serve(
        dir.path(),
        vec![
            route(
                "DELETE",
                BUILD,
                vec![(
                    409,
                    json!({"error": "chosen", "code": "chosen", "after": "auto"}),
                )],
            ),
            route("DELETE", &confirmed, vec![(204, Value::Null)]),
        ],
    );
    // WHEN it is removed with --confirm
    let code = run_remove(socket, true).await;
    // THEN the removal is asked again with confirm=true and exits 0
    assert_eq!(code, exit_codes::SUCCESS);
    assert_eq!(fake.count(&format!("DELETE {confirmed} ")), 1);
}

#[tokio::test]
async fn test_remove_of_a_build_not_chosen_asks_once() {
    // GIVEN a runtime that removes the build at once
    let dir = tempfile::tempdir().unwrap();
    let (socket, fake) = serve(
        dir.path(),
        vec![route("DELETE", BUILD, vec![(204, Value::Null)])],
    );
    // WHEN it is removed
    let code = run_remove(socket, true).await;
    // THEN one call, exit 0
    assert_eq!(code, exit_codes::SUCCESS);
    assert_eq!(fake.lines().len(), 1);
}

#[tokio::test]
async fn test_remove_refused_by_the_runtime_exits_with_the_contract() {
    // GIVEN a runtime refusing the build in service, the embedded one, a locked one, an absent one, then failing
    for (status, code, expected) in [
        (409, "in_service", exit_codes::GENERAL_ERROR),
        (409, "embedded", exit_codes::GENERAL_ERROR),
        (403, "locked", exit_codes::GENERAL_ERROR),
        (404, "not_found", exit_codes::GENERAL_ERROR),
        (500, "store", exit_codes::RUNTIME_ERROR),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (socket, fake) = serve(
            dir.path(),
            vec![route(
                "DELETE",
                BUILD,
                vec![(status, json!({"error": code, "code": code}))],
            )],
        );
        // WHEN it is removed with --confirm
        let exit = run_remove(socket, true).await;
        // THEN the exit follows the contract and nothing is asked twice
        assert_eq!(exit, expected, "{code}");
        assert_eq!(fake.lines().len(), 1, "{code}");
    }
}

#[tokio::test]
async fn test_remove_without_confirm_is_refused_before_any_call() {
    // GIVEN a runtime that would remove the build
    let dir = tempfile::tempdir().unwrap();
    let (socket, fake) = serve(
        dir.path(),
        vec![route("DELETE", BUILD, vec![(204, Value::Null)])],
    );
    // WHEN it is removed without --confirm under --json
    let code = run_remove(socket, false).await;
    // THEN exit 1 and the runtime never asked
    assert_eq!(code, exit_codes::GENERAL_ERROR);
    assert!(fake.lines().is_empty());
}

#[tokio::test]
async fn test_new_leaves_exit_2_when_no_runtime_answers() {
    // GIVEN a socket no runtime listens on
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("absent.sock");
    // WHEN install, import and remove run
    // THEN each exits 2
    for cmd in [
        EngineCommand::Install {
            engine: InstallEngine::LlamaServer,
            variant: None,
            detach: false,
            force: false,
        },
        EngineCommand::Install {
            engine: InstallEngine::Nodejs,
            variant: None,
            detach: true,
            force: false,
        },
        EngineCommand::Import {
            dir: dir.path().to_path_buf(),
            detach: false,
        },
    ] {
        assert_eq!(
            super::run(&cmd, Some(socket.clone()), true).await,
            exit_codes::RUNTIME_ERROR
        );
    }
    assert_eq!(run_remove(socket, true).await, exit_codes::RUNTIME_ERROR);
}
