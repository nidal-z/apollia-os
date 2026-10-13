use super::*;
use crate::llama_server::test_support::supervisor_with_ceiling;
use apollia_core::model_preset::{ModelSource, PresetDraft, RequestSettings};
use apollia_core::LlmBackendRepository;
use serde_json::json;

/// A model no test reads: its window comes from the settings alone.
const MODEL: &str = "/nonexistent/model.gguf";

#[test]
fn test_the_environment_window_wins_over_the_preset() {
    // GIVEN APOLLIA_LLAMA_N_CTX pinned at 4096 and a preset that fixes 8192
    let mut sup = supervisor_with_ceiling(1);
    sup.n_ctx = 4096;
    sup.n_ctx_pinned = true;
    let load = LoadSettings {
        n_ctx: Some(8192),
        ..LoadSettings::default()
    };

    // WHEN the window is resolved for a backend asking for 16384
    let window = sup.window_for_blocking(MODEL, &load, Some(16_384));

    // THEN the environment wins
    assert_eq!(window, 4096);
}

#[test]
fn test_the_preset_window_wins_over_the_automatic_one() {
    // GIVEN no variable, a backend asking for 16384 and a default of 32768
    let sup = supervisor_with_ceiling(1);
    let load = LoadSettings {
        n_ctx: Some(8192),
        ..LoadSettings::default()
    };

    // WHEN the window is resolved under a preset that fixes 8192
    let window = sup.window_for_blocking(MODEL, &load, Some(16_384));
    let without = sup.window_for_blocking(MODEL, &LoadSettings::default(), None);

    // THEN the preset wins, and without it the default stands
    assert_eq!(window, 8192);
    assert_eq!(without, 32_768);
}

#[test]
fn test_the_window_is_resolved_in_the_full_order_down_to_the_built_in_default() {
    // GIVEN a default of 16384 tokens in the settings, a preset at 8192, a
    //       backend asking for 12288, and the built-in window of 32768
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("apollia.toml");
    std::fs::write(&file, "[llm.defaults]\ncontext_tokens = 16384\n").expect("write");
    let mut sup = supervisor_with_ceiling(1);
    sup.config_file = Some(file.clone());
    let preset = LoadSettings {
        n_ctx: Some(8192),
        ..LoadSettings::default()
    };
    let none = LoadSettings::default();

    // WHEN the window is resolved with each source removed in turn, the
    //      environment last since it is read once, at startup
    let under_preset = sup.window_for_blocking(MODEL, &preset, Some(12_288));
    let under_backend = sup.window_for_blocking(MODEL, &none, Some(12_288));
    let under_setting = sup.window_for_blocking(MODEL, &none, None);
    std::fs::write(&file, "").expect("write");
    let built_in = sup.window_for_blocking(MODEL, &none, None);
    sup.n_ctx = 4096;
    sup.n_ctx_pinned = true;
    std::fs::write(&file, "[llm.defaults]\ncontext_tokens = 16384\n").expect("write");
    let under_env = sup.window_for_blocking(MODEL, &preset, Some(12_288));

    // THEN the environment wins, then the preset, then the backend, then the
    //      default of the settings, and 32768 is the last resort
    assert_eq!(
        (
            under_env,
            under_preset,
            under_backend,
            under_setting,
            built_in
        ),
        (4096, 8192, 12_288, 16_384, 32_768)
    );
}

#[tokio::test]
async fn test_a_default_changed_in_the_settings_leaves_a_resident_model_serving() {
    // GIVEN a model resident at the built-in window, and settings that now
    //       state a default of 16384 tokens
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("apollia.toml");
    std::fs::write(&file, "[llm.defaults]\ncontext_tokens = 16384\n").expect("write");
    let mut sup = supervisor_with_ceiling(1);
    sup.config_file = Some(file);
    let resident = crate::llama_server::test_support::live_instance(&sup, MODEL, 9001).await;
    sup.instances.lock().await.push(resident);

    // WHEN a request asks for the model without fixing a window
    let lease = sup
        .ensure_model("local", MODEL.to_owned(), None, LoadConsent::Unconfirmed)
        .await;

    // THEN the resident process serves it at its own window, and nothing is
    //      launched: the engine of this supervisor does not exist
    assert_eq!(lease.map(|l| l.n_ctx()).ok(), Some(32_768));
    assert_eq!(sup.instances.lock().await.len(), 1);
}

/// A stand-in for `llama-server`: it appends each command line to `args.log`
/// in `dir`, then answers every request with 200 on the port it was given.
#[cfg(unix)]
fn fake_engine(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let log = dir.join("args.log");
    let script = format!(
        r#"#!/bin/sh
echo "$@" >> "{log}"
port=""
while [ $# -gt 0 ]; do
  if [ "$1" = "--port" ]; then port="$2"; fi
  shift
done
exec python3 -c '
import http.server, sys
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"{{}}")
    def log_message(self, *args):
        pass
class Quick(http.server.HTTPServer):
    # HTTPServer.server_bind asks the resolver for the name of the host. On a
    # machine with no reverse lookup that takes seconds per start, which the
    # health deadline of a test then reads as an engine that never came up.
    def server_bind(self):
        import socketserver
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = self.server_address[:2]
Quick(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
' "$port"
"#,
        log = log.display()
    );
    let path = dir.join("llama-server");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
fn launches(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("args.log"))
        .unwrap_or_default()
        .lines()
        // Provenance probes the binary with `--version`; only a line naming
        // a port is a launch.
        .filter(|line| line.contains("--port"))
        .map(str::to_owned)
        .collect()
}

#[cfg(unix)]
fn default_preset(load: serde_json::Value) -> PresetDraft {
    PresetDraft {
        name: "main".to_owned(),
        load: load.as_object().cloned().unwrap(),
        request: RequestSettings::default(),
        is_model_default: true,
    }
}

#[cfg(unix)]
#[tokio::test]
async fn test_the_default_preset_applies_when_a_model_is_ensured() {
    // GIVEN a model whose default preset sets six threads, and a supervisor
    // reading that system.db with a stand-in engine
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("system.db");
    let repo = LlmBackendRepository::open(&db).unwrap();
    repo.create_preset(
        ModelSource::Local,
        "qwen",
        &default_preset(json!({ "n_threads": 6 })),
    )
    .unwrap();
    drop(repo);
    let mut sup = supervisor_with_ceiling(1);
    sup.bin_path = fake_engine(dir.path());
    sup.system_db = Some(db);

    // WHEN the model is ensured
    let lease = sup
        .ensure_model(
            "qwen",
            "/models/qwen.gguf".to_owned(),
            None,
            LoadConsent::Unconfirmed,
        )
        .await
        .unwrap();
    drop(lease);

    // THEN the engine was launched with the preset's thread count
    let lines = launches(dir.path());
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(lines[0].contains(" -t 6 "), "{}", lines[0]);
    sup.shutdown_in_place().await;
}

#[cfg(unix)]
#[tokio::test]
async fn test_changing_a_load_setting_relaunches_the_resident_model() {
    // GIVEN a resident model launched under its default preset of six threads,
    // and a bus listening
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("system.db");
    let repo = LlmBackendRepository::open(&db).unwrap();
    let preset = repo
        .create_preset(
            ModelSource::Local,
            "qwen",
            &default_preset(json!({ "n_threads": 6 })),
        )
        .unwrap();
    let (tx, mut rx) = tokio::sync::broadcast::channel(16);
    let mut sup = supervisor_with_ceiling(1);
    sup.bin_path = fake_engine(dir.path());
    sup.system_db = Some(db);
    sup.events = Some(tx);
    drop(
        sup.ensure_model(
            "qwen",
            "/models/qwen.gguf".to_owned(),
            None,
            LoadConsent::Unconfirmed,
        )
        .await
        .unwrap(),
    );
    let first_port = sup.instances.lock().await[0].port;

    // WHEN the preset's thread count changes and the model is requested again
    repo.update_preset(preset.id, &default_preset(json!({ "n_threads": 2 })))
        .unwrap();
    drop(
        sup.ensure_model(
            "qwen",
            "/models/qwen.gguf".to_owned(),
            None,
            LoadConsent::Unconfirmed,
        )
        .await
        .unwrap(),
    );

    // THEN the first process is gone, a second one runs the new setting, and
    // the bus heard the reload
    let instances = sup.instances.lock().await;
    assert_eq!(instances.len(), 1);
    assert_ne!(instances[0].port, first_port);
    assert_eq!(instances[0].load.n_threads, Some(2));
    drop(instances);
    let lines = launches(dir.path());
    assert_eq!(lines.len(), 2);
    assert!(lines[1].contains(" -t 2 "), "{}", lines[1]);
    let mut heard = Vec::new();
    while let Ok(event) = rx.try_recv() {
        heard.push(event);
    }
    assert!(
        heard.iter().any(|event| matches!(
            event,
            RuntimeEvent::LlmModelReloaded { backend, model_path }
                if backend == "qwen" && model_path == "/models/qwen.gguf"
        )),
        "expected LlmModelReloaded among {heard:?}"
    );
    // AND each of the two launches was announced as a load that came up
    let loads = |wanted: fn(&RuntimeEvent) -> bool| heard.iter().filter(|e| wanted(e)).count();
    assert_eq!(
        loads(|e| matches!(e, RuntimeEvent::LlmModelLoading { .. })),
        2
    );
    assert_eq!(
        loads(|e| matches!(e, RuntimeEvent::LlmModelReady { .. })),
        2
    );
    sup.shutdown_in_place().await;
}

#[cfg(unix)]
#[tokio::test]
async fn test_an_unchanged_preset_keeps_the_resident_model() {
    // GIVEN a resident model launched under its default preset
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("system.db");
    let repo = LlmBackendRepository::open(&db).unwrap();
    repo.create_preset(
        ModelSource::Local,
        "qwen",
        &default_preset(json!({ "n_threads": 6 })),
    )
    .unwrap();
    drop(repo);
    let (tx, mut rx) = tokio::sync::broadcast::channel(16);
    let mut sup = supervisor_with_ceiling(1);
    sup.bin_path = fake_engine(dir.path());
    sup.system_db = Some(db);
    sup.events = Some(tx);
    drop(
        sup.ensure_model(
            "qwen",
            "/models/qwen.gguf".to_owned(),
            None,
            LoadConsent::Unconfirmed,
        )
        .await
        .unwrap(),
    );

    // The first launch announced its load: what follows is heard from here.
    while rx.try_recv().is_ok() {}

    // WHEN it is requested again with the preset untouched
    drop(
        sup.ensure_model(
            "qwen",
            "/models/qwen.gguf".to_owned(),
            None,
            LoadConsent::Unconfirmed,
        )
        .await
        .unwrap(),
    );

    // THEN the engine was launched once and nothing was announced
    assert_eq!(launches(dir.path()).len(), 1);
    assert!(rx.try_recv().is_err());
    sup.shutdown_in_place().await;
}

#[cfg(unix)]
#[tokio::test]
async fn test_a_preset_fixing_the_slots_keeps_the_backend_window_and_the_process() {
    // GIVEN a default preset that fixes two slots and leaves the window, and a
    // backend asking for 16384
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("system.db");
    let repo = LlmBackendRepository::open(&db).unwrap();
    repo.create_preset(
        ModelSource::Local,
        "qwen",
        &default_preset(json!({ "n_parallel": 2 })),
    )
    .unwrap();
    drop(repo);
    let mut sup = supervisor_with_ceiling(1);
    sup.bin_path = fake_engine(dir.path());
    sup.system_db = Some(db);

    // WHEN the model is requested twice with the preset untouched
    let first = sup
        .ensure_model(
            "qwen",
            "/models/qwen.gguf".to_owned(),
            Some(16_384),
            LoadConsent::Unconfirmed,
        )
        .await
        .unwrap();
    let first_window = first.n_ctx();
    drop(first);
    drop(
        sup.ensure_model(
            "qwen",
            "/models/qwen.gguf".to_owned(),
            Some(16_384),
            LoadConsent::Unconfirmed,
        )
        .await
        .unwrap(),
    );

    // THEN the engine was launched once, under the backend's window and the
    // preset's slots
    let lines = launches(dir.path());
    assert_eq!(lines.len(), 1, "{lines:#?}");
    assert!(lines[0].contains(" -c 16384 "), "{}", lines[0]);
    assert!(lines[0].contains(" -np 2"), "{}", lines[0]);
    assert_eq!(first_window, 16_384);
    sup.shutdown_in_place().await;
}

#[cfg(unix)]
mod guardrails;
