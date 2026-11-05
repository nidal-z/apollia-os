//! `engine` against canned route bodies and a fake daemon on a Unix socket.

use clap::Parser;
use serde_json::{json, Value};

use super::render::{self, ChoiceVerdict, Line, CATALOGUE_NOT_READ, CHECK_HINT};
use super::{with_gguf_choice, ChannelArg, EngineCommand, APPLIES};
use crate::exit_codes;
use crate::{Cli, Commands};

fn parse(args: &[&str]) -> Cli {
    Cli::parse_from(args)
}

fn engine_command(cli: Cli) -> EngineCommand {
    let Commands::Engine { command } = cli.command else {
        panic!("expected engine");
    };
    command
}

fn texts(lines: &[Line]) -> Vec<String> {
    lines.iter().map(|l| l.text().to_owned()).collect()
}

/// A body of `GET /api/v1/engines` on a Linux machine with an NVIDIA card
/// whose driver is older than the CUDA 12 toolkit.
fn linux_nvidia_body() -> Value {
    json!({
        "engines": [
            {"engine": "llama-server", "tag": null, "variant": null, "path": "/opt/apollia/llama-server",
             "source": "embedded", "version": "b12064", "in_service": false},
            {"engine": "llama-server", "tag": "b12064", "variant": "cuda-12", "path": "/s/llama-server/b12064/cuda-12",
             "source": "downloaded", "version": "b12064", "in_service": true},
            {"engine": "nodejs", "tag": "v24.22.2", "variant": "cpu", "path": "/s/nodejs/v24.22.2/cpu",
             "source": "downloaded", "version": "v24.22.2", "in_service": false}
        ],
        "channel": "stable",
        "manifest_sequence": 7,
        "updates": [],
        "hardware": {"cards": [{"vendor": "nvidia", "name": "NVIDIA RTX A4000", "vram_gb": 16.0,
            "driver_version": "560.94", "compute_capability": "8.6", "gfx_target": null,
            "kfd_accessible": null, "timed_out": false}], "vulkan_loader": true, "rocm_runtime": false},
        "available": [
            {"engine": "llama-server", "tag": "b12064", "variant": "cuda-12", "size": 1, "released": null, "installed": true},
            {"engine": "llama-server", "tag": "b12064", "variant": "vulkan", "size": 1, "released": null, "installed": false},
            {"engine": "llama-server", "tag": "b12064", "variant": "cpu", "size": 1, "released": null, "installed": false},
            {"engine": "nodejs", "tag": "v24.22.2", "variant": "cpu", "size": 1, "released": null, "installed": true}
        ],
        "ratings": [
            {"variant": "cuda-12", "verdict": "recommended",
             "reason": {"code": "driver_too_old", "have": "560.94", "need": "570.26"}},
            {"variant": "cuda-13", "verdict": "incompatible",
             "reason": {"code": "driver_too_old", "have": "560.94", "need": "580"}},
            {"variant": "rocm", "verdict": "incompatible", "reason": {"code": "no_gpu"}},
            {"variant": "vulkan", "verdict": "compatible", "reason": null},
            {"variant": "cpu", "verdict": "compatible", "reason": null},
            {"variant": "metal", "verdict": "incompatible", "reason": {"code": "not_built_for_platform"}}
        ],
        "dictation": null,
        "verifiable": true
    })
}

// ─── parsing ─────────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_engine_list_and_list_all() {
    // GIVEN "engine list" and "engine list --all"
    // WHEN the top-level parser reads them
    let plain = engine_command(parse(&["apollia-os", "engine", "list"]));
    let all = engine_command(parse(&["apollia-os", "engine", "list", "--all"]));
    // THEN the flag is read
    assert!(matches!(plain, EngineCommand::List { all: false }));
    assert!(matches!(all, EngineCommand::List { all: true }));
}

#[test]
fn test_cli_parses_engine_check() {
    // GIVEN "engine check"
    // WHEN the parser reads it
    let cmd = engine_command(parse(&["apollia-os", "engine", "check"]));
    // THEN it is the check
    assert!(matches!(cmd, EngineCommand::Check));
}

#[test]
fn test_cli_parses_engine_use_with_every_published_choice() {
    // GIVEN "engine use auto", then every value engines.gguf accepts
    let auto = engine_command(parse(&["apollia-os", "engine", "use", "auto"]));
    assert!(matches!(auto, EngineCommand::Use { choice: ref c } if c == "auto"));
    for choice in apollia_core::config::GGUF_ENGINE_CHOICES {
        // WHEN "engine use <choice>" is read
        let cmd = engine_command(parse(&["apollia-os", "engine", "use", choice]));
        // THEN the choice is carried as written
        assert!(matches!(cmd, EngineCommand::Use { choice: ref c } if c == choice));
    }
}

#[test]
fn test_cli_refuses_engine_use_of_an_unknown_variant() {
    // GIVEN a value engines.gguf does not accept
    // WHEN the parser reads it
    let result = Cli::try_parse_from(["apollia-os", "engine", "use", "opencl"]);
    // THEN clap refuses it, which main maps to exit 1
    assert!(result.is_err());
}

#[test]
fn test_cli_parses_engine_channel_with_and_without_a_value() {
    // GIVEN "engine channel", "engine channel stable" and "engine channel preview"
    // WHEN the parser reads them
    let read = engine_command(parse(&["apollia-os", "engine", "channel"]));
    let stable = engine_command(parse(&["apollia-os", "engine", "channel", "stable"]));
    let preview = engine_command(parse(&["apollia-os", "engine", "channel", "preview"]));
    // THEN no value reads, a value writes
    assert!(matches!(read, EngineCommand::Channel { channel: None }));
    assert!(matches!(
        stable,
        EngineCommand::Channel {
            channel: Some(ChannelArg::Stable)
        }
    ));
    assert!(matches!(
        preview,
        EngineCommand::Channel {
            channel: Some(ChannelArg::Preview)
        }
    ));
}

#[test]
fn test_cli_refuses_an_unknown_channel() {
    // GIVEN a channel the store does not have
    // WHEN the parser reads it
    let result = Cli::try_parse_from(["apollia-os", "engine", "channel", "nightly"]);
    // THEN clap refuses it
    assert!(result.is_err());
}

// ─── rendering of list ───────────────────────────────────────────────────────

#[test]
fn test_list_shows_a_recommendation_with_its_reason() {
    // GIVEN a recommended CUDA 12 build whose driver is older than its toolkit
    let body = linux_nvidia_body();
    // WHEN the list is rendered
    let lines = texts(&render::list(&body, false, false));
    // THEN the recommendation carries the driver to update
    assert!(
        lines
            .iter()
            .any(|l| l == "  cuda-12  b12064  recommended (driver 560 < 570)  installed"),
        "{lines:#?}"
    );
}

#[test]
fn test_list_shows_rocm_on_windows_as_experimental() {
    // GIVEN ROCm rated compatible without a reason, on Windows
    let mut body = linux_nvidia_body();
    body["ratings"][2] = json!({"variant": "rocm", "verdict": "compatible", "reason": null});
    body["available"].as_array_mut().unwrap().push(
        json!({"engine": "llama-server", "tag": "b12064", "variant": "rocm",
                     "size": 1, "released": null, "installed": false}),
    );
    // WHEN the list is rendered for Windows, then for Linux
    let windows = texts(&render::list(&body, false, true));
    let linux = texts(&render::list(&body, false, false));
    // THEN Windows says experimental, Linux says compatible alone
    assert!(
        windows
            .iter()
            .any(|l| l == "  rocm  b12064  compatible (experimental on Windows)"),
        "{windows:#?}"
    );
    assert!(
        linux.iter().any(|l| l == "  rocm  b12064  compatible"),
        "{linux:#?}"
    );
}

#[test]
fn test_list_without_a_manifest_says_the_catalogue_is_not_read_and_lists_the_hardware() {
    // GIVEN no verified manifest: no sequence, nothing available, nothing rated
    let mut body = linux_nvidia_body();
    body["manifest_sequence"] = Value::Null;
    body["available"] = json!([]);
    body["ratings"] = json!([]);
    // WHEN the list is rendered
    let lines = texts(&render::list(&body, false, false));
    // THEN the sentence and its hint are there, and the card is listed anyway
    assert!(
        lines.iter().any(|l| l.trim() == CATALOGUE_NOT_READ),
        "{lines:#?}"
    );
    assert!(lines.iter().any(|l| l.trim() == CHECK_HINT), "{lines:#?}");
    assert!(CHECK_HINT.starts_with("Run `apollia-os engine check`"));
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("  NVIDIA RTX A4000  driver 560.94  16.0 GiB")),
        "{lines:#?}"
    );
}

#[test]
fn test_list_counts_the_incompatible_variants_unless_all_is_asked() {
    // GIVEN three variants rated incompatible
    let body = linux_nvidia_body();
    // WHEN the list is rendered without, then with, --all
    let plain = texts(&render::list(&body, false, false));
    let all = texts(&render::list(&body, true, false));
    // THEN they are counted, then listed with their reason
    assert!(plain
        .iter()
        .any(|l| l.contains("3 variant(s) that do not run here")));
    assert!(!plain.iter().any(|l| l.contains("incompatible")));
    assert!(all
        .iter()
        .any(|l| l == "  metal  -  incompatible (not built for this platform)"));
    assert!(all
        .iter()
        .any(|l| l == "  cuda-13  -  incompatible (driver 560 < 580)"));
}

#[test]
fn test_list_shows_the_installed_engines_the_one_in_service_and_nodejs() {
    // GIVEN an embedded engine, a downloaded CUDA 12 build in service and Node.js
    let body = linux_nvidia_body();
    // WHEN the list is rendered
    let lines = render::list(&body, false, false);
    let text = texts(&lines);
    // THEN each installed engine has its row, Node.js its own section
    assert!(text
        .iter()
        .any(|l| l == "  llama-server  b12064  cuda-12  downloaded  in service"));
    assert!(text.iter().any(|l| l == "  llama-server  -  -  embedded"));
    assert!(text.iter().any(|l| l == "  installed  v24.22.2  v24.22.2"));
    // AND the headers are lines --quiet drops
    assert!(lines.contains(&Line::Note("Hardware".to_owned())));
    assert!(lines.contains(&Line::Note("Node.js".to_owned())));
}

#[test]
fn test_list_says_when_the_probe_did_not_answer() {
    // GIVEN hardware the route could not read in time
    let mut body = linux_nvidia_body();
    body["hardware"] = Value::Null;
    // WHEN the list is rendered
    let lines = texts(&render::list(&body, false, false));
    // THEN it says so
    assert!(lines
        .iter()
        .any(|l| l == "  the hardware probe did not answer in time"));
}

// ─── use ─────────────────────────────────────────────────────────────────────

#[test]
fn test_use_refuses_a_variant_not_installed() {
    // GIVEN a machine holding CUDA 12 only
    let body = linux_nvidia_body();
    // WHEN vulkan is chosen
    // THEN it is not installed
    assert_eq!(
        render::judge_choice(&body, "vulkan"),
        ChoiceVerdict::NotInstalled
    );
    // AND the embedded engine does not stand for a named variant
    let mut mac = body.clone();
    mac["engines"] = json!([{"engine": "llama-server", "variant": "metal", "source": "embedded"}]);
    assert_eq!(
        render::judge_choice(&mac, "metal"),
        ChoiceVerdict::NotInstalled
    );
}

#[test]
fn test_use_accepts_an_incompatible_installed_variant_with_a_warning() {
    // GIVEN CUDA 13 installed and rated incompatible
    let mut body = linux_nvidia_body();
    body["engines"].as_array_mut().unwrap().push(json!({
        "engine": "llama-server", "tag": "b12064", "variant": "cuda-13",
        "source": "imported", "in_service": false
    }));
    // WHEN cuda-13 is chosen
    let verdict = render::judge_choice(&body, "cuda-13");
    // THEN it is accepted with the reason it may not run
    let ChoiceVerdict::Accepted {
        warning: Some(warning),
    } = verdict
    else {
        panic!("expected a warning, got {verdict:?}");
    };
    assert!(warning.contains("driver 560 < 580"), "{warning}");
}

#[test]
fn test_use_accepts_auto_embedded_and_cuda_without_a_warning() {
    // GIVEN a machine holding a recommended CUDA 12 build
    let body = linux_nvidia_body();
    // WHEN auto, embedded or cuda is chosen
    // THEN each is accepted silently
    for choice in ["auto", "embedded", "cuda", "cuda-12"] {
        assert_eq!(
            render::judge_choice(&body, choice),
            ChoiceVerdict::Accepted { warning: None },
            "{choice}"
        );
    }
}

#[test]
fn test_use_writes_the_key_and_keeps_the_rest_of_the_file() {
    // GIVEN a file with a comment, an api section and an engines section
    let text = "# mine\n[api]\nport = 7790\n\n[engines]\nlog_level = \"info\"\ngguf = \"auto\"\n";
    // WHEN cuda-13 is chosen
    let edited = with_gguf_choice(text, "cuda-13").unwrap();
    // THEN only engines.gguf changed
    assert_eq!(
        edited,
        "# mine\n[api]\nport = 7790\n\n[engines]\nlog_level = \"info\"\ngguf = \"cuda-13\"\n"
    );
    // AND a file without the section gains it
    let added = with_gguf_choice("[api]\nport = 7790\n", "vulkan").unwrap();
    let parsed: toml::Table = toml::from_str(&added).unwrap();
    assert_eq!(parsed["engines"]["gguf"].as_str(), Some("vulkan"));
}

// ─── against a fake daemon ───────────────────────────────────────────────────

#[cfg(unix)]
mod daemon {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    use super::*;

    type Answer = (&'static str, &'static str, u16, Value);

    /// What the fake daemon answers per method and path, and what it received.
    struct Fake {
        answers: Vec<Answer>,
        received: Mutex<Vec<(String, String)>>,
    }

    async fn read_request(stream: &mut tokio::net::UnixStream) -> (String, String) {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let n = stream.read(&mut chunk).await.unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&buf).to_string();
            if let Some(end) = text.find("\r\n\r\n") {
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
        (String::new(), String::new())
    }

    fn serve(dir: &Path, answers: Vec<Answer>) -> (PathBuf, Arc<Fake>) {
        let fake = Arc::new(Fake {
            answers,
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
                    let (status, answer) = fake
                        .answers
                        .iter()
                        .find(|(method, path, _, _)| {
                            first.starts_with(&format!("{method} {path} "))
                        })
                        .map(|(_, _, status, body)| (*status, body.to_string()))
                        .unwrap_or((404, r#"{"error":"no route"}"#.to_string()));
                    let response = format!(
                        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}",
                        answer.len()
                    );
                    stream.write_all(response.as_bytes()).await.unwrap();
                    stream.shutdown().await.ok();
                });
            }
        });
        (socket, fake)
    }

    async fn run(cmd: EngineCommand, socket: PathBuf) -> i32 {
        super::super::run(&cmd, Some(socket), true).await
    }

    #[tokio::test]
    async fn test_check_exits_0_when_the_channel_verifies() {
        // GIVEN a runtime whose check verifies manifest 7
        let dir = tempfile::tempdir().unwrap();
        let (socket, fake) = serve(
            dir.path(),
            vec![(
                "POST",
                "/api/v1/engines/check",
                200,
                json!({"channel": "stable", "manifest_sequence": 7, "updates": []}),
            )],
        );
        // WHEN engine check runs
        let code = run(EngineCommand::Check, socket).await;
        // THEN it exits 0, having posted the check
        assert_eq!(code, exit_codes::SUCCESS);
        assert!(fake.received.lock().unwrap()[0]
            .0
            .starts_with("POST /api/v1/engines/check "));
    }

    #[tokio::test]
    async fn test_check_exits_2_when_the_channel_is_out_of_reach_or_its_signature_refused() {
        // GIVEN a runtime that cannot reach the mirror, then one that refuses the signature
        for (status, message) in [
            (502, "the mirror could not be reached"),
            (422, "the manifest signature does not verify"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let (socket, _) = serve(
                dir.path(),
                vec![(
                    "POST",
                    "/api/v1/engines/check",
                    status,
                    json!({ "error": message }),
                )],
            );
            // WHEN engine check runs
            let code = run(EngineCommand::Check, socket).await;
            // THEN it exits 2
            assert_eq!(code, exit_codes::RUNTIME_ERROR, "{status}");
        }
    }

    #[tokio::test]
    async fn test_every_leaf_exits_2_when_no_runtime_answers() {
        // GIVEN a socket no runtime listens on
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("absent.sock");
        // WHEN each leaf runs
        // THEN each exits 2
        for cmd in [
            EngineCommand::List { all: false },
            EngineCommand::Check,
            EngineCommand::Use {
                choice: "auto".to_owned(),
            },
            EngineCommand::Channel { channel: None },
            EngineCommand::Channel {
                channel: Some(ChannelArg::Preview),
            },
        ] {
            assert_eq!(run(cmd, socket.clone()).await, exit_codes::RUNTIME_ERROR);
        }
    }

    #[tokio::test]
    async fn test_use_puts_the_edited_file_under_the_fingerprint_read() {
        // GIVEN a runtime holding CUDA 12 and a file with an api section
        let dir = tempfile::tempdir().unwrap();
        let (socket, fake) = serve(
            dir.path(),
            vec![
                ("GET", "/api/v1/engines", 200, linux_nvidia_body()),
                (
                    "GET",
                    "/api/v1/config",
                    200,
                    json!({"text": "[api]\nport = 7790\n", "sha256": "abc"}),
                ),
                (
                    "PUT",
                    "/api/v1/config",
                    200,
                    json!({"text": "", "sha256": "def"}),
                ),
            ],
        );
        // WHEN cuda-12 is chosen
        let code = run(
            EngineCommand::Use {
                choice: "cuda-12".to_owned(),
            },
            socket,
        )
        .await;
        // THEN the file is saved with the key, from the fingerprint read, as the CLI
        assert_eq!(code, exit_codes::SUCCESS);
        let received = fake.received.lock().unwrap();
        let (_, put) = received
            .iter()
            .find(|(first, _)| first.starts_with("PUT /api/v1/config "))
            .expect("a save");
        let put: Value = serde_json::from_str(put).unwrap();
        assert_eq!(put["sha256"], "abc");
        assert_eq!(put["origin"], "cli");
        assert!(put["text"]
            .as_str()
            .unwrap()
            .contains("[engines]\ngguf = \"cuda-12\""));
        assert!(APPLIES.contains("the next time a model is loaded"));
    }

    #[tokio::test]
    async fn test_use_of_a_variant_not_installed_exits_1_without_writing() {
        // GIVEN a runtime holding CUDA 12 only
        let dir = tempfile::tempdir().unwrap();
        let (socket, fake) = serve(
            dir.path(),
            vec![("GET", "/api/v1/engines", 200, linux_nvidia_body())],
        );
        // WHEN rocm is chosen
        let code = run(
            EngineCommand::Use {
                choice: "rocm".to_owned(),
            },
            socket,
        )
        .await;
        // THEN it exits 1 and nothing was saved
        assert_eq!(code, exit_codes::GENERAL_ERROR);
        assert!(!fake
            .received
            .lock()
            .unwrap()
            .iter()
            .any(|(first, _)| first.starts_with("PUT ")));
    }

    #[tokio::test]
    async fn test_channel_reads_then_writes_keeping_the_mirror_address() {
        // GIVEN a runtime following stable from a mirror
        let dir = tempfile::tempdir().unwrap();
        let settings = json!({"channel": "stable", "base_url": "https://mirror.example/engines"});
        let (socket, fake) = serve(
            dir.path(),
            vec![
                ("GET", "/api/v1/engines/settings", 200, settings),
                (
                    "PUT",
                    "/api/v1/engines/settings",
                    200,
                    json!({"channel": "preview", "base_url": "https://mirror.example/engines"}),
                ),
            ],
        );
        // WHEN the channel is read, then set to preview
        let read = run(EngineCommand::Channel { channel: None }, socket.clone()).await;
        let write = run(
            EngineCommand::Channel {
                channel: Some(ChannelArg::Preview),
            },
            socket,
        )
        .await;
        // THEN both exit 0 and the write keeps the mirror address
        assert_eq!(read, exit_codes::SUCCESS);
        assert_eq!(write, exit_codes::SUCCESS);
        let received = fake.received.lock().unwrap();
        let (_, put) = received
            .iter()
            .find(|(first, _)| first.starts_with("PUT /api/v1/engines/settings "))
            .expect("a write");
        let put: Value = serde_json::from_str(put).unwrap();
        assert_eq!(
            put,
            json!({"channel": "preview", "base_url": "https://mirror.example/engines"})
        );
    }

    #[tokio::test]
    async fn test_list_exits_0_without_a_manifest() {
        // GIVEN a runtime that has read no manifest yet
        let dir = tempfile::tempdir().unwrap();
        let mut body = linux_nvidia_body();
        body["manifest_sequence"] = Value::Null;
        body["available"] = json!([]);
        body["ratings"] = json!([]);
        let (socket, _) = serve(dir.path(), vec![("GET", "/api/v1/engines", 200, body)]);
        // WHEN engine list runs
        let code = run(EngineCommand::List { all: false }, socket).await;
        // THEN it exits 0
        assert_eq!(code, exit_codes::SUCCESS);
    }
}
