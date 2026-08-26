//! AIP duck-typing validator for Python agent objects.
//!
//! Every Apollia agent is built with the decorator SDK (`@agent`,
//! `@skill`, `@on_message`, `@orchestrated`), which attaches
//! `__apollia_manifest__` (class attribute) and `__apollia_dispatch__`
//! (async method) to the agent class.  The validator deserialises the
//! manifest, detects optional callbacks, and refuses any agent that does
//! not expose those two attributes.

use apollia_core::AgentManifest;
use apollia_tools::NATIVE_TOOL_NAMES;
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
    /// `true` if the agent implements `health_check()`.
    pub has_health_check: bool,
    /// `true` if the agent implements `on_plan_complete()`.
    pub has_on_plan_complete: bool,
}

/// Errors that can occur during AIP validation of a Python agent.
#[derive(Debug, thiserror::Error)]
pub enum AIPValidationError {
    /// The agent class is missing `__apollia_manifest__`: the @agent
    /// decorator was not applied.
    #[error(
        "agent missing required attribute '__apollia_manifest__' \
         - apply the @agent decorator from apollia"
    )]
    MissingManifest,

    /// The agent class is missing `__apollia_dispatch__`: the @agent
    /// decorator was not applied.
    #[error(
        "agent missing required attribute '__apollia_dispatch__' \
         - apply the @agent decorator from apollia"
    )]
    MissingRun,

    /// The `__apollia_dispatch__` method is not async (not a coroutine function).
    #[error("agent method '__apollia_dispatch__' must be async (coroutine function)")]
    RunNotAsync,

    /// `__apollia_manifest__` could not be deserialized into `AgentManifest`.
    #[error("__apollia_manifest__ contains invalid data: {0}")]
    InvalidManifest(String),

    /// `manifest.version` is not valid semver (`MAJOR.MINOR.PATCH`).
    #[error("version '{value}' is not valid semver - use '1.0.0'")]
    InvalidVersion {
        /// Raw value provided in the manifest.
        value: String,
    },

    /// `tools_required` references a name that looks like a typo of a known
    /// native tool (Levenshtein distance ≤ 2 to a known name).
    #[error("tool '{name}' not found - did you mean '{suggestion}'?")]
    UnknownTool {
        /// The unknown tool name as declared by the agent.
        name: String,
        /// The closest matching native tool name.
        suggestion: String,
    },

    /// Generic Python error during validation.
    #[error("Python error during validation: {0}")]
    PythonError(String),
}

/// Levenshtein distance between two strings (iterative, O(n·m)).
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0usize; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (curr[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

/// Length of the common ASCII prefix of `a` and `b`.
fn common_prefix_len(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}

/// Find the closest native tool name to `name` if it looks like a typo.
///
/// Heuristic: a candidate is a likely typo if it shares a common prefix of
/// at least 4 bytes with `name`, or if its Levenshtein distance is small
/// enough relative to the name length (<= max(2, len / 3)). The candidate
/// minimizing `(-prefix_len, distance)` wins. Returns `None` if no native
/// tool matches the heuristic: the unknown name is then assumed to belong
/// to a non-native registry (MCP, custom dispatcher) and passes through.
fn closest_native_tool(name: &str) -> Option<&'static str> {
    let limit = (name.len() / 3).max(2);
    let mut best: Option<(&'static str, usize, usize)> = None;
    for &candidate in NATIVE_TOOL_NAMES {
        if candidate == name {
            return None;
        }
        let prefix = common_prefix_len(name, candidate);
        let dist = levenshtein(name, candidate);
        let prefix_match = prefix >= 4;
        let distance_match = dist <= limit;
        if !prefix_match && !distance_match {
            continue;
        }
        let better = match best {
            None => true,
            Some((_, bp, bd)) => prefix > bp || (prefix == bp && dist < bd),
        };
        if better {
            best = Some((candidate, prefix, dist));
        }
    }
    best.map(|(n, _, _)| n)
}

/// Apply semantic validation on a freshly deserialized manifest:
/// - `version` must be valid semver
/// - each `tools_required` entry must not be a typo of a known native tool
fn validate_manifest_semantics(manifest: &AgentManifest) -> Result<(), AIPValidationError> {
    if semver::Version::parse(&manifest.version).is_err() {
        return Err(AIPValidationError::InvalidVersion {
            value: manifest.version.clone(),
        });
    }
    for tool in &manifest.tools_required {
        if NATIVE_TOOL_NAMES.contains(&tool.as_str()) {
            continue;
        }
        if let Some(suggestion) = closest_native_tool(tool) {
            return Err(AIPValidationError::UnknownTool {
                name: tool.clone(),
                suggestion: suggestion.to_string(),
            });
        }
    }
    Ok(())
}

/// Validates that a Python object is an Apollia decorator-built agent.
///
/// Checks for the presence of `__apollia_manifest__` and
/// `__apollia_dispatch__` (both installed by the `@agent` decorator),
/// deserialises the manifest into [`AgentManifest`], and detects optional
/// callbacks (`health_check`, `on_plan_complete`).
///
/// # Errors
///
/// - [`AIPValidationError::MissingManifest`] if the object has no `__apollia_manifest__`
/// - [`AIPValidationError::MissingRun`] if the object has no `__apollia_dispatch__`
/// - [`AIPValidationError::RunNotAsync`] if `__apollia_dispatch__` is not a coroutine function
/// - [`AIPValidationError::InvalidManifest`] if the manifest dict is invalid
/// - [`AIPValidationError::PythonError`] for any other Python error
pub fn validate_agent(agent: &Py<PyAny>) -> Result<ValidatedAgent, AIPValidationError> {
    Python::with_gil(|py| {
        let agent_ref = agent.bind(py);

        // Require __apollia_manifest__ (installed by @agent).
        if !agent_ref
            .hasattr("__apollia_manifest__")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?
        {
            return Err(AIPValidationError::MissingManifest);
        }

        // Require __apollia_dispatch__ (installed by @agent).
        if !agent_ref
            .hasattr("__apollia_dispatch__")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?
        {
            return Err(AIPValidationError::MissingRun);
        }

        // The dispatch hook must be an async coroutine function.
        let inspect = py
            .import("inspect")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;
        let dispatch_method = agent_ref
            .getattr("__apollia_dispatch__")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;
        let is_coro: bool = inspect
            .call_method1("iscoroutinefunction", (&dispatch_method,))
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?
            .extract()
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;
        if !is_coro {
            return Err(AIPValidationError::RunNotAsync);
        }

        // The manifest is a static class attribute pre-built by the
        // @agent decorator. We do not call any dynamic `manifest()`
        // method; that legacy escape hatch is gone.
        let json_mod = py
            .import("json")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;
        let manifest_obj = agent_ref
            .getattr("__apollia_manifest__")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;
        let json_str: String = json_mod
            .call_method1("dumps", (&manifest_obj,))
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?
            .extract()
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;
        let mut manifest: AgentManifest = serde_json::from_str(&json_str)
            .map_err(|e| AIPValidationError::InvalidManifest(e.to_string()))?;

        // Semantic validation: semver + tool typo detection.
        validate_manifest_semantics(&manifest)?;

        // The Python class is the source of truth for agent type.
        // Extract `agent.__class__.__name__` and stamp it on the manifest so
        // downstream consumers (UI, registry) can render it as a badge.
        let class_name: Option<String> = agent_ref
            .getattr("__class__")
            .and_then(|cls| cls.getattr("__name__"))
            .and_then(|n| n.extract::<String>())
            .ok();
        manifest.agent_class = class_name;

        // Detect optional callbacks
        let has_health_check = agent_ref
            .hasattr("health_check")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;
        let has_on_plan_complete = agent_ref
            .hasattr("on_plan_complete")
            .map_err(|e| AIPValidationError::PythonError(e.to_string()))?;

        Ok(ValidatedAgent {
            object: agent.clone_ref(py),
            manifest,
            has_health_check,
            has_on_plan_complete,
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
            let code_c = std::ffi::CString::new(code).expect("code contains NUL byte");
            let module = PyModule::from_code(py, &code_c, c"test_agent.py", c"test_agent")
                .expect("failed to create test module");
            module.getattr("agent").expect("failed to get agent").into()
        })
    }

    /// Builds a minimal decorator-style agent.
    ///
    /// We do not import the real Apollia SDK from these unit tests (the
    /// PyO3 bindings live in the same crate, and importing them would
    /// require pip-installing the SDK). Instead, we forge a class with
    /// the same surface contract the validator inspects:
    /// `__apollia_manifest__` (dict) and an async `__apollia_dispatch__`
    /// method.
    fn build_agent(manifest: &str, extra_methods: &str) -> Py<PyAny> {
        let code = format!(
            r#"
class A:
    __apollia_manifest__ = {manifest}

    async def __apollia_dispatch__(self, task, ctx):
        return {{"status": "completed", "output": []}}
{extra_methods}
agent = A()
"#,
        );
        create_py_agent(&code)
    }

    const VALID_MANIFEST: &str = r#"{
        "name": "test-agent",
        "version": "0.1.0",
        "description": "A test agent",
        "tools_required": [],
    }"#;

    #[test]
    fn test_validate_valid_agent() {
        // GIVEN a valid decorator-style agent with __apollia_manifest__
        //   AND async __apollia_dispatch__
        let agent = build_agent(VALID_MANIFEST, "");

        // WHEN we validate
        let validated = validate_agent(&agent).expect("validation should succeed");

        // THEN validation succeeds with correct manifest
        assert_eq!(validated.manifest.name, "test-agent");
        assert_eq!(validated.manifest.version, "0.1.0");
        assert!(!validated.has_health_check);
    }

    #[test]
    fn test_validate_missing_manifest() {
        // GIVEN an agent without __apollia_manifest__
        let agent = create_py_agent(
            "class A:\n    async def __apollia_dispatch__(self, t, c): pass\nagent = A()\n",
        );

        // WHEN we validate
        let result = validate_agent(&agent);

        // THEN we get MissingManifest
        assert!(matches!(result, Err(AIPValidationError::MissingManifest)));
    }

    #[test]
    fn test_validate_missing_dispatch() {
        // GIVEN an agent without __apollia_dispatch__
        let agent = create_py_agent("class A:\n    __apollia_manifest__ = {}\nagent = A()\n");

        // WHEN we validate
        let result = validate_agent(&agent);

        // THEN we get MissingRun (dispatch hook absent)
        assert!(matches!(result, Err(AIPValidationError::MissingRun)));
    }

    #[test]
    fn test_validate_dispatch_not_async() {
        // GIVEN an agent whose dispatch hook is synchronous
        let agent = create_py_agent(
            "class A:\n    __apollia_manifest__ = {}\n    def __apollia_dispatch__(self, t, c): pass\nagent = A()\n",
        );

        // WHEN we validate
        let result = validate_agent(&agent);

        // THEN we get RunNotAsync
        assert!(matches!(result, Err(AIPValidationError::RunNotAsync)));
    }

    #[test]
    fn test_validate_invalid_manifest_data() {
        // GIVEN an agent whose manifest is missing required fields
        let agent = build_agent(r#"{"name": "x"}"#, "");

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
        // GIVEN an agent with health_check
        let extra = r#"
    def health_check(self): return True
"#;
        let agent = build_agent(
            r#"{
                "name": "cb-agent",
                "version": "1.0.0",
                "description": "Agent with callbacks",
                "tools_required": [],
            }"#,
            extra,
        );

        // WHEN we validate
        let validated = validate_agent(&agent).expect("validation should succeed");

        // THEN callbacks are detected
        assert!(validated.has_health_check);
    }

    #[test]
    fn test_validate_rejects_non_semver_version() {
        // GIVEN an agent whose manifest version is not semver (emoji).
        let agent = build_agent(
            r#"{
                "name": "broken",
                "version": "🚀",
                "description": "bad",
                "tools_required": [],
            }"#,
            "",
        );

        // WHEN we validate
        let result = validate_agent(&agent);

        // THEN we get InvalidVersion with the offending value in the message.
        match result {
            Err(AIPValidationError::InvalidVersion { value }) => assert_eq!(value, "🚀"),
            other => panic!("expected InvalidVersion, got {other:?}"),
        }
    }

    #[test]
    fn test_validate_detects_tool_typo() {
        // GIVEN an agent requiring a tool that is a Levenshtein-close typo.
        let agent = build_agent(
            r#"{
                "name": "typo-agent",
                "version": "1.0.0",
                "description": "typo",
                "tools_required": ["bash_explorr"],
            }"#,
            "",
        );

        // WHEN we validate
        let result = validate_agent(&agent);

        // THEN we get UnknownTool with `bash_executor` as the suggestion.
        match result {
            Err(AIPValidationError::UnknownTool { name, suggestion }) => {
                assert_eq!(name, "bash_explorr");
                assert_eq!(suggestion, "bash_executor");
            }
            other => panic!("expected UnknownTool, got {other:?}"),
        }
    }

    #[test]
    fn test_validate_extracts_python_class_name() {
        // GIVEN a valid decorator-style agent whose class is named `MyAgent`.
        let agent = create_py_agent(
            r#"
class MyAgent:
    __apollia_manifest__ = {
        "name": "demo",
        "version": "1.0.0",
        "description": "demo",
        "tools_required": [],
    }
    async def __apollia_dispatch__(self, t, c): pass
agent = MyAgent()
"#,
        );

        // WHEN we validate
        let validated = validate_agent(&agent).expect("validation should succeed");

        // THEN the class name is captured on the manifest.
        assert_eq!(validated.manifest.agent_class.as_deref(), Some("MyAgent"));
    }

    #[test]
    fn test_validate_passes_unknown_non_typo_tool() {
        // GIVEN an agent requiring a tool whose name is too far from any
        // native tool to be a typo (e.g. an MCP-provided tool).
        let agent = build_agent(
            r#"{
                "name": "mcp-agent",
                "version": "1.0.0",
                "description": "uses an mcp tool",
                "tools_required": ["my_custom_mcp_tool"],
            }"#,
            "",
        );

        // WHEN we validate
        let result = validate_agent(&agent);

        // THEN validation succeeds; unknown but distant names pass through.
        assert!(result.is_ok(), "expected ok, got {result:?}");
    }
}
