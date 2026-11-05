//! The engine choice of the settings is read again at every launch.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::LaunchBinary;
use crate::engine_store::selection::resolve;
use crate::engine_store::{package_dir, Engine, InstalledPackage, Source, Variant, RECORD_FILE};
use crate::llama_server::locate::BIN_OVERRIDE_LOCK;
use crate::llama_server::test_support::supervisor_with_ceiling;
use crate::local_engine::LoadConsent;
use apollia_core::config::EngineChoice;

/// Installs under `root` a verified build of `variant` whose binary is a
/// stand-in: it appends each command line to `args.log` beside it, then
/// answers every request with 200 on the port it was given.
fn install_stand_in(root: &Path, variant: Variant) -> PathBuf {
    let dir = package_dir(root, Engine::LlamaServer, "b9000", variant);
    std::fs::create_dir_all(&dir).unwrap();
    let record = InstalledPackage {
        engine: Engine::LlamaServer,
        tag: "b9000".to_owned(),
        variant,
        platform: "test".to_owned(),
        sha256: "0".repeat(64),
        source: Source::Imported,
        released: None,
        path: PathBuf::new(),
    };
    std::fs::write(dir.join(RECORD_FILE), serde_json::to_vec(&record).unwrap()).unwrap();
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
import http.server, socketserver, sys
class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"{{}}")
    def log_message(self, *args):
        pass
class Quick(http.server.HTTPServer):
    def server_bind(self):
        socketserver.TCPServer.server_bind(self)
        self.server_name, self.server_port = self.server_address[:2]
Quick(("127.0.0.1", int(sys.argv[1])), H).serve_forever()
' "$port"
"#,
        log = log.display()
    );
    let bin = dir.join("llama-server");
    std::fs::write(&bin, script).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// The launches a stand-in recorded: the lines naming a port.
fn launches(bin: &Path) -> Vec<String> {
    std::fs::read_to_string(bin.with_file_name("args.log"))
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("--port"))
        .map(str::to_owned)
        .collect()
}

fn choose(file: &Path, engine: &str) {
    std::fs::write(file, format!("[engines]\ngguf = \"{engine}\"\n")).unwrap();
}

/// The window a launch line passes with `-c`.
fn window_of(line: &str) -> Option<u32> {
    let mut words = line.split_whitespace();
    words.find(|w| *w == "-c")?;
    words.next()?.parse().ok()
}

#[tokio::test]
async fn test_a_changed_engine_serves_the_next_launch_without_a_new_supervisor() {
    // GIVEN a CPU and a Vulkan build in the store, settings choosing the CPU
    //       build, and one supervisor reading those settings, no binary
    //       override posed by another test
    let _override = BIN_OVERRIDE_LOCK.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("engines");
    let cpu = install_stand_in(&root, Variant::Cpu);
    let vulkan = install_stand_in(&root, Variant::Vulkan);
    let file = dir.path().join("apollia.toml");
    choose(&file, "cpu");
    let mut sup = supervisor_with_ceiling(1);
    sup.binary = LaunchBinary::FromSettings {
        store_root: Some(root.clone()),
    };
    sup.config_file = Some(file.clone());

    // WHEN a model is loaded, the settings then choose Vulkan, and a second
    //      model is loaded by the same supervisor
    let first = sup
        .ensure_model(
            "local",
            "/models/a.gguf".to_owned(),
            None,
            LoadConsent::Unconfirmed,
        )
        .await
        .unwrap();
    drop(first);
    choose(&file, "vulkan");
    let second = sup
        .ensure_model(
            "local",
            "/models/b.gguf".to_owned(),
            None,
            LoadConsent::Unconfirmed,
        )
        .await
        .unwrap();
    let reported = second.n_ctx();
    drop(second);
    sup.shutdown_in_place().await;

    // THEN the first launch ran the CPU build and the second the Vulkan one,
    //      and the window reported for the second is the one its process got
    let (on_cpu, on_vulkan) = (launches(&cpu), launches(&vulkan));
    assert_eq!(on_cpu.len(), 1, "{on_cpu:#?}");
    assert!(on_cpu[0].contains("/models/a.gguf"), "{}", on_cpu[0]);
    assert_eq!(on_vulkan.len(), 1, "{on_vulkan:#?}");
    assert!(on_vulkan[0].contains("/models/b.gguf"), "{}", on_vulkan[0]);
    assert_eq!(window_of(&on_vulkan[0]), Some(reported), "{}", on_vulkan[0]);
}

#[test]
fn test_a_chosen_variant_since_removed_launches_the_automatic_choice_and_says_so() {
    // GIVEN settings choosing the Vulkan build, which the store holds, and no
    //       binary override posed by another test
    let _override = BIN_OVERRIDE_LOCK.blocking_lock();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("engines");
    let vulkan = install_stand_in(&root, Variant::Vulkan);
    let file = dir.path().join("apollia.toml");
    choose(&file, "vulkan");
    let mut sup = supervisor_with_ceiling(1);
    sup.binary = LaunchBinary::FromSettings {
        store_root: Some(root.clone()),
    };
    sup.config_file = Some(file);
    let before = sup.launch_binary().unwrap();

    // WHEN the Vulkan build is removed and the next launch resolves its binary
    std::fs::remove_dir_all(vulkan.parent().unwrap()).unwrap();
    let after = sup.launch_binary().ok();
    let resolution = resolve(Some(&root), Engine::LlamaServer, EngineChoice::Vulkan);

    // THEN the launch before took the Vulkan build, the one after does not,
    //      and the store reports the fall back to the automatic choice
    assert_eq!(before, vulkan);
    assert_ne!(after, Some(vulkan));
    assert!(resolution.fell_back);
}
