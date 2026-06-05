//! The declarative evaluation suite model and its TOML parsing.
//!
//! A suite is authored by hand as TOML (Principle #8: a human-readable, editable
//! format). It names a set of tasks; each task carries a prompt, a run count, an
//! optional target agent, and a list of typed assertions that decide pass/fail.

use std::path::{Path, PathBuf};

/// A declarative evaluation suite parsed from TOML.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct EvalSuite {
    /// Human-readable suite name, surfaced in reports.
    pub name: String,
    /// Tasks to evaluate. Defaults to empty when the `tasks` array is absent.
    #[serde(default)]
    pub tasks: Vec<EvalTask>,
}

/// A single task in a suite: one prompt, run `runs` times, judged by `assertions`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct EvalTask {
    /// Stable identifier for the task, used as the report row key.
    pub id: String,
    /// The instruction submitted to the agent.
    pub prompt: String,
    /// Number of times the task is executed. Defaults to 3.
    #[serde(default = "default_runs")]
    pub runs: u32,
    /// Target agent identifier. `None` defers the choice to the caller.
    pub agent: Option<String>,
    /// Typed pass/fail assertions evaluated on each run.
    #[serde(default)]
    pub assertions: Vec<Assertion>,
}

/// The default number of runs per task when the field is omitted.
fn default_runs() -> u32 {
    3
}

/// A typed pass/fail assertion on a single task run.
///
/// The TOML discriminator is the `type` field (`exit_code`, `file_exists`,
/// `regex`, `llm_judge`); an unknown value is a deserialization error.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Assertion {
    /// Passes when the run exit code equals `equals`.
    ExitCode {
        /// The expected exit code.
        equals: i32,
    },
    /// Passes when a file exists at `path` after the run.
    FileExists {
        /// The path that must exist.
        path: String,
    },
    /// Passes when `pattern` matches the selected output channel.
    Regex {
        /// The channel to match against.
        on: OutputChannel,
        /// The regular expression to search for.
        pattern: String,
    },
    /// Passes when an LLM judge rates the output against `rubric` as a pass.
    LlmJudge {
        /// The rubric handed to the judge.
        rubric: String,
    },
}

/// The output channel a [`Assertion::Regex`] matches against.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputChannel {
    /// The streamed stdout of the run.
    Stdout,
    /// The final result text of the run.
    Result,
}

/// Errors raised while loading or running an evaluation suite.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum EvalError {
    /// The suite TOML is malformed, has an unknown shape, or is missing a
    /// required field. The inner string carries the underlying detail.
    #[error("invalid eval suite: {0}")]
    InvalidSuite(String),

    /// Reading the suite file from disk failed.
    #[error("failed to read suite at {path}")]
    Io {
        /// The path that could not be read.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

impl EvalSuite {
    /// Parses a suite from a TOML string.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError::InvalidSuite`] when the TOML is malformed, a required
    /// field is missing, or an assertion `type` is unknown.
    pub fn from_toml_str(content: &str) -> Result<Self, EvalError> {
        toml::from_str(content).map_err(|e| EvalError::InvalidSuite(e.to_string()))
    }

    /// Reads a suite from a TOML file on disk.
    ///
    /// # Errors
    ///
    /// Returns [`EvalError::Io`] when the file cannot be read, or
    /// [`EvalError::InvalidSuite`] when its contents do not parse.
    pub fn from_path(path: &Path) -> Result<Self, EvalError> {
        let content = std::fs::read_to_string(path).map_err(|source| EvalError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_toml_str(&content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // AC-1
    #[test]
    fn test_parse_valid_suite_two_tasks() {
        // GIVEN a valid TOML suite with two tasks, each with prompt, runs and assertions
        let toml = r#"
            name = "smoke"

            [[tasks]]
            id = "echo"
            prompt = "say hello"
            runs = 2
            assertions = [
                { type = "exit_code", equals = 0 },
            ]

            [[tasks]]
            id = "write-file"
            prompt = "write a report"
            runs = 4
            agent = "writer"
            assertions = [
                { type = "file_exists", path = "/tmp/report.txt" },
                { type = "regex", on = "result", pattern = "done" },
            ]
        "#;

        // WHEN parsing the suite
        let suite = EvalSuite::from_toml_str(toml).expect("valid suite must parse");

        // THEN both tasks and their typed assertions are present
        assert_eq!(suite.name, "smoke");
        assert_eq!(suite.tasks.len(), 2);

        let first = &suite.tasks[0];
        assert_eq!(first.id, "echo");
        assert_eq!(first.runs, 2);
        assert_eq!(first.agent, None);
        assert!(matches!(
            first.assertions.as_slice(),
            [Assertion::ExitCode { equals: 0 }]
        ));

        let second = &suite.tasks[1];
        assert_eq!(second.agent.as_deref(), Some("writer"));
        assert_eq!(second.assertions.len(), 2);
        assert!(matches!(second.assertions[0], Assertion::FileExists { .. }));
        assert!(matches!(
            second.assertions[1],
            Assertion::Regex {
                on: OutputChannel::Result,
                ..
            }
        ));
    }

    // AC-2 (error case)
    #[test]
    fn test_unknown_assertion_type_is_invalid_suite() {
        // GIVEN a TOML suite with an unknown assertion type
        let toml = r#"
            name = "broken"

            [[tasks]]
            id = "t1"
            prompt = "do a thing"
            assertions = [
                { type = "magic", power = 9000 },
            ]
        "#;

        // WHEN parsing the suite
        let err = EvalSuite::from_toml_str(toml).expect_err("unknown type must fail");

        // THEN it is an InvalidSuite error naming the offending value
        let EvalError::InvalidSuite(message) = err else {
            panic!("expected InvalidSuite, got {err:?}");
        };
        assert!(
            message.contains("magic"),
            "message should name the bad variant, got: {message}"
        );
    }

    // AC-3 (error case)
    #[test]
    fn test_missing_prompt_field_fails() {
        // GIVEN a task without the required `prompt` field
        let toml = r#"
            name = "broken"

            [[tasks]]
            id = "t1"
            runs = 2
        "#;

        // WHEN parsing the suite
        let err = EvalSuite::from_toml_str(toml).expect_err("missing prompt must fail");

        // THEN parsing fails cleanly with InvalidSuite
        assert!(matches!(err, EvalError::InvalidSuite(_)));
    }

    // AC-4
    #[test]
    fn test_runs_defaults_to_three() {
        // GIVEN a task with no `runs` field
        let toml = r#"
            name = "defaults"

            [[tasks]]
            id = "t1"
            prompt = "do a thing"
        "#;

        // WHEN parsing succeeds
        let suite = EvalSuite::from_toml_str(toml).expect("valid suite must parse");

        // THEN `runs` falls back to the serde default of 3
        assert_eq!(suite.tasks[0].runs, 3);
    }
}
