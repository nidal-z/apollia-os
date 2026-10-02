//! The Python tools of installed packages, `pkg:<package>/<tool>`.
//!
//! A package declares its tools in the `[[tools]]` table of its
//! `apollia.toml`, written by the SDK when the package is built. Reading them
//! runs no Python: [`PackageTools::read`] takes each descriptor from the
//! manifest the package store holds, and [`PackageTools::register`] enters
//! them in the shared tool registry with their input schema and their action
//! family. The module of a tool is imported on its first call only, in the
//! embedded interpreter, with the package root and the package environment on
//! `sys.path`; see [`PackageToolExecutor`]. Once no tool of a package has been
//! called for the idle delay, the package is unloaded: its functions are
//! released and its modules leave `sys.modules`, until the next call imports
//! them again; see [`PackageTools::unload_idle`].
//!
//! A package tool and an MCP tool are the same kind of object for the agent
//! engine: one registry, one dispatcher, one access check. [`PackageTools`]
//! hands its executors to the engine through
//! [`apollia_runtime::package_tools::PackageToolHost`].

mod executor;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use apollia_core::paths::DataFile;
use apollia_core::SandboxProfile;
use apollia_runtime::chat::ChatSessionManagerHandle;
use apollia_runtime::coordinator::CancellationToken;
use apollia_runtime::package_tools::PackageToolHost;
use apollia_tools::executor::ToolExecutor;
use apollia_tools::{
    ActionFamily, FamilySource, InstalledPackage, PackageItemKind, PackageRepository,
    PackageRepositoryError, ToolDescriptor, ToolKind, ToolRegistryHandle,
};
use pyo3::prelude::*;

use crate::apollia_manifest::{self, ApolliaManifest, ToolDecl, PACKAGE_TOOL_PREFIX};

pub use executor::PackageToolExecutor;

/// One Python tool of an installed package.
pub struct PackageTool {
    package: String,
    version: String,
    root: PathBuf,
    decl: ToolDecl,
    /// The decorated function while the package is loaded, and the calls
    /// that keep it so.
    state: Mutex<ToolState>,
}

/// What one tool holds between calls.
#[derive(Default)]
struct ToolState {
    /// Set by the first call that imports the module, cleared by an unload.
    function: Option<Py<PyAny>>,
    /// Calls running now.
    in_flight: usize,
    /// When the last call started or ended.
    last_call: Option<Instant>,
}

impl PackageTool {
    fn new(package: &InstalledPackage, decl: ToolDecl) -> Self {
        Self {
            package: package.name.clone(),
            version: package.version.clone(),
            root: package.root_path.clone(),
            decl,
            state: Mutex::new(ToolState::default()),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, ToolState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The loaded function, when there is one.
    fn function(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.state().function.as_ref().map(|f| f.clone_ref(py))
    }

    /// Keeps the function a call has just imported.
    fn keep_function(&self, py: Python<'_>, function: &Py<PyAny>) {
        let mut state = self.state();
        if state.function.is_none() {
            state.function = Some(function.clone_ref(py));
        }
    }

    /// A call starts.
    fn begin_call(&self) {
        let mut state = self.state();
        state.in_flight += 1;
        state.last_call = Some(Instant::now());
    }

    /// A call ends, whatever its outcome.
    fn end_call(&self) {
        let mut state = self.state();
        state.in_flight = state.in_flight.saturating_sub(1);
        state.last_call = Some(Instant::now());
    }

    /// Whether the tool's function is loaded.
    pub fn is_loaded(&self) -> bool {
        self.state().function.is_some()
    }

    /// The tool's name in the registry, `pkg:<package>/<tool>`.
    pub fn full_name(&self) -> String {
        format!("{PACKAGE_TOOL_PREFIX}{}/{}", self.package, self.decl.name)
    }

    /// The descriptor the registry holds, read from the manifest alone.
    ///
    /// The family is the one the package declares. Like an MCP server's own
    /// hint, it classifies the tool but is not trusted to make it read-only:
    /// a package tool never runs concurrently with another call, nor before a
    /// plan is approved.
    pub fn descriptor(&self) -> ToolDescriptor {
        let family = action_family(self.decl.action);
        let reads = matches!(family, ActionFamily::Read | ActionFamily::ReadPersonal);
        let sandbox_profile = if family == ActionFamily::ExecuteCode {
            SandboxProfile::Full
        } else {
            SandboxProfile::NetworkRestricted
        };
        ToolDescriptor {
            name: self.full_name(),
            version: self.version.clone(),
            description: self.decl.description.clone(),
            kind: ToolKind::Package {
                package: self.package.clone(),
                module: self.decl.module.clone(),
            },
            input_schema: self
                .decl
                .inputs
                .clone()
                .unwrap_or_else(|| serde_json::json!({ "type": "object" })),
            output_schema: None,
            sandbox_profile,
            tags: vec!["pkg".to_string(), self.package.clone()],
            family,
            family_source: FamilySource::Package,
            is_read_only: false,
            risk_score: if reads { 1 } else { 6 },
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }
}

/// The family of the registry for the family a package declares.
fn action_family(action: apollia_manifest::ActionFamily) -> ActionFamily {
    match action {
        apollia_manifest::ActionFamily::Read => ActionFamily::Read,
        apollia_manifest::ActionFamily::ReadPersonal => ActionFamily::ReadPersonal,
        apollia_manifest::ActionFamily::WriteOrDelete => ActionFamily::WriteOrDelete,
        apollia_manifest::ActionFamily::SendExternal => ActionFamily::SendExternal,
        apollia_manifest::ActionFamily::ExecuteCode => ActionFamily::ExecuteCode,
    }
}

/// The Python tools of every installed package.
#[derive(Clone)]
pub struct PackageTools {
    tools: Vec<Arc<PackageTool>>,
    venvs: PathBuf,
}

impl PackageTools {
    /// The tools the package store records, each described by the manifest of
    /// its package. `venvs` is the directory of the package environments
    /// (`<data dir>/venvs`); a package's environment is named after it.
    ///
    /// A tool counts when the store links it to its package and the
    /// package's manifest describes it. A package whose manifest cannot be
    /// read as `apollia.toml` is skipped and reported; the others are kept.
    ///
    /// # Errors
    ///
    /// The store could not be read.
    pub fn read(repo: &PackageRepository, venvs: PathBuf) -> Result<Self, PackageRepositoryError> {
        let mut tools = Vec::new();
        for package in repo.list()? {
            let linked: BTreeSet<String> = repo
                .list_items(&package.name)?
                .into_iter()
                .filter(|item| item.kind == PackageItemKind::Tool)
                .map(|item| item.item_id)
                .collect();
            if linked.is_empty() {
                continue;
            }
            let Some(manifest) = manifest_of(&package) else {
                continue;
            };
            for decl in manifest.tools {
                if linked.contains(&decl.name) {
                    tools.push(Arc::new(PackageTool::new(&package, decl)));
                }
            }
        }
        Ok(Self { tools, venvs })
    }

    /// The names of the tools, `pkg:<package>/<tool>`.
    pub fn names(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.full_name()).collect()
    }

    /// The descriptors of the tools.
    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.tools.iter().map(|t| t.descriptor()).collect()
    }

    /// Enter every tool in `registry`. A tool the registry refuses is
    /// reported and skipped. Returns how many were entered.
    pub async fn register(&self, registry: &ToolRegistryHandle) -> usize {
        let mut registered = 0;
        for descriptor in self.descriptors() {
            let name = descriptor.name.clone();
            match registry.register(descriptor).await {
                Ok(()) => registered += 1,
                Err(error) => tracing::event!(
                    tracing::Level::WARN,
                    tool = %name,
                    error = %error,
                    "package.tool.register.failed"
                ),
            }
        }
        registered
    }

    /// Whether a tool of `package` is loaded.
    pub fn is_loaded(&self, package: &str) -> bool {
        self.tools
            .iter()
            .any(|tool| tool.package == package && tool.is_loaded())
    }

    /// Unloads every package none of whose tools has been called within
    /// `idle` of `now`, and none of whose tools is running: the functions are
    /// released and the package's modules leave `sys.modules`, so the next
    /// call imports them again. Returns the names of the packages unloaded.
    ///
    /// Takes the GIL; call it from a blocking thread.
    pub fn unload_idle(&self, now: Instant, idle: Duration) -> Vec<String> {
        let mut packages: BTreeMap<&str, Vec<&Arc<PackageTool>>> = BTreeMap::new();
        for tool in &self.tools {
            packages
                .entry(tool.package.as_str())
                .or_default()
                .push(tool);
        }
        let mut unloaded = Vec::new();
        for (package, tools) in packages {
            let states: Vec<_> = tools.iter().map(|tool| tool.state()).collect();
            let loaded = states.iter().any(|state| state.function.is_some());
            let running = states.iter().any(|state| state.in_flight > 0);
            let last_call = states.iter().filter_map(|state| state.last_call).max();
            let idle_for = last_call.map_or(Duration::MAX, |at| now.saturating_duration_since(at));
            if !loaded || running || idle_for < idle {
                continue;
            }
            let functions: Vec<Py<PyAny>> = states
                .into_iter()
                .filter_map(|mut state| state.function.take())
                .collect();
            Python::with_gil(|py| {
                drop(functions);
                for root in tools.iter().map(|tool| &tool.root).collect::<BTreeSet<_>>() {
                    forget_modules(py, &executor::module_prefix(root));
                }
            });
            tracing::event!(
                tracing::Level::INFO,
                package = %package,
                idle_secs = idle_for.as_secs(),
                "package.unloaded"
            );
            unloaded.push(package.to_string());
        }
        unloaded
    }

    fn site_packages(&self, package: &str) -> Vec<PathBuf> {
        apollia_tools::tools::python_executor::agent_venv_site_packages(&self.venvs, package)
    }
}

impl PackageToolHost for PackageTools {
    fn executors(&self, caller: &str, cancel: &CancellationToken) -> Vec<Box<dyn ToolExecutor>> {
        self.tools
            .iter()
            .map(|tool| {
                Box::new(PackageToolExecutor::new(
                    Arc::clone(tool),
                    self.site_packages(&tool.package),
                    caller,
                    cancel.clone(),
                )) as Box<dyn ToolExecutor>
            })
            .collect()
    }
}

/// How often the idle packages are looked for.
const IDLE_CHECK_PERIOD: Duration = Duration::from_secs(30);

/// Read the package tools of the store under `data_dir`, enter them in
/// `registry`, and hand them to the agent engine behind `chat`. A package
/// none of whose tools has been called for `idle` is unloaded.
///
/// Called once the runtime is up. Without a store there is nothing to read;
/// a store that cannot be read is reported and the runtime goes on without
/// package tools. Returns how many tools were entered.
pub async fn attach(
    data_dir: &Path,
    registry: &ToolRegistryHandle,
    chat: Option<&ChatSessionManagerHandle>,
    idle: Duration,
) -> usize {
    let agents_db = DataFile::Agents.path(data_dir);
    if !agents_db.exists() {
        return 0;
    }
    let tools = match PackageRepository::open(&agents_db)
        .and_then(|repo| PackageTools::read(&repo, data_dir.join("venvs")))
    {
        Ok(tools) => tools,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                error = %error,
                detail = "no package tool is available",
                "package.tools.unreadable"
            );
            return 0;
        }
    };
    let registered = tools.register(registry).await;
    if !tools.tools.is_empty() {
        tokio::spawn(unload_idle_packages(tools.clone(), idle));
    }
    if let Some(chat) = chat {
        chat.set_package_tools(Arc::new(tools)).await;
    }
    tracing::event!(tracing::Level::INFO, registered, "package.tools.attached");
    registered
}

/// Unloads the idle packages of `tools` every [`IDLE_CHECK_PERIOD`], for the
/// life of the runtime.
async fn unload_idle_packages(tools: PackageTools, idle: Duration) {
    let mut ticker = tokio::time::interval(IDLE_CHECK_PERIOD);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        let tools = tools.clone();
        let _ = tokio::task::spawn_blocking(move || tools.unload_idle(Instant::now(), idle)).await;
    }
}

/// Removes from `sys.modules` every module imported under `prefix`.
fn forget_modules(py: Python<'_>, prefix: &str) {
    let dotted = format!("{prefix}.");
    let result = py
        .import("sys")
        .and_then(|sys| sys.getattr("modules"))
        .and_then(|modules| {
            let names: Vec<String> = modules
                .call_method0("keys")?
                .try_iter()?
                .filter_map(|name| name.ok()?.extract::<String>().ok())
                .filter(|name| name == prefix || name.starts_with(&dotted))
                .collect();
            for name in names {
                modules.call_method1("pop", (name, py.None()))?;
            }
            Ok(())
        });
    if let Err(error) = result {
        tracing::event!(
            tracing::Level::WARN,
            prefix = %prefix,
            error = %error,
            "package.modules.forget.failed"
        );
    }
}

/// The `apollia.toml` manifest a package was stored with, when it is one.
fn manifest_of(package: &InstalledPackage) -> Option<ApolliaManifest> {
    let parsed = serde_json::from_str::<ApolliaManifest>(&package.manifest_json)
        .map_err(|e| e.to_string())
        .and_then(|m| m.validate().map(|()| m).map_err(|e| e.to_string()));
    match parsed {
        Ok(manifest) if manifest.package.name == package.name => Some(manifest),
        Ok(manifest) => {
            tracing::event!(
                tracing::Level::WARN,
                package = %package.name,
                declared = %manifest.package.name,
                detail = "its tools are not registered",
                "package.manifest.name_mismatch"
            );
            None
        }
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                package = %package.name,
                error = %error,
                detail = "its tools are not registered",
                "package.manifest.unreadable"
            );
            None
        }
    }
}

/// The dotted module name of `module`, a `.py` path relative to the package
/// root (`tools/extract.py` gives `tools.extract`). `None` for any path that
/// is absolute, climbs out of the root, or is not a Python file.
fn dotted_module(module: &str) -> Option<String> {
    let path = Path::new(module);
    let stem = path.to_str()?.strip_suffix(".py")?;
    let mut parts = Vec::new();
    for component in Path::new(stem).components() {
        match component {
            std::path::Component::Normal(part) => parts.push(part.to_str()?.to_string()),
            _ => return None,
        }
    }
    if parts.last().is_some_and(|last| last == "__init__") {
        parts.pop();
    }
    (!parts.is_empty()).then(|| parts.join("."))
}

#[cfg(test)]
mod tests;
