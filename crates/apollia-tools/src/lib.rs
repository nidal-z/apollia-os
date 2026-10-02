#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Apollia OS tool registry and native tools.
//!
//! `unsafe_code` is allowed by this crate's manifest for two production
//! sites, the `libc::setrlimit` calls in `tools/rlimits.rs`, and for
//! test-only `std::env::set_var` blocks; every site carries its
//! `// SAFETY:` comment.
//!
//! Provides the tooling infrastructure for agents:
//! - `ToolRegistry`: in-memory catalogue of available tools
//! - `ToolResolver`: validates tool availability at INITIALIZING
//! - `SandboxProfile`: tool isolation profiles (namespace isolation is Linux-only)
//! - `AuditTrail`: SQLite-persisted tool invocation log
//! - `TaskRepository`: SQLite-persisted HITL task state
//! - `AgentRepository`: SQLite-persisted agents, their definitions and overrides
//! - `executor`: [`ToolExecutor`] trait and [`ToolDispatcher`] for unified JSON dispatch
//! - `apollia_manifest`, `apo_archive`: the `apollia.toml` package manifest and
//!   the `.apo` archive that carries it
//!
//! Native tools:
//! - `bash_executor`: shell execution through a resolved POSIX shell;
//!   namespace isolation via unshare(1) on Linux only, no OS sandbox elsewhere
//! - `python_executor`: isolated virtualenv execution
//! - `file_read`, `file_write`, `file_list`, `file_edit`, `file_glob`, `file_grep`: atomic filesystem operations
//! - `http_fetch`: network-restricted HTTP client (feature `http`)
//! - `memory_search`: FTS5 full-text search (feature `memory-search`)
//! - `notebook_read`: read and format Jupyter `.ipynb` cells for LLM consumption
//! - `notebook_edit`: apply atomic cell operations to Jupyter `.ipynb` notebooks

pub mod agent_definition;
mod agent_overrides;
pub mod agent_repository;
pub(crate) mod agents_db;
pub mod apo_archive;
pub mod apollia_manifest;
pub mod audit;
pub mod descriptor;
pub mod dispatcher_invoker;
pub mod executor;
pub mod file_path_extractor;
pub mod governance_db;
pub mod granted_access;
pub mod hitl_schema;
pub mod host_env;
pub mod journal;
pub mod native_dispatcher;
pub mod package_repository;
pub mod project_repository;
pub mod registry;
pub mod resolver;
pub mod sandbox_path;
pub mod task_repository;
pub mod tool_registry;
pub mod tools;
pub mod trusted_keys;

pub use agent_definition::{
    apollia_definition, definition_from_manifest, is_built_in_agent, resolve as resolve_agent,
    AgentOrigin, AgentOverride, OverrideField, OverrideValueError, ResolvedAgent, StoredAgent,
    APOLLIA_AGENT_ID, OVERRIDE_FIELDS,
};
pub use agent_repository::{AgentRepository, AgentRepositoryError, InstalledAgent};
pub use audit::{
    compute_input_hash, AuditStats, AuditTrailError, AuditTrailHandle, ToolInvocationRecord,
};
pub use descriptor::{
    ActionFamily, ApprovalRiskLevel, FamilySource, McpTransport, ToolDescriptor,
    ToolDescriptorError, ToolKind,
};
pub use executor::{
    SessionToolFilter, ToolBatchCall, ToolDispatcher, ToolExecutionError, ToolExecutor,
};
pub use file_path_extractor::FilePathExtractor;
pub use governance_db::{
    GovernanceDb, GovernanceError, GOVERNANCE_DB_FILENAME, LEGACY_BACKUP_FILENAME,
    LEGACY_PERMISSIONS_FILENAME,
};
pub use granted_access::GrantedAccess;
pub use journal::{
    list_sessions, rollback_session, JournalEntry, JournalError, JournalWriter, JournalWriterHandle,
};
pub use native_dispatcher::{
    build_dispatcher_with, build_native_dispatcher, is_disabled, NativeDispatcherConfig,
    UnavailableTool,
};
pub use package_repository::{
    InstalledPackage, PackageItem, PackageItemKind, PackageRepository, PackageRepositoryError,
    PackageSignature,
};
pub use project_repository::{
    ProjectDetail, ProjectDocument, ProjectPatch, ProjectProviderRow, ProjectRepository,
    ProjectRepositoryError, ProjectSummary, ProjectTemplate,
};
pub use registry::{ToolRegistryError, ToolRegistryHandle};
pub use resolver::{resolve, ResolutionReport, ResolutionStatus, ToolResolutionError};
pub use sandbox_path::{SandboxPathError, SandboxRoot, SandboxSpec};
pub use task_repository::{
    AgentRunEnd, AgentRunStart, ApprovalInfo, BatchItemRow, BatchRow, HistoryRemoved, NewBatch,
    NewWorkflowRun, PauseRecord, PausedTaskRow, PersistedTaskSummary, ResolvedApprovalRow,
    TaskDetail, TaskRepoError, TaskRepository, WorkflowRunRow, WorkflowStepEnd, WorkflowStepRow,
};
pub use tool_registry::{
    load_governance_snapshot, CredentialEntry, GovernanceSnapshot, ToolCredentialStore,
    ToolGovernanceError, ToolRegistry as NativeToolRegistry, ToolStatus,
    AGENT_CREDENTIALS_NAMESPACE, NATIVE_TOOL_NAMES,
};
pub use tools::bash_executor::{BashExecutor, BashExecutorError, BashInput, BashOutput};
pub use tools::bash_validator::BashValidator;
pub use tools::notebook_edit::{
    NotebookEdit, NotebookEditError, NotebookEditInput, NotebookEditOutput,
};
pub use tools::notebook_read::{
    NotebookRead, NotebookReadError, NotebookReadInput, NotebookReadOutput,
};
pub use tools::risk_classifier::{FilesystemOp, RiskCategory, RiskClassifier, RiskLevel};
#[cfg(feature = "web-search")]
pub use tools::web_search::{ToolConfigError, WebSearch, WebSearchError};
pub use trusted_keys::{TrustedKey, TrustedKeyError, TrustedKeyOrigin, TrustedKeyRepository};
