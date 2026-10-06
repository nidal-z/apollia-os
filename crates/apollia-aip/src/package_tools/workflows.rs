//! The Python workflows of installed packages, `<package>/<workflow>`.
//!
//! A package declares its workflows in the `[[workflows]]` table of its
//! `apollia.toml`, with their steps and their input schema, written by the SDK
//! when the package is built. [`PackageWorkflows::read`] takes them from the
//! manifests the package store holds, running no Python; the module of a
//! workflow is imported on its first run, the way a package tool's is.
//!
//! A run calls the decorated function with a `ctx` that adds four things to
//! the context a package tool gets: `ctx.agents.run`, `ctx.workflows.run`,
//! `ctx.user.ask`, which stops the run on a question until the user answers,
//! and the private hook the SDK calls each time a step opens or closes. Each goes
//! back to the workflow engine of the runtime through
//! [`WorkflowServices`]. Cancelling the run's token cancels the coroutine at
//! its next await point, inside whatever step it is in.

use std::path::PathBuf;
use std::sync::Arc;

use apollia_core::paths::DataFile;
use apollia_runtime::workflows::{
    BoxFuture, QuestionAsked, UserReply, WorkflowCall, WorkflowDescriptor, WorkflowHandle,
    WorkflowHost, WorkflowServices,
};
use apollia_tools::{PackageItemKind, PackageRepository, PackageRepositoryError};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{PyDict, PyModule};
use serde_json::Value;

use super::executor::{helpers, module_name};
use super::{dotted_module, manifest_of};
use crate::apollia_manifest::WorkflowDecl;
use crate::cancel;
use crate::context::RuntimeContext;

/// The `ctx` of a workflow, built around the runtime context. `agents`,
/// `workflows` and `user` reach the engine; `_apollia_step_hook` is what the SDK's
/// `ctx.step` reports through, and is absent from any other context, so a
/// workflow run under a test double reports nothing.
///
/// A workflow receives a date input as a `datetime.date`, so the inputs it
/// hands to another workflow and the value it returns may hold one: `to_json`
/// writes it back as `YYYY-MM-DD`, the form the runtime reads.
const WORKFLOW_CODE: &std::ffi::CStr = c"import datetime
import json


def _plain(value):
    if isinstance(value, datetime.date):
        return value.isoformat()
    raise TypeError(f'{type(value).__name__} is not JSON serializable')


def to_json(value):
    return json.dumps(value, default=_plain)


class _Agents:
    def __init__(self, services):
        self._services = services

    async def run(self, agent_id, message):
        return await self._services.run_agent(agent_id, message)


class _Workflows:
    def __init__(self, services):
        self._services = services

    async def run(self, workflow_id, inputs):
        answer = await self._services.run_workflow(workflow_id, to_json(inputs))
        return json.loads(answer)


class _User:
    def __init__(self, services):
        self._services = services

    async def ask(self, question, *, options=None, hint=None):
        from apollia.errors import PayloadError, QuestionDeclined

        if not isinstance(question, str):
            raise PayloadError('ctx.user.ask: the question must be text', field='question')
        if isinstance(options, str) or not all(isinstance(o, str) for o in options or []):
            raise PayloadError('ctx.user.ask: options must be a list of text', field='options')
        if hint is not None and not isinstance(hint, str):
            raise PayloadError('ctx.user.ask: the hint must be text', field='hint')
        reply = json.loads(await self._services.ask_user(question, list(options or []), hint))
        if reply['declined'] is not None:
            raise QuestionDeclined(question, reason=reply['declined'])
        return reply['answer']


class WorkflowContext:
    def __init__(self, inner, services):
        self._apollia_inner = inner
        self._apollia_services = services
        self.agents = _Agents(services)
        self.workflows = _Workflows(services)
        self.user = _User(services)

    def __getattr__(self, name):
        return getattr(self.__dict__['_apollia_inner'], name)

    async def _apollia_step_hook(self, event, name, error=None):
        await self._apollia_services.step(event, name, error)


async def call_workflow(fn, inner, services, inputs):
    return await fn(WorkflowContext(inner, services), inputs)
";

static WORKFLOW_HELPERS: GILOnceCell<Py<PyModule>> = GILOnceCell::new();

fn workflow_helpers(py: Python<'_>) -> PyResult<&Bound<'_, PyModule>> {
    Ok(WORKFLOW_HELPERS
        .get_or_try_init(py, || -> PyResult<Py<PyModule>> {
            Ok(PyModule::from_code(
                py,
                WORKFLOW_CODE,
                c"apollia_package_workflows.py",
                c"apollia_package_workflows",
            )?
            .unbind())
        })?
        .bind(py))
}

/// The engine's services, as the Python `ctx` of a workflow reaches them.
#[pyclass(name = "WorkflowServices", frozen)]
struct PyWorkflowServices {
    services: Arc<dyn WorkflowServices>,
}

#[pymethods]
impl PyWorkflowServices {
    /// Report that the step `name` `started`, `completed` or `failed`.
    #[pyo3(signature = (event, name, error=None))]
    fn step<'py>(
        &self,
        py: Python<'py>,
        event: String,
        name: String,
        error: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let services = Arc::clone(&self.services);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let reported = match event.as_str() {
                "started" => services.step_started(name).await,
                "completed" => services.step_ended(name, None).await,
                "failed" => {
                    let error = error.unwrap_or_else(|| "the step failed".to_string());
                    services.step_ended(name, Some(error)).await
                }
                other => Err(format!("unknown step event '{other}'")),
            };
            reported.map_err(PyRuntimeError::new_err)
        })
    }

    /// Run an agent on a message; resolves to its answer.
    fn run_agent<'py>(
        &self,
        py: Python<'py>,
        agent_id: String,
        message: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let services = Arc::clone(&self.services);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            services
                .run_agent(agent_id, message)
                .await
                .map_err(PyRuntimeError::new_err)
        })
    }

    /// Run a workflow on inputs given as JSON; resolves to what it returned,
    /// as JSON.
    fn run_workflow<'py>(
        &self,
        py: Python<'py>,
        workflow_id: String,
        inputs: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        let services = Arc::clone(&self.services);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let inputs: Value = serde_json::from_str(&inputs)
                .map_err(|e| PyRuntimeError::new_err(format!("inputs: {e}")))?;
            services
                .run_workflow(workflow_id, inputs)
                .await
                .map(|output| output.to_string())
                .map_err(PyRuntimeError::new_err)
        })
    }

    /// Ask the user a question; resolves, once the user replied, to a JSON
    /// object holding `answer`, or `declined` with the reason given.
    #[pyo3(signature = (question, options, hint=None))]
    fn ask_user<'py>(
        &self,
        py: Python<'py>,
        question: String,
        options: Vec<String>,
        hint: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let services = Arc::clone(&self.services);
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let reply = services
                .ask_user(QuestionAsked {
                    question,
                    options,
                    hint,
                })
                .await
                .map_err(PyRuntimeError::new_err)?;
            let reply = match reply {
                UserReply::Answer(answer) => {
                    serde_json::json!({"answer": answer, "declined": null})
                }
                UserReply::Declined(reason) => {
                    serde_json::json!({"answer": null, "declined": reason})
                }
            };
            Ok(reply.to_string())
        })
    }
}

/// One Python workflow of an installed package.
pub struct PackageWorkflow {
    package: String,
    version: String,
    root: PathBuf,
    decl: WorkflowDecl,
    /// The decorated function, set by the first run that imports it.
    function: GILOnceCell<Py<PyAny>>,
}

impl PackageWorkflow {
    /// The workflow's identifier, `<package>/<workflow>`.
    pub fn id(&self) -> String {
        format!("{}/{}", self.package, self.decl.name)
    }

    fn descriptor(&self) -> WorkflowDescriptor {
        WorkflowDescriptor {
            id: self.id(),
            version: self.version.clone(),
            description: self.decl.description.clone(),
            steps: self.decl.steps.clone(),
            input_schema: self.decl.inputs.clone(),
        }
    }
}

/// The Python workflows of every installed package.
#[derive(Clone)]
pub struct PackageWorkflows {
    workflows: Vec<Arc<PackageWorkflow>>,
    venvs: PathBuf,
}

impl PackageWorkflows {
    /// The workflows the package store records, each described by the
    /// manifest of its package. `venvs` is the directory of the package
    /// environments (`<data dir>/venvs`).
    ///
    /// A workflow counts when the store links it to its package and the
    /// package's manifest describes it.
    ///
    /// # Errors
    ///
    /// The store could not be read.
    pub fn read(repo: &PackageRepository, venvs: PathBuf) -> Result<Self, PackageRepositoryError> {
        let mut workflows = Vec::new();
        for package in repo.list()? {
            let linked: Vec<String> = repo
                .list_items(&package.name)?
                .into_iter()
                .filter(|item| item.kind == PackageItemKind::Workflow)
                .map(|item| item.item_id)
                .collect();
            if linked.is_empty() {
                continue;
            }
            let Some(manifest) = manifest_of(&package) else {
                continue;
            };
            for decl in manifest.workflows {
                if linked.contains(&decl.name) {
                    workflows.push(Arc::new(PackageWorkflow {
                        package: package.name.clone(),
                        version: package.version.clone(),
                        root: package.root_path.clone(),
                        decl,
                        function: GILOnceCell::new(),
                    }));
                }
            }
        }
        Ok(Self { workflows, venvs })
    }

    /// The identifiers of the workflows, `<package>/<workflow>`.
    pub fn ids(&self) -> Vec<String> {
        self.workflows.iter().map(|w| w.id()).collect()
    }

    fn find(&self, workflow_id: &str) -> Option<&Arc<PackageWorkflow>> {
        self.workflows.iter().find(|w| w.id() == workflow_id)
    }
}

impl WorkflowHost for PackageWorkflows {
    fn describe(&self, workflow_id: &str) -> Option<WorkflowDescriptor> {
        self.find(workflow_id).map(|w| w.descriptor())
    }

    fn list(&self) -> Vec<WorkflowDescriptor> {
        self.workflows.iter().map(|w| w.descriptor()).collect()
    }

    fn run(&self, call: WorkflowCall) -> BoxFuture<'static, Result<Value, String>> {
        let Some(workflow) = self.find(&call.workflow.id).cloned() else {
            let id = call.workflow.id;
            return Box::pin(async move { Err(format!("no installed workflow '{id}'")) });
        };
        let site_packages = apollia_tools::tools::python_executor::agent_venv_site_packages(
            &self.venvs,
            &workflow.package,
        );
        Box::pin(run(workflow, site_packages, call))
    }
}

/// Run `call` on a blocking thread, under `asyncio.run`, until it ends or its
/// token is cancelled.
async fn run(
    workflow: Arc<PackageWorkflow>,
    site_packages: Vec<PathBuf>,
    call: WorkflowCall,
) -> Result<Value, String> {
    let WorkflowCall {
        run_id,
        workflow: descriptor,
        inputs,
        cancel: token,
        services,
    } = call;
    let (slot, slot_for_cancel, slot_for_drop) = Python::with_gil(|py| {
        let slot: Py<PyDict> = PyDict::new(py).unbind();
        let for_cancel = slot.clone_ref(py);
        let for_drop = slot.clone_ref(py);
        (slot, for_cancel, for_drop)
    });
    let mut guard = cancel::CancelOnDrop::new(slot_for_drop);
    let id = descriptor.id.clone();
    let inputs = inputs.to_string();
    let mut blocking = tokio::task::spawn_blocking(move || {
        Python::with_gil(|py| {
            let services = PythonServices { services, inputs };
            run_in_python(py, &workflow, &site_packages, services, &slot)
        })
    });
    let joined = tokio::select! {
        joined = &mut blocking => joined,
        () = token.cancelled() => {
            guard.disarm();
            cancel::request(&slot_for_cancel);
            let _ = blocking.await;
            tracing::event!(
                tracing::Level::INFO,
                run_id = %run_id,
                workflow = %id,
                "package.workflow.canceled"
            );
            return Err(format!("{id}: the run was stopped"));
        }
    };
    guard.disarm();
    let text = joined.map_err(|e| format!("{id}: {e}"))??;
    serde_json::from_str(&text)
        .map_err(|e| format!("{id}: the workflow returned no JSON value: {e}"))
}

/// What a run hands its code: the engine's services and the inputs as JSON.
struct PythonServices {
    services: Arc<dyn WorkflowServices>,
    inputs: String,
}

/// Import the workflow if needed, call it, and return the JSON text of what
/// it returned.
fn run_in_python(
    py: Python<'_>,
    workflow: &PackageWorkflow,
    site_packages: &[PathBuf],
    call: PythonServices,
    slot: &Py<PyDict>,
) -> Result<String, String> {
    let PythonServices { services, inputs } = call;
    let id = workflow.id();
    let fail = |e: PyErr| format!("{id}: {e}");
    let function = workflow
        .function
        .get_or_try_init(py, || load(py, workflow, site_packages))
        .map_err(|e| format!("{id}: cannot load the workflow: {e}"))?
        .bind(py);
    let inner = Py::new(py, RuntimeContext::for_package_tool(&id)).map_err(fail)?;
    let services = Py::new(py, PyWorkflowServices { services }).map_err(fail)?;
    let json = py.import("json").map_err(fail)?;
    let inputs = json
        .call_method1("loads", (inputs.as_str(),))
        .map_err(fail)?;
    let coroutine = workflow_helpers(py)
        .and_then(|h| h.call_method1("call_workflow", (function, inner, services, inputs)))
        .and_then(|coroutine| cancel::wrap(py, &coroutine, slot))
        .map_err(fail)?;
    let result = py
        .import("asyncio")
        .and_then(|asyncio| asyncio.call_method1("run", (coroutine,)))
        .map_err(fail)?;
    workflow_helpers(py)
        .and_then(|h| h.call_method1("to_json", (result,)))
        .and_then(|text| text.extract::<String>())
        .map_err(|e| format!("{id}: the workflow returned no JSON value: {e}"))
}

/// Import the module of `workflow` and return its decorated function.
fn load(
    py: Python<'_>,
    workflow: &PackageWorkflow,
    site_packages: &[PathBuf],
) -> Result<Py<PyAny>, String> {
    let dotted = dotted_module(&workflow.decl.module).ok_or_else(|| {
        format!(
            "module `{}` is not a Python file inside the package",
            workflow.decl.module
        )
    })?;
    let unique = module_name(&workflow.root, &dotted);
    let site_packages: Vec<String> = site_packages
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let function = helpers(py)
        .and_then(|h| {
            h.call_method1(
                "load_decorated",
                (
                    workflow.root.to_string_lossy().into_owned(),
                    site_packages,
                    workflow.decl.module.as_str(),
                    unique.as_str(),
                    workflow.decl.name.as_str(),
                    "__apollia_workflow__",
                    "workflow",
                ),
            )
        })
        .map_err(|e| e.to_string())?;
    tracing::event!(
        tracing::Level::INFO,
        workflow = %workflow.id(),
        module = %unique,
        "package.workflow.loaded"
    );
    Ok(function.unbind())
}

/// Read the package workflows of the store under `data_dir` and hand them to
/// the workflow engine.
///
/// Called once the runtime is up. Without a store or an engine there is
/// nothing to attach; a store that cannot be read is reported and the engine
/// is attached with no workflow. Returns how many workflows were attached.
pub async fn attach(data_dir: &std::path::Path, engine: Option<&WorkflowHandle>) -> usize {
    let Some(engine) = engine else {
        return 0;
    };
    let agents_db = DataFile::Agents.path(data_dir);
    let workflows = if agents_db.exists() {
        match PackageRepository::open(&agents_db)
            .and_then(|repo| PackageWorkflows::read(&repo, data_dir.join("venvs")))
        {
            Ok(workflows) => workflows,
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %error,
                    detail = "no package workflow is available",
                    "package.workflows.unreadable"
                );
                PackageWorkflows {
                    workflows: Vec::new(),
                    venvs: data_dir.join("venvs"),
                }
            }
        }
    } else {
        PackageWorkflows {
            workflows: Vec::new(),
            venvs: data_dir.join("venvs"),
        }
    };
    let count = workflows.workflows.len();
    if let Err(error) = engine.set_host(Arc::new(workflows)).await {
        tracing::event!(
            tracing::Level::WARN,
            error = %error,
            "package.workflows.attach.failed"
        );
        return 0;
    }
    tracing::event!(tracing::Level::INFO, count, "package.workflows.attached");
    count
}
