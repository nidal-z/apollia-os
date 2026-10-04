//! The migration of the former projects into sources, at boot.
//!
//! A project held documents (a path and a size each), instructions, linked
//! agents, a working folder, its own memory spaces and its conversations. The
//! migration carries each of them over to what replaces projects:
//!
//! - every readable document becomes a `file` source, grouped under the
//!   project's name;
//! - the instructions are written to
//!   `<data_dir>/sources/projets/<name>/instructions.md`, a `file` source of
//!   the same group: searchable when the agent looks for them, never injected;
//! - these sources are checked for the Apollia agent and for every agent the
//!   project linked, through their `sources` override;
//! - the working folder becomes a folder connection, granted to no agent;
//! - a project whose context came from its git state, its rules or its tree
//!   gets a `code_folder` source on that connection, checked like its files:
//!   the agent reads it through `code_context` when it needs it; the `script`
//!   and `style` providers have no replacement and are listed;
//! - every memory space `<project id>:<space>` is merged into `<space>` by the
//!   memory import in merge mode;
//! - every conversation loses its project and stays with its agent: a free
//!   conversation is the Apollia agent's; the chat database no longer has
//!   a project column.
//!
//! What cannot be carried over is listed in the report, never dropped in
//! silence. The report is written beside the sources
//! ([`report_path`]); `GET /api/v1/sources/project-migration` and
//! `apollia-os source report` read it.
//!
//! Every step is idempotent: a second run over the same state finds every
//! source, check, connection and memory entry already in place and adds
//! nothing. The migration runs at each boot that finds `projects.db` until a
//! run completes; a complete run is not replayed, and `projects.db` is
//! removed after it, never before: a partial run leaves it for the next
//! boot.

use std::path::{Path, PathBuf};

use apollia_core::folder_connection::{FolderConnectionError, FolderConnectionStore};
use apollia_core::paths::{DataFile, SOURCES_DIR_NAME};
use apollia_core::source_registry::{NewSource, SourceKind, SourceRegistryStore};
use apollia_tools::{AgentRepository, ProjectDetail, ProjectRepository, APOLLIA_AGENT_ID};
use serde::{Deserialize, Serialize};

mod steps;

/// Base name of the report, in the sources directory.
pub const REPORT_FILE_NAME: &str = "project-migration.json";

/// Directory, under the sources directory, holding one folder per project
/// for the files the migration writes (its instructions).
pub const PROJECTS_SOURCES_DIR: &str = "projets";

/// Base name of the file that holds the instructions of a project.
pub const INSTRUCTIONS_FILE_NAME: &str = "instructions.md";

/// Version of the report format.
pub const REPORT_FORMAT_VERSION: u32 = 1;

/// The report of the migration under `data_dir`.
pub fn report_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SOURCES_DIR_NAME).join(REPORT_FILE_NAME)
}

/// Where a run of the migration ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStatus {
    /// Everything that could be carried over was; the issues left, if any,
    /// are documents or folders that no longer exist, or agents no longer
    /// installed.
    Complete,
    /// A step failed: `projects.db` is kept and the next boot runs the
    /// migration again.
    #[default]
    Partial,
}

/// What an issue is about.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IssueItem {
    /// The project database itself, or a store the migration writes to.
    #[default]
    Store,
    /// A document of a project.
    Document,
    /// The instructions of a project.
    Instructions,
    /// An agent the sources are checked for.
    Agent,
    /// The working folder of a project.
    Folder,
    /// A context provider of a project.
    Provider,
    /// A memory space of a project.
    Memory,
    /// The conversations.
    Conversations,
}

/// Why an item was not carried over.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IssueReason {
    /// The file or folder is no longer there.
    Missing,
    /// The file is there but cannot be read.
    Unreadable,
    /// The path names something that is not a file, or not a folder.
    WrongKind,
    /// The agent is no longer installed.
    AgentNotInstalled,
    /// A store refused the write or could not be read.
    #[default]
    Failed,
    /// Nothing replaces it: a `script` or `style` provider, or a code
    /// provider of a project without a working folder.
    NotCarriedOver,
}

/// One item the migration did not carry over.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MigrationIssue {
    /// Identifier of the project, if the issue belongs to one.
    pub project_id: Option<String>,
    /// Name of the project, if the issue belongs to one.
    pub project_name: Option<String>,
    /// What the issue is about.
    pub item: IssueItem,
    /// The path, agent or space concerned.
    pub subject: String,
    /// Why it was not carried over.
    pub reason: IssueReason,
    /// The underlying error, when there is one.
    pub detail: Option<String>,
    /// Whether the issue keeps the migration partial. A missing document is
    /// listed and does not: running again would not bring it back.
    pub blocking: bool,
}

/// A memory space of a project merged into the space of the same name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MemoryMerge {
    /// The space of the project, `<project id>:<space>`.
    pub from: String,
    /// The space it was merged into.
    pub into: String,
    /// Entries this run added; 0 when they were already there.
    pub entries_added: usize,
}

/// What the migration did for one project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ProjectOutcome {
    /// Identifier of the project.
    pub id: String,
    /// Name of the project, which is also the group of its sources.
    pub name: String,
    /// Identifiers of the sources made of its documents.
    pub document_sources: Vec<String>,
    /// Identifier of the source made of its instructions, if it had any.
    pub instructions_source: Option<String>,
    /// Agents its sources are checked for.
    pub agents_checked: Vec<String>,
    /// Identifier of the folder connection made of its working folder.
    pub folder_connection: Option<String>,
    /// Identifier of the code folder source made on that connection, when
    /// the project read its git state, its rules or its tree.
    pub code_folder_source: Option<String>,
    /// Its memory spaces, merged.
    pub memory: Vec<MemoryMerge>,
}

/// The report of a run of the migration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MigrationReport {
    /// Version of this format.
    pub format_version: u32,
    /// Where the run ended.
    pub status: MigrationStatus,
    /// When the run ended, RFC 3339 in UTC.
    pub ran_at: String,
    /// One entry per project found.
    pub projects: Vec<ProjectOutcome>,
    /// Conversations detached from a project by this run.
    pub conversations_detached: usize,
    /// What was not carried over.
    pub issues: Vec<MigrationIssue>,
}

/// Errors of reading or writing the report.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProjectMigrationError {
    /// The report could not be read or written.
    #[error("cannot access the migration report at {path}: {source}")]
    Io {
        /// Path of the report.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The report is not valid JSON.
    #[error("the migration report at {path} is not valid: {source}")]
    Json {
        /// Path of the report.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: serde_json::Error,
    },
}

/// The report under `data_dir`, `None` when no migration has run.
///
/// # Errors
///
/// [`ProjectMigrationError::Io`] when the file exists but cannot be read,
/// [`ProjectMigrationError::Json`] when it does not parse.
pub fn read_report(data_dir: &Path) -> Result<Option<MigrationReport>, ProjectMigrationError> {
    let path = report_path(data_dir);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(ProjectMigrationError::Io { path, source }),
    };
    serde_json::from_str(&raw)
        .map(Some)
        .map_err(|source| ProjectMigrationError::Json { path, source })
}

/// Writes `report` under `data_dir`, through a temporary file renamed in
/// place so that a reader never sees half a report.
///
/// # Errors
///
/// [`ProjectMigrationError::Io`] when the directory or the file cannot be
/// written, [`ProjectMigrationError::Json`] when the report does not
/// serialize.
pub fn write_report(
    data_dir: &Path,
    report: &MigrationReport,
) -> Result<(), ProjectMigrationError> {
    let path = report_path(data_dir);
    let io = |source| ProjectMigrationError::Io {
        path: path.clone(),
        source,
    };
    let body =
        serde_json::to_string_pretty(report).map_err(|source| ProjectMigrationError::Json {
            path: path.clone(),
            source,
        })?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, body).map_err(io)?;
    std::fs::rename(&tmp, &path).map_err(io)
}

/// Runs the migration when `projects.db` exists under `data_dir` and no run
/// has completed yet, writes its report and returns it. `None` when there is
/// nothing to migrate.
///
/// A complete run removes `projects.db` once its report is written; a
/// partial run leaves it, and the next boot runs again. A report that cannot
/// be read is treated as absent: the migration runs again, which changes
/// nothing that is already in place.
pub fn run_if_needed(data_dir: &Path) -> Option<MigrationReport> {
    if !DataFile::Projects.path(data_dir).exists() {
        return None;
    }
    if let Ok(Some(report)) = read_report(data_dir) {
        if report.status == MigrationStatus::Complete {
            remove_projects_db(data_dir);
            return Some(report);
        }
    }
    let report = migrate(data_dir);
    match write_report(data_dir, &report) {
        Ok(()) if report.status == MigrationStatus::Complete => remove_projects_db(data_dir),
        Ok(()) => {}
        Err(e) => tracing::event!(
            tracing::Level::WARN,
            error = %e,
            "project_migration.report.failed"
        ),
    }
    Some(report)
}

/// Removes `projects.db` and its journal files. A file that cannot be removed
/// is left for the next boot, which finds the complete report and tries again.
fn remove_projects_db(data_dir: &Path) {
    let db = DataFile::Projects.path(data_dir);
    let journal = |suffix: &str| {
        let mut name = db.clone().into_os_string();
        name.push(suffix);
        PathBuf::from(name)
    };
    for path in [journal("-wal"), journal("-shm"), db.clone()] {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %e,
                    "project_migration.projects_db.remove_failed"
                );
                return;
            }
        }
    }
    tracing::event!(
        tracing::Level::INFO,
        "project_migration.projects_db.removed"
    );
}

/// The boot step: [`run_if_needed`] off the async runtime, its outcome
/// logged. Never fails the boot.
pub async fn run_at_boot(data_dir: &Path) {
    let dir = data_dir.to_owned();
    match tokio::task::spawn_blocking(move || run_if_needed(&dir)).await {
        Ok(Some(report)) => {
            let level_complete = report.status == MigrationStatus::Complete;
            if level_complete {
                tracing::event!(
                    tracing::Level::INFO,
                    projects = report.projects.len(),
                    issues = report.issues.len(),
                    "project_migration.complete"
                );
            } else {
                tracing::event!(
                    tracing::Level::WARN,
                    projects = report.projects.len(),
                    issues = report.issues.len(),
                    "project_migration.partial"
                );
            }
        }
        Ok(None) => {}
        Err(e) => tracing::event!(
            tracing::Level::WARN,
            error = %e,
            "project_migration.interrupted"
        ),
    }
}

/// One run of the migration over the stores under `data_dir`, whatever an
/// earlier report says. Writes nothing to `projects.db` and leaves it in
/// place: removing it is [`run_if_needed`]'s, after a complete run.
pub fn migrate(data_dir: &Path) -> MigrationReport {
    let mut run = steps::Run::new(data_dir);
    run.all();
    run.into_report()
}

/// The stores a run writes to, opened once.
struct Stores {
    projects: ProjectRepository,
    sources: SourceRegistryStore,
    folders: FolderConnectionStore,
    agents: AgentRepository,
}

impl Stores {
    /// Opens the four stores, or answers the issue that stops the run.
    fn open(data_dir: &Path) -> Result<Self, MigrationIssue> {
        let failed = |subject: &str, detail: String| MigrationIssue {
            item: IssueItem::Store,
            subject: subject.to_owned(),
            reason: IssueReason::Failed,
            detail: Some(detail),
            blocking: true,
            ..MigrationIssue::default()
        };
        let projects = ProjectRepository::open(&DataFile::Projects.path(data_dir))
            .map_err(|e| failed(DataFile::Projects.file_name(), e.to_string()))?;
        let system = DataFile::System.path(data_dir);
        let sources = SourceRegistryStore::open(&system)
            .map_err(|e| failed(DataFile::System.file_name(), e.to_string()))?;
        let folders = FolderConnectionStore::open(&system)
            .map_err(|e| failed(DataFile::System.file_name(), e.to_string()))?;
        let agents = AgentRepository::open(&DataFile::Agents.path(data_dir))
            .map_err(|e| failed(DataFile::Agents.file_name(), e.to_string()))?;
        Ok(Self {
            projects,
            sources,
            folders,
            agents,
        })
    }
}

/// The name of the folder that holds the files of `project`, safe on every
/// file system: separators and reserved characters become `_`, and a name
/// left empty falls back to the project's identifier.
fn folder_name_of(project: &ProjectDetail) -> String {
    let cleaned: String = project
        .name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        project.id.clone()
    } else {
        trimmed.to_owned()
    }
}

/// The group the sources of `project` are filed under: its name, or its
/// identifier when the name is blank.
fn group_of(project: &ProjectDetail) -> String {
    let name = project.name.trim();
    if name.is_empty() {
        project.id.clone()
    } else {
        name.to_owned()
    }
}

/// Why the document at `path` cannot become a source, `None` when it can.
fn document_problem(path: &str) -> Option<(IssueReason, Option<String>)> {
    let path = Path::new(path);
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Some((IssueReason::Missing, None))
        }
        Err(e) => return Some((IssueReason::Unreadable, Some(e.to_string()))),
    };
    if !meta.is_file() {
        return Some((IssueReason::WrongKind, None));
    }
    match std::fs::File::open(path) {
        Ok(_) => None,
        Err(e) => Some((IssueReason::Unreadable, Some(e.to_string()))),
    }
}

/// Adds a `file` source labelled `label` in `group` pointing at `reference`,
/// unless one is already there, and returns its identifier.
fn ensure_file_source(
    store: &SourceRegistryStore,
    label: &str,
    group: &str,
    reference: &str,
) -> Result<String, apollia_core::source_registry::SourceRegistryError> {
    let existing = store.list()?.into_iter().find(|s| {
        s.kind == SourceKind::File
            && s.group.as_deref() == Some(group)
            && s.reference.as_deref() == Some(reference)
    });
    if let Some(source) = existing {
        return Ok(source.id);
    }
    store
        .create(&NewSource {
            kind: SourceKind::File,
            label: label.to_owned(),
            group: Some(group.to_owned()),
            reference: Some(reference.to_owned()),
        })
        .map(|source| source.id)
}

/// Adds a `code_folder` source labelled `label` in `group` on the folder
/// connection `connection`, unless one is already there, and returns its
/// identifier.
fn ensure_code_folder_source(
    store: &SourceRegistryStore,
    label: &str,
    group: &str,
    connection: &str,
) -> Result<String, apollia_core::source_registry::SourceRegistryError> {
    let existing = store
        .list()?
        .into_iter()
        .find(|s| s.kind == SourceKind::CodeFolder && s.reference.as_deref() == Some(connection));
    if let Some(source) = existing {
        return Ok(source.id);
    }
    store
        .create(&NewSource {
            kind: SourceKind::CodeFolder,
            label: label.to_owned(),
            group: Some(group.to_owned()),
            reference: Some(connection.to_owned()),
        })
        .map(|source| source.id)
}

/// The providers whose context `code_context` now reads on demand.
const CODE_PROVIDERS: [&str; 3] = ["git", "rules", "tree"];

/// Adds the folder at `path` as a connection labelled `label`, or finds the
/// one already there, and returns its identifier.
fn ensure_folder_connection(
    store: &FolderConnectionStore,
    path: &str,
    label: &str,
) -> Result<String, FolderConnectionError> {
    match store.add(path, Some(label)) {
        Ok(folder) => Ok(folder.id),
        Err(FolderConnectionError::AlreadyAdded { path: canonical }) => store
            .list()?
            .into_iter()
            .find(|f| f.path == canonical)
            .map(|f| f.id)
            .ok_or(FolderConnectionError::NotFound(canonical)),
        Err(e) => Err(e),
    }
}

/// The agents the sources of `project` are checked for: the Apollia agent,
/// then each agent the project linked, once.
fn agents_of(project: &ProjectDetail) -> Vec<String> {
    let mut agents = vec![APOLLIA_AGENT_ID.to_owned()];
    for agent in &project.agents {
        if !agents.contains(agent) {
            agents.push(agent.clone());
        }
    }
    agents
}

#[cfg(test)]
mod tests;
