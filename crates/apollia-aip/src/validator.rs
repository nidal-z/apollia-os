//! AIP duck-typing validator for Python agent objects.
//!
//! Validates that a Python object is AIP-compatible by checking for
//! `manifest()` (synchronous) and `run()` (async coroutine function).
//! Optional callbacks (`on_start`, `on_stop`, `health_check`) are detected
//! but not required.

use apollia_core::AgentManifest;
use pyo3::prelude::*;

/// Result of AIP validation on a Python agent object.
///
/// Contains the validated Python object, the deserialized manifest,
/// and flags indicating which optional callbacks are present.
#[derive(Debug)]
pub struct ValidatedAgent {
    /// The validated Python agent object.
    pub object: Py<PyAny>,
    /// The manifest deserialized from the Python dict.
    pub manifest: AgentManifest,
    /// `true` if the agent implements `on_start()`.
    pub has_on_start: bool,
    /// `true` if the agent implements `on_stop()`.
    pub has_on_stop: bool,
    /// `true` if the agent implements `health_check()`.
    pub has_health_check: bool,
}

/// Errors that can occur during AIP validation of a Python agent.
#[derive(Debug, thiserror::Error)]
pub enum AIPValidationError {
    /// The agent does not have a `manifest()` method.
    #[error("agent missing required method 'manifest()'")]
    MissingManifest,

    /// The agent does not have a `run()` method.
    #[error("agent missing required method 'run()'")]
    MissingRun,

    /// The `run()` method is not async (not a coroutine function).
    #[error("agent method 'run()' must be async (coroutine function)")]
    RunNotAsync,

    /// `manifest()` returned data that cannot be deserialized into `AgentManifest`.
    #[error("manifest() returned invalid data: {0}")]
    InvalidManifest(String),

    /// Generic Python error during validation.
    #[error("Python error during validation: {0}")]
    PythonError(String),
}

/// Validates that a Python object is AIP-compatible.
///
/// Checks for the presence of `manifest()` (synchronous) and `run()` (async),
/// calls `manifest()` to deserialize into [`AgentManifest`],
/// and detects optional callbacks (`on_start`, `on_stop`, `health_check`).
///
/// # Errors
///
/// - [`AIPValidationError::MissingManifest`] if the object has no `manifest()` method
/// - [`AIPValidationError::MissingRun`] if the object has no `run()` method
/// - [`AIPValidationError::RunNotAsync`] if `run()` is not a coroutine function
/// - [`AIPValidationError::InvalidManifest`] if the dict returned by `manifest()` is invalid
/// - [`AIPValidationError::PythonError`] for any other Python error
pub fn validate_agent(agent: &Py<PyAny>) -> Result<ValidatedAgent, AIPValidationError> {
    Python::with_gil(|py| {
        let agent_ref = agent.bind(py);

        // Check manifest() exists
        if !agent_ref
            .hasattr("manifest")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?
        {
            return Err(AIPValidationError::MissingManifest);
        }

        // Check run() exists
        if !agent_ref
            .hasattr("run")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?
        {
            return Err(AIPValidationError::MissingRun);
        }

        // Check run() is async (coroutine function)
        let inspect = py
            .import_bound("inspect")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;
        let run_method = agent_ref
            .getattr("run")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;
        let is_coro: bool = inspect
            .call_method1("iscoroutinefunction", (&run_method,))
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?
            .extract()
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;
        if !is_coro {
            return Err(AIPValidationError::RunNotAsync);
        }

        // Call manifest() and convert dict -> JSON -> AgentManifest
        let manifest_dict = agent_ref
            .call_method0("manifest")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;
        let json_mod = py
            .import_bound("json")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;
        let json_str: String = json_mod
            .call_method1("dumps", (&manifest_dict,))
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?
            .extract()
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;
        let manifest: AgentManifest = serde_json::from_str(&json_str)
            .map_err(|e| AIPValidationError::InvalidManifest(e.to_string()))?;

        // Detect optional callbacks
        let has_on_start = agent_ref
            .hasattr("on_start")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;
        let has_on_stop = agent_ref
            .hasattr("on_stop")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;
        let has_health_check = agent_ref
            .hasattr("health_check")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;

        Ok(ValidatedAgent {
            object: agent.clone_ref(py),
            manifest,
            has_on_start,
            has_on_stop,
            has_health_check,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::types::PyModule;

    /// Helper: create a Python agent object from inline code.
    fn create_py_agent(code: &str) -> Py<PyAny> {
        Python::with_gil(|py| {
            let module = PyModule::from_code_bound(py, code, "test_agent.py", "test_agent")
                .expect("failed to create test module");
            module
                .getattr("agent")
                .expect("failed to get agent")
                .into()
        })
    }

    const VALID_AGENT: &str = r#"
import asyncio

class TestAgent:
    def manifest(self):
        return {
            "name": "test-agent",
            "version": "0.1.0",
            "description": "A test agent",
            "tools_required": [],
        }

    async def run(self, task, ctx):
        return {"status": "completed", "output": "ok"}

agent = TestAgent()
"#;

    #[test]
    fn test_validate_valid_agent() {
        // GIVEN a valid Python agent with manifest() + async run()
        let agent = create_py_agent(VALID_AGENT);

        // WHEN we validate
        let result = validate_agent(&agent);

        // THEN validation succeeds with correct manifest
        assert!(result.is_ok());
        let validated = result.expect("validation should succeed");
        assert_eq!(validated.manifest.name, "test-agent");
        assert_eq!(validated.manifest.version, "0.1.0");
        assert!(!validated.has_on_start);
        assert!(!validated.has_on_stop);
        assert!(!validated.has_health_check);
    }

    #[test]
    fn test_validate_missing_manifest() {
        // GIVEN a Python agent without manifest()
        let agent =
            create_py_agent("class A:\n    async def run(self, t, c): pass\nagent = A()\n");

        // WHEN we validate
        let result = validate_agent(&agent);

        // THEN we get MissingManifest
        assert!(matches!(result, Err(AIPValidationError::MissingManifest)));
    }

    #[test]
    fn test_validate_missing_run() {
        // GIVEN a Python agent without run()
        let agent = create_py_agent("class A:\n    def manifest(self): return {}\nagent = A()\n");

        // WHEN we validate
        let result = validate_agent(&agent);

        // THEN we get MissingRun
        assert!(matches!(result, Err(AIPValidationError::MissingRun)));
    }

    #[test]
    fn test_validate_run_not_async() {
        // GIVEN a Python agent with synchronous run()
        let agent = create_py_agent(
            "class A:\n    def manifest(self): return {}\n    def run(self, t, c): pass\nagent = A()\n",
        );

        // WHEN we validate
        let result = validate_agent(&agent);

        // THEN we get RunNotAsync
        assert!(matches!(result, Err(AIPValidationError::RunNotAsync)));
    }

    #[test]
    fn test_validate_invalid_manifest_data() {
        // GIVEN an agent whose manifest() returns an incomplete dict
        let agent = create_py_agent(
            r#"
class A:
    def manifest(self):
        return {"name": "x"}
    async def run(self, t, c): pass
agent = A()
"#,
        );

        // WHEN we validate
        let result = validate_agent(&agent);

        // THEN we get InvalidManifest
        assert!(matches!(
            result,
            Err(AIPValidationError::InvalidManifest(_))
        ));
    }

    #[test]
    fn test_validate_detects_optional_callbacks() {
        // GIVEN an agent with on_start, on_stop and health_check
        let agent = create_py_agent(
            r#"
class A:
    def manifest(self):
        return {
            "name": "cb-agent",
            "version": "1.0.0",
            "description": "Agent with callbacks",
            "tools_required": [],
        }
    async def run(self, t, c): pass
    async def on_start(self, ctx): pass
    async def on_stop(self): pass
    def health_check(self): return True
agent = A()
"#,
        );

        // WHEN we validate
        let result = validate_agent(&agent);

        // THEN callbacks are detected
        assert!(result.is_ok());
        let validated = result.expect("validation should succeed");
        assert!(validated.has_on_start);
        assert!(validated.has_on_stop);
        assert!(validated.has_health_check);
    }
}
