//! Running one package tool in the embedded interpreter.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use apollia_runtime::coordinator::CancellationToken;
use apollia_tools::executor::{ToolExecutionError, ToolExecutor};
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::{PyDict, PyModule};
use serde_json::Value;

use super::{dotted_module, PackageTool};
use crate::bridge::cancel;
use crate::context::RuntimeContext;

/// The two Python helpers: importing the module and finding the decorated
/// function, then calling it with `ctx` bound to the parameter that takes it.
///
/// The module is imported from its file under a name proper to its package
/// root, so two packages that both ship `tools/calc.py` never share a module.
/// The package root stays on `sys.path`, behind nothing, for the absolute
/// imports the module makes of its own package.
///
/// `ctx` goes to the parameters the SDK leaves out of the input schema: the one
/// named `ctx`, or the one annotated `Ctx`. Every other argument is passed by
/// name, as the schema names it.
const HELPERS_CODE: &std::ffi::CStr = c"import importlib.util
import inspect
import os
import sys


def load_tool(root, site_packages, relative, unique, name):
    for path in reversed(site_packages):
        if path not in sys.path:
            sys.path.insert(0, path)
    if root in sys.path:
        sys.path.remove(root)
    sys.path.insert(0, root)
    module = sys.modules.get(unique)
    if module is None:
        spec = importlib.util.spec_from_file_location(unique, os.path.join(root, relative))
        module = importlib.util.module_from_spec(spec)
        sys.modules[unique] = module
        try:
            spec.loader.exec_module(module)
        except BaseException:
            sys.modules.pop(unique, None)
            raise
    for value in list(vars(module).values()):
        spec = getattr(value, '__apollia_tool__', None)
        if spec is None or getattr(spec, 'name', None) != name:
            continue
        if getattr(value, '__module__', None) == module.__name__:
            return value
    raise LookupError(f'no @tool named {name!r} in {relative!r}')


async def call_tool(fn, ctx, arguments):
    kwargs = dict(arguments)
    for param in inspect.signature(fn).parameters.values():
        annotation = param.annotation
        label = annotation if isinstance(annotation, str) else getattr(annotation, '__name__', None)
        if param.name == 'ctx' or label == 'Ctx':
            kwargs[param.name] = ctx
    return await fn(**kwargs)
";

static HELPERS: GILOnceCell<Py<PyModule>> = GILOnceCell::new();

/// Executes one package tool on behalf of one agent.
///
/// The first call of the tool, whoever makes it, imports its module; later
/// calls reuse the function. Each call runs the coroutine under `asyncio.run`
/// on a blocking thread, with a `ctx` naming the calling agent. Cancelling the
/// run's token, or dropping the call, cancels the coroutine at its next await
/// point, and the call returns once the thread has given control back.
pub struct PackageToolExecutor {
    tool: Arc<PackageTool>,
    name: String,
    site_packages: Vec<PathBuf>,
    caller: String,
    cancel: CancellationToken,
}

impl PackageToolExecutor {
    /// An executor of `tool` for the agent `caller`, with `site_packages` of
    /// the package environment on `sys.path`, stopped by `cancel`.
    pub fn new(
        tool: Arc<PackageTool>,
        site_packages: Vec<PathBuf>,
        caller: &str,
        cancel: CancellationToken,
    ) -> Self {
        let name = tool.full_name();
        Self {
            tool,
            name,
            site_packages,
            caller: caller.to_string(),
            cancel,
        }
    }

    async fn run(&self, input: Value) -> Result<Value, ToolExecutionError> {
        if !input.is_object() {
            return Err(ToolExecutionError::InvalidInput {
                message: format!("{}: the arguments must be an object", self.name),
            });
        }
        let call = Call {
            tool: Arc::clone(&self.tool),
            site_packages: self.site_packages.clone(),
            caller: self.caller.clone(),
            arguments: input.to_string(),
        };
        let (slot, slot_for_cancel, slot_for_drop) = Python::with_gil(|py| {
            let slot: Py<PyDict> = PyDict::new(py).unbind();
            let for_cancel = slot.clone_ref(py);
            let for_drop = slot.clone_ref(py);
            (slot, for_cancel, for_drop)
        });
        let mut guard = cancel::CancelOnDrop::new(slot_for_drop);
        let mut blocking =
            tokio::task::spawn_blocking(move || Python::with_gil(|py| call.run(py, &slot)));
        let joined = tokio::select! {
            joined = &mut blocking => joined,
            () = self.cancel.cancelled() => {
                guard.disarm();
                cancel::request(&slot_for_cancel);
                let _ = blocking.await;
                tracing::event!(
                    tracing::Level::INFO,
                    tool = %self.name,
                    caller = %self.caller,
                    "package.tool.canceled"
                );
                return Err(ToolExecutionError::ExecutionFailed {
                    code: "canceled".to_string(),
                    message: format!("{}: the run was stopped", self.name),
                });
            }
        };
        guard.disarm();
        let text = joined.map_err(|e| failure("join_failed", &self.name, e))??;
        serde_json::from_str(&text).map_err(|e| failure("invalid_output", &self.name, e))
    }
}

impl ToolExecutor for PackageToolExecutor {
    fn name(&self) -> &str {
        &self.name
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(self.run(input))
    }
}

fn failure(code: &str, tool: &str, error: impl std::fmt::Display) -> ToolExecutionError {
    ToolExecutionError::ExecutionFailed {
        code: code.to_string(),
        message: format!("{tool}: {error}"),
    }
}

/// What one call carries to the blocking thread.
struct Call {
    tool: Arc<PackageTool>,
    site_packages: Vec<PathBuf>,
    caller: String,
    arguments: String,
}

impl Call {
    /// Run the call under the GIL, cancellable through `slot`. Returns the
    /// JSON text of what the tool returned.
    fn run(&self, py: Python<'_>, slot: &Py<PyDict>) -> Result<String, ToolExecutionError> {
        let name = self.tool.full_name();
        let helpers = HELPERS
            .get_or_try_init(py, || -> PyResult<Py<PyModule>> {
                Ok(PyModule::from_code(
                    py,
                    HELPERS_CODE,
                    c"apollia_package_tools.py",
                    c"apollia_package_tools",
                )?
                .unbind())
            })
            .map_err(|e| failure("internal", &name, e))?
            .bind(py);
        let function = self
            .tool
            .function
            .get_or_try_init(py, || load(helpers, &self.tool, &self.site_packages))
            .map_err(|e| failure("load_failed", &name, e))?
            .bind(py);
        let ctx = Py::new(py, RuntimeContext::for_package_tool(&self.caller))
            .map_err(|e| failure("internal", &name, e))?;
        let json = py
            .import("json")
            .map_err(|e| failure("internal", &name, e))?;
        let arguments = json
            .call_method1("loads", (self.arguments.as_str(),))
            .map_err(|e| failure("invalid_input", &name, e))?;
        let coroutine = helpers
            .call_method1("call_tool", (function, ctx, arguments))
            .and_then(|coroutine| cancel::wrap(py, &coroutine, slot))
            .map_err(|e| failure("python_exception", &name, e))?;
        let result = py
            .import("asyncio")
            .and_then(|asyncio| asyncio.call_method1("run", (coroutine,)))
            .map_err(|e| failure("python_exception", &name, e))?;
        json.call_method1("dumps", (result,))
            .and_then(|text| text.extract::<String>())
            .map_err(|e| failure("invalid_output", &name, e))
    }
}

/// Import the module of `tool` and return its decorated function.
fn load(
    helpers: &Bound<'_, PyModule>,
    tool: &PackageTool,
    site_packages: &[PathBuf],
) -> Result<Py<PyAny>, String> {
    let dotted = dotted_module(&tool.decl.module).ok_or_else(|| {
        format!(
            "module `{}` is not a Python file inside the package",
            tool.decl.module
        )
    })?;
    let unique = module_name(&tool.root, &dotted);
    let site_packages: Vec<String> = site_packages
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let root = tool.root.to_string_lossy().into_owned();
    let function = helpers
        .call_method1(
            "load_tool",
            (
                root,
                site_packages,
                tool.decl.module.as_str(),
                unique.as_str(),
                tool.decl.name.as_str(),
            ),
        )
        .map_err(|e| e.to_string())?;
    tracing::event!(
        tracing::Level::INFO,
        tool = %tool.full_name(),
        module = %unique,
        "package.tool.loaded"
    );
    Ok(function.unbind())
}

/// The name a package module is imported under: its dotted name, prefixed by
/// a digest of the package root.
fn module_name(root: &std::path::Path, dotted: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut hasher);
    format!("apollia_pkg_{:016x}.{dotted}", hasher.finish())
}
