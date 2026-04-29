//! Apollia OS — Tool Registry and native tools.
//!
//! Provides the tooling infrastructure for agents:
//! - `ToolRegistry` — in-memory catalogue of available tools
//! - `ToolResolver` — validates tool availability at INITIALIZING
//! - `SandboxProfile` — Linux namespace isolation profiles
//! - `AuditTrail` — SQLite-persisted tool invocation log
//! - `TaskRepository` — SQLite-persisted HITL task state
//! - `AgentRepository` — SQLite-persisted installed agents
//! - `executor` — [`ToolExecutor`] trait and [`ToolDispatcher`] for unified JSON dispatch
//!
//! Native tools:
//! - `bash_executor` — sandboxed shell execution via unshare(1)
//! - `python_executor` — isolated virtualenv execution
//! - `file_read`, `file_write`, `file_list`, `file_edit`, `file_glob`, `file_grep` — atomic filesystem operations
//! - `http_fetch` — network-restricted HTTP client (feature `http`)
//! - `memory_search` — FTS5 full-text search (feature `memory-search`)
//! - `persistent_bash` — stateful shell executor (CWD + env vars persist across steps)
//! - `notebook_read` — read and format Jupyter `.ipynb` cells for LLM consumption
//! - `notebook_edit` — apply atomic cell operations to Jupyter `.ipynb` notebooks

pub mod agent_repository;
pub mod audit;
pub mod chat_libre_config;
pub mod descriptor;
pub mod executor;
pub mod file_path_extractor;
pub mod governance_db;
pub mod journal;
pub mod native_dispatcher;
pub mod package_repository;
pub mod project_repository;
pub mod registry;
pub mod resolver;
pub mod sandbox_path;
pub mod ssrf;
pub mod task_repository;
pub mod tool_registry;
pub mod tools;

pub use agent_repository::{AgentRepository, AgentRepositoryError, InstalledAgent};
pub use audit::{
    compute_input_hash, AuditStats, AuditTrailError, AuditTrailHandle, ToolInvocationRecord,
};
pub use descriptor::{
    ApprovalRiskLevel, McpTransport, ToolDescriptor, ToolDescriptorError, ToolKind,
};
pub use executor::{
    SessionToolFilter, ToolBatchCall, ToolDispatcher, ToolExecutionError, ToolExecutor,
};
pub use file_path_extractor::FilePathExtractor;
pub use governance_db::{
    GovernanceDb, GovernanceError, GOVERNANCE_DB_FILENAME, LEGACY_BACKUP_FILENAME,
    LEGACY_PERMISSIONS_FILENAME,
};
pub use journal::{
    list_sessions, rollback_session, JournalEntry, JournalError, JournalWriter, JournalWriterHandle,
};
pub use native_dispatcher::{build_native_dispatcher, NativeDispatcherConfig};
pub use package_repository::{InstalledPackage, PackageRepository, PackageRepositoryError};
pub use project_repository::{
    ProjectDetail, ProjectDocument, ProjectPatch, ProjectProviderRow, ProjectRepository,
    ProjectRepositoryError, ProjectSummary, ProjectTemplate,
};
pub use registry::{ToolRegistryError, ToolRegistryHandle};
pub use resolver::{resolve, ResolutionReport, ResolutionStatus, ToolResolutionError};
pub use sandbox_path::{SandboxPathError, SandboxRoot};
pub use task_repository::{
    ApprovalInfo, PersistedTaskSummary, ResolvedApprovalRow, TaskDetail, TaskRepoError,
    TaskRepository,
};
pub use tool_registry::{
    load_governance_snapshot, CredentialEntry, GovernanceSnapshot, ToolCredentialStore,
    ToolGovernanceError, ToolRegistry as NativeToolRegistry, ToolStatus, NATIVE_TOOL_NAMES,
};
pub use tools::bash_executor::{BashExecutor, BashExecutorError, BashInput, BashOutput};
pub use tools::bash_validator::BashValidator;
pub use tools::notebook_edit::{
    NotebookEdit, NotebookEditError, NotebookEditInput, NotebookEditOutput,
};
pub use tools::notebook_read::{
    NotebookRead, NotebookReadError, NotebookReadInput, NotebookReadOutput,
};
pub use tools::persistent_bash::{
    registry::ShellSessionRegistry,
    session::{PersistentBashError, SessionId, ShellOutput},
    PersistentBashExecutor,
};
pub use tools::risk_classifier::{FilesystemOp, RiskCategory, RiskClassifier, RiskLevel};
#[cfg(feature = "web-search")]
pub use tools::web_search::{ToolConfigError, WebSearch, WebSearchError};
