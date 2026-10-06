//! `code_context`: the state of a code folder, read when the agent asks.
//!
//! A code folder is a source an agent has checked: a folder connection whose
//! git state, rules (`APOLLIA.md`) and tree the agent may consult. Nothing of
//! it reaches the prompt on its own; the agent calls this tool, which runs the
//! git, rules and tree providers of `apollia-workspace` on the folder at that
//! moment and answers what they found.
//!
//! The tool exists for an agent only while a code folder is checked for it:
//! the source grants the tool, the agent's access does not
//! ([`granted_with_code_folders`]).

use std::path::PathBuf;

use apollia_core::workspace::WorkspaceProvider;
use apollia_core::SandboxProfile;
use apollia_workspace::{
    GitProvider, ProjectRuntime, RulesProvider, RuntimeConfig, TreeProvider, WorkspaceSnapshot,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;

use crate::descriptor::{ActionFamily, FamilySource, ToolDescriptor, ToolKind};
use crate::executor::{ToolExecutionError, ToolExecutor};

/// Name of the tool, as the agent calls it.
pub const CODE_CONTEXT_TOOL: &str = "code_context";

/// The parts of a code folder the tool can read.
const PARTS: [&str; 3] = ["git", "rules", "tree"];

/// One code folder the agent has checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeFolder {
    /// The name the user gave the source.
    pub label: String,
    /// The folder, as its folder connection holds it.
    pub path: PathBuf,
}

/// The `code_context` tool, bound to the code folders of one agent.
#[derive(Debug, Clone)]
pub struct CodeContext {
    folders: Vec<CodeFolder>,
}

/// Input of a call.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeContextInput {
    /// The folder to read, by its label or its path; the only one when the
    /// agent has a single code folder, every one otherwise.
    #[serde(default)]
    pub folder: Option<String>,
    /// The parts to read among `git`, `rules` and `tree`; all three when
    /// absent or empty.
    #[serde(default)]
    pub parts: Vec<String>,
}

/// What was read in one folder.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FolderContext {
    /// The name of the source.
    pub label: String,
    /// The folder read.
    pub path: String,
    /// One entry per section a provider produced.
    pub sections: Vec<ContextSection>,
    /// What a provider could not read.
    pub errors: Vec<String>,
}

/// One section of a folder's context.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextSection {
    /// The part it comes from: `git`, `rules` or `tree`.
    pub part: String,
    /// Its title.
    pub title: String,
    /// Its text.
    pub content: String,
}

impl CodeContext {
    /// The tool over `folders`.
    pub fn new(folders: Vec<CodeFolder>) -> Self {
        Self { folders }
    }

    /// Reads the parts `input` asks for in the folders it names.
    ///
    /// # Errors
    ///
    /// [`ToolExecutionError::InvalidInput`] for a part that is not one of
    /// `git`, `rules`, `tree`, and [`ToolExecutionError::ExecutionFailed`]
    /// (`unknown_folder`) for a folder that is not checked for the agent.
    pub async fn run(
        &self,
        input: CodeContextInput,
    ) -> Result<Vec<FolderContext>, ToolExecutionError> {
        if let Some(unknown) = input.parts.iter().find(|p| !PARTS.contains(&p.as_str())) {
            return Err(ToolExecutionError::InvalidInput {
                message: format!("unknown part '{unknown}', expected git, rules or tree"),
            });
        }
        let folders: Vec<&CodeFolder> = match input.folder.as_deref().map(str::trim) {
            None | Some("") => self.folders.iter().collect(),
            Some(wanted) => {
                let found: Vec<&CodeFolder> = self
                    .folders
                    .iter()
                    .filter(|f| f.label == wanted || f.path == std::path::Path::new(wanted))
                    .collect();
                if found.is_empty() {
                    let known: Vec<&str> = self.folders.iter().map(|f| f.label.as_str()).collect();
                    return Err(ToolExecutionError::ExecutionFailed {
                        code: "unknown_folder".to_owned(),
                        message: format!(
                            "'{wanted}' is not a code folder of this agent; its code folders are: {}",
                            known.join(", ")
                        ),
                    });
                }
                found
            }
        };
        let runtime = runtime_for(&input.parts);
        let mut out = Vec::with_capacity(folders.len());
        for folder in folders {
            let snapshot = runtime.collect(&folder.path).await;
            out.push(folder_context(folder, snapshot));
        }
        Ok(out)
    }

    /// The descriptor registered in the tool registry.
    pub fn descriptor() -> ToolDescriptor {
        ToolDescriptor {
            name: CODE_CONTEXT_TOOL.to_owned(),
            version: "1.0.0".to_owned(),
            description: "Read the state of a code folder checked for this agent: its git branch, \
                          status and recent commits, its rules file (APOLLIA.md) and its file tree. \
                          Call it when the task needs them; nothing of the folder is in the prompt \
                          otherwise."
                .to_owned(),
            kind: ToolKind::Native,
            input_schema: json!({
                "type": "object",
                "properties": {
                    "folder": {
                        "type": "string",
                        "description": "The code folder to read, by its name or its path. Every code folder when absent."
                    },
                    "parts": {
                        "type": "array",
                        "items": { "type": "string", "enum": PARTS },
                        "description": "The parts to read. All three when absent."
                    }
                }
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "folders": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "label": { "type": "string" },
                                "path": { "type": "string" },
                                "sections": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "part": { "type": "string" },
                                            "title": { "type": "string" },
                                            "content": { "type": "string" }
                                        },
                                        "required": ["part", "title", "content"]
                                    }
                                },
                                "errors": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["label", "path", "sections", "errors"]
                        }
                    }
                },
                "required": ["folders"]
            })),
            sandbox_profile: SandboxProfile::FileSystem,
            tags: vec!["code".to_owned(), "git".to_owned(), "source".to_owned()],
            family: ActionFamily::Read,
            family_source: FamilySource::Builtin,
            target_arg: Some("folder".to_string()),
            is_read_only: true,
            risk_score: 0,
            approval_risk_level: None,
            impact_description: None,
            reject_reason_required: false,
        }
    }
}

/// `tools`, the tools an agent's access grants, with `code_context` present
/// exactly when the agent has a code folder checked.
///
/// The tool is granted by the source, never by the access: an access that
/// names it without a code folder does not keep it, and a code folder adds it
/// to an access that does not name it.
pub fn granted_with_code_folders(mut tools: Vec<String>, has_code_folder: bool) -> Vec<String> {
    tools.retain(|t| t != CODE_CONTEXT_TOOL);
    if has_code_folder {
        tools.push(CODE_CONTEXT_TOOL.to_owned());
    }
    tools
}

/// The providers for `parts`, all three when it is empty, run without cache:
/// each call reads the folder as it is now.
fn runtime_for(parts: &[String]) -> ProjectRuntime {
    let wants = |name: &str| parts.is_empty() || parts.iter().any(|p| p == name);
    let mut providers: Vec<Box<dyn WorkspaceProvider>> = Vec::new();
    if wants("git") {
        providers.push(Box::new(GitProvider::default()));
    }
    if wants("rules") {
        providers.push(Box::new(RulesProvider::default()));
    }
    if wants("tree") {
        providers.push(Box::new(TreeProvider::default()));
    }
    ProjectRuntime::new(
        providers,
        RuntimeConfig {
            context_ttl_secs: 0,
            ..RuntimeConfig::default()
        },
    )
}

fn folder_context(folder: &CodeFolder, snapshot: WorkspaceSnapshot) -> FolderContext {
    let mut sections = Vec::new();
    let mut errors = Vec::new();
    for slice in snapshot.slices {
        errors.extend(slice.errors);
        sections.extend(slice.sections.into_iter().map(|s| ContextSection {
            part: slice.source.clone(),
            title: s.title,
            content: s.content,
        }));
    }
    FolderContext {
        label: folder.label.clone(),
        path: folder.path.display().to_string(),
        sections,
        errors,
    }
}

impl ToolExecutor for CodeContext {
    fn name(&self) -> &str {
        CODE_CONTEXT_TOOL
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            let typed: CodeContextInput =
                serde_json::from_value(input).map_err(|e| ToolExecutionError::InvalidInput {
                    message: e.to_string(),
                })?;
            let folders = self.run(typed).await?;
            Ok(json!({ "folders": folders }))
        })
    }
}

#[cfg(test)]
mod tests;
