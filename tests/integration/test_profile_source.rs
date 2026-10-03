//! Integration test: a Python agent reads `ctx.profile` only when its profile
//! source is checked.
//!
//! The source resolves from the package default of the agent's `sources`,
//! then the user's override in `agents.db`. Unchecked, the context carries no
//! profile and `ctx.profile` is `None`; checked, it serves the profile
//! designated for the agent, else the default one.
//!
//! Requires the Apollia SDK importable by the interpreter PYO3_PYTHON points
//! to. Gated behind the `python-tests` feature:
//!
//!   PYO3_PYTHON="$(command -v python3)" \
//!   cargo test -p apollia-e2e-tests --features python-tests --test test_profile_source

use std::io::Write as _;
use std::path::Path;

use apollia_aip::context::RuntimeContext;
use apollia_aip::profile::ProfileInterface;
use apollia_aip::{bridge::AIPBridge, loader::load_agent_module, validator::validate_agent};
use apollia_core::paths::DataFile;
use apollia_core::AIPTask;
use apollia_memory::user_memory::{UserMemoryRepository, WrittenBy};
use apollia_tools::{AgentOrigin, AgentRepository, OverrideField, StoredAgent};
use pyo3::prelude::*;

/// An agent that writes down what `ctx.profile` gave it: `none` without a
/// profile, else the role of the profile it was served.
const READING_AGENT: &str = r#"
import pathlib

from apollia import agent, on_message

MARKER = pathlib.Path(__MARKER__)


@agent(name="cours", version="0.1.0", description="Reads the role of the user.")
class Cours:
    @on_message
    async def handle(self, message, history, ctx):
        if ctx.profile is None:
            MARKER.write_text("none")
        else:
            MARKER.write_text(await ctx.profile.get("role") or "")
        return "done"
"#;

/// Stores the agent `cours` in the `agents.db` of `data_dir`, its package
/// declaring `sources`.
fn store_cours(data_dir: &Path, sources: &[&str]) -> AgentRepository {
    let repo = AgentRepository::open(&DataFile::Agents.path(data_dir)).expect("agents.db opens");
    repo.save_definition(&StoredAgent {
        name: "cours".to_owned(),
        origin: AgentOrigin::Package,
        package: Some("formation".to_owned()),
        version: "0.1.0".to_owned(),
        definition: serde_json::json!({
            "id": "cours",
            "name": "cours",
            "mission": "",
            "defaults": { "sources": sources },
        }),
        enabled: true,
        installed_at: "2026-10-03T00:00:00Z".to_owned(),
        updated_at: "2026-10-03T00:00:00Z".to_owned(),
    })
    .expect("agent stored");
    repo
}

/// Runs [`READING_AGENT`] once against the stores of `data_dir` and returns
/// what it wrote down.
async fn run_cours(data_dir: &Path) -> String {
    pyo3::prepare_freethreaded_python();
    let marker = data_dir.join("marker.txt");
    let source = READING_AGENT.replace("__MARKER__", &format!("{:?}", marker.to_string_lossy()));
    // One module name per data directory: the interpreter is shared by every
    // test of this binary, and a module already imported keeps its MARKER.
    let stem: String = data_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    let agent_path = data_dir.join(format!("cours_{stem}.py"));
    std::fs::File::create(&agent_path)
        .and_then(|mut f| f.write_all(source.as_bytes()))
        .expect("write the agent");
    let agent_obj = load_agent_module(&agent_path).expect("the agent loads");
    let validated = validate_agent(&agent_obj).expect("the agent validates");
    let bridge = AIPBridge::new(validated).expect("bridge");
    let profile = ProfileInterface::new(
        DataFile::UserMemory.path(data_dir),
        "cours".to_owned(),
        false,
        false,
    );
    let ctx: PyObject = Python::with_gil(|py| {
        let ctx = RuntimeContext::for_package_tool("cours").with_profile(profile);
        Py::new(py, ctx).expect("ctx").into_any()
    });
    let result = bridge
        .call_run(&AIPTask::default(), ctx)
        .await
        .expect("the agent runs");
    std::fs::read_to_string(&marker)
        .unwrap_or_else(|e| panic!("the agent wrote no marker ({e}): {result:?}"))
}

/// A default profile whose role is "Responsable SI", and a "Formateur"
/// profile; returns the identifier of the latter.
fn two_profiles(data_dir: &Path) -> String {
    let repo = UserMemoryRepository::new(&DataFile::UserMemory.path(data_dir))
        .expect("user_memory.db opens");
    repo.set("role", "Responsable SI", WrittenBy::User)
        .expect("default role");
    let formateur = repo.create_profile("Formateur").expect("created");
    repo.set_profile_field(&formateur.id, "role", Some("Formateur"))
        .expect("formateur role");
    formateur.id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_whose_profile_source_is_unchecked_sees_no_profile() {
    // GIVEN a profile, and the agent stored with no profile source
    let dir = tempfile::tempdir().expect("temp dir");
    two_profiles(dir.path());
    store_cours(dir.path(), &["crm"]);

    // WHEN the agent runs and reads ctx.profile
    let seen = run_cours(dir.path()).await;

    // THEN ctx.profile is None
    assert_eq!(seen, "none");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_whose_profile_source_is_checked_reads_its_designated_profile_else_the_default() {
    // GIVEN two profiles, and the agent whose package declares the profile source
    let dir = tempfile::tempdir().expect("temp dir");
    let formateur = two_profiles(dir.path());
    let agents = store_cours(dir.path(), &["profile"]);

    // WHEN the agent runs without a designation, then designated "Formateur"
    let undesignated = run_cours(dir.path()).await;
    agents
        .set_override(
            "cours",
            OverrideField::Profile,
            &serde_json::json!(formateur),
        )
        .expect("designation set");
    let designated = run_cours(dir.path()).await;

    // THEN it reads the default profile first, then the designated one
    assert_eq!(undesignated, "Responsable SI");
    assert_eq!(designated, "Formateur");
}
