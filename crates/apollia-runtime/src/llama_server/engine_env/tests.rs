//! A launched engine never inherits a `LLAMA_ARG_*` variable.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::ffi::OsString;
use std::path::Path;
use std::sync::{Arc, Mutex};

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

use super::engine_command;
use crate::llama_server::test_support::supervisor_with_ceiling;
use apollia_llm::engine_env::engine_flag_names;

/// Serialises the tests that write the environment of this process.
static LOCK: Mutex<()> = Mutex::new(());

/// The variables these tests pose in the environment of the test process.
const POSED: [(&str, &str); 3] = [
    ("LLAMA_ARG_CONNECT", "1"),
    ("LLAMA_ARG_CTX_SIZE", "1"),
    ("APOLLIA_ENGINE_ENV_PROBE", "kept"),
];

/// Poses [`POSED`] for the life of the value, and removes them on drop.
struct Posed;

impl Posed {
    fn new() -> Self {
        for (name, value) in POSED {
            std::env::set_var(name, value);
        }
        Self
    }
}

impl Drop for Posed {
    fn drop(&mut self) {
        for (name, _) in POSED {
            std::env::remove_var(name);
        }
    }
}

/// The fields of one event, rendered.
type Seen = Vec<(String, String)>;

/// The fields of every event a subscriber receives.
struct Recorder(Arc<Mutex<Vec<Seen>>>);

struct Fields(Vec<(String, String)>);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.push((field.name().to_owned(), format!("{value:?}")));
    }
}

impl Subscriber for Recorder {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, event: &Event<'_>) {
        let mut fields = Fields(Vec::new());
        event.record(&mut fields);
        self.0.lock().unwrap().push(fields.0);
    }
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
}

fn field<'a>(fields: &'a [(String, String)], name: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
}

#[cfg(unix)]
#[test]
fn test_a_launched_engine_receives_no_llama_arg_variable_and_the_rest_of_the_environment() {
    // GIVEN two LLAMA_ARG_* variables and one other variable in the test
    //       process, and an engine that writes its environment and its
    //       arguments to files
    let _lock = LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _posed = Posed::new();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("env.txt");
    let script = r#"out="$1"; shift; env > "$out"; printf '%s\n' "$@" > "$out.args""#;
    let args = [
        "-c".to_owned(),
        script.to_owned(),
        "engine".to_owned(),
        out.display().to_string(),
        "--connect".to_owned(),
    ];
    let sup = supervisor_with_ceiling(1);

    // WHEN the supervisor launches it, through the path the server and the
    //      embeddings instance both take
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let status = runtime.block_on(async {
        let mut child = sup.spawn_child(Path::new("/bin/sh"), &args).unwrap();
        child.wait().await.unwrap()
    });

    // THEN no LLAMA_ARG_* variable reached it, the other variable did, and
    //      an argument passed explicitly still did
    assert!(status.success(), "{status}");
    let env = std::fs::read_to_string(&out).unwrap();
    let leaked: Vec<&str> = env
        .lines()
        .filter(|line| line.starts_with("LLAMA_ARG_"))
        .collect();
    assert!(leaked.is_empty(), "{leaked:?}");
    assert!(
        env.lines()
            .any(|line| line == "APOLLIA_ENGINE_ENV_PROBE=kept"),
        "{env}"
    );
    let passed = std::fs::read_to_string(dir.path().join("env.txt.args")).unwrap();
    assert_eq!(passed.lines().collect::<Vec<_>>(), ["--connect"]);
}

#[test]
fn test_the_launch_logs_the_count_and_names_removed_never_their_values() {
    // GIVEN two LLAMA_ARG_* variables in the test process and a recorder of
    //       the events
    let _lock = LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _posed = Posed::new();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorder = Recorder(Arc::clone(&seen));

    // WHEN a command for the engine is built
    {
        let _guard = crate::test_support::recording(recorder);
        let _command = engine_command(Path::new("/nonexistent/llama-server"));
    }

    // THEN one event gives the count and the names, and no value is in it
    let seen = seen.lock().unwrap();
    let removed: Vec<&Seen> = seen
        .iter()
        .filter(|e| field(e, "message") == Some("llama.server.env.engine_flags_removed"))
        .collect();
    assert_eq!(removed.len(), 1, "{seen:?}");
    let event = removed[0];
    assert_eq!(field(event, "count"), Some("2"), "{event:?}");
    assert_eq!(
        field(event, "names"),
        Some("LLAMA_ARG_CONNECT,LLAMA_ARG_CTX_SIZE"),
        "{event:?}"
    );
    assert_eq!(
        event.len(),
        3,
        "count, names and the message only: {event:?}"
    );
}

#[test]
fn test_only_names_with_the_engine_prefix_are_selected() {
    // GIVEN variable names with and without the prefix
    let names = [
        "PATH",
        "LLAMA_ARG_PORT",
        "LLAMA_ARGS",
        "APOLLIA_LLAMA_EXTRA_ARGS",
        "LLAMA_ARG_CONNECT",
    ]
    .map(OsString::from);

    // WHEN the engine flags are picked out
    let picked = engine_flag_names(names.into_iter());

    // THEN only the two prefixed names are, sorted
    assert_eq!(
        picked,
        [
            OsString::from("LLAMA_ARG_CONNECT"),
            OsString::from("LLAMA_ARG_PORT")
        ]
    );
}

#[cfg(unix)]
#[test]
fn test_an_inherited_engine_flag_does_not_reach_the_version_banner() {
    // GIVEN an engine that refuses to run when any LLAMA_ARG_* is in its
    //       environment, as a build does on an invalid one, and two such
    //       variables in the test process, posed under the environment lock
    let _lock = LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _posed = Posed::new();
    let dir = tempfile::tempdir().unwrap();
    let engine = dir.path().join("llama-server");
    std::fs::write(
        &engine,
        "#!/bin/sh\n\
         if env | grep -q '^LLAMA_ARG_'; then echo 'invalid argument' >&2; exit 1; fi\n\
         echo 'version: 12064 (abcdef0)'\n",
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&engine, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    // WHEN its banner is read
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let version = runtime.block_on(crate::engine_store::version::probe_banner(&engine));

    // THEN the variables did not reach it, and its version is read rather
    //      than the banner coming back empty
    assert_eq!(version.as_deref(), Some("12064 (abcdef0)"));
}
