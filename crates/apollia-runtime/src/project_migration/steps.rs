//! The steps of one run of the migration, project by project.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use apollia_core::folder_connection::FolderConnectionError;
use apollia_core::paths::{DataFile, SOURCES_DIR_NAME};
use apollia_memory::export::{export_namespace, import_namespace, ImportMode};
use apollia_tools::{OverrideField, ProjectDetail};
use serde_json::Value;

use super::{
    agents_of, document_problem, ensure_file_source, ensure_folder_connection, folder_name_of,
    group_of, IssueItem, IssueReason, MemoryMerge, MigrationIssue, MigrationReport,
    MigrationStatus, ProjectOutcome, Stores, INSTRUCTIONS_FILE_NAME, PROJECTS_SOURCES_DIR,
    REPORT_FORMAT_VERSION,
};
use crate::chat::repository::ChatSessionRepository;

/// One run: the data directory it works on and the report it fills.
pub(super) struct Run<'a> {
    data_dir: &'a Path,
    report: MigrationReport,
}

impl<'a> Run<'a> {
    pub(super) fn new(data_dir: &'a Path) -> Self {
        Self {
            data_dir,
            report: MigrationReport {
                format_version: REPORT_FORMAT_VERSION,
                ..MigrationReport::default()
            },
        }
    }

    /// Every project, then the conversations.
    pub(super) fn all(&mut self) {
        let stores = match Stores::open(self.data_dir) {
            Ok(stores) => stores,
            Err(issue) => {
                self.report.issues.push(issue);
                return;
            }
        };
        let projects = match stores.projects.list_projects() {
            Ok(projects) => projects,
            Err(e) => {
                self.store_failed(DataFile::Projects.file_name(), e.to_string());
                return;
            }
        };
        let mut folder_names: HashSet<String> = HashSet::new();
        for summary in projects {
            let project = match stores.projects.get_project(&summary.id) {
                Ok(project) => project,
                Err(e) => {
                    self.store_failed(DataFile::Projects.file_name(), e.to_string());
                    continue;
                }
            };
            let mut folder = folder_name_of(&project);
            if !folder_names.insert(folder.clone()) {
                folder = format!("{folder} ({})", project.id);
                folder_names.insert(folder.clone());
            }
            let outcome = self.project(&stores, &project, &folder);
            self.report.projects.push(outcome);
        }
        self.conversations();
    }

    /// The report, its status set from the issues it holds.
    pub(super) fn into_report(mut self) -> MigrationReport {
        self.report.status = if self.report.issues.iter().any(|i| i.blocking) {
            MigrationStatus::Partial
        } else {
            MigrationStatus::Complete
        };
        self.report.ran_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        self.report
    }

    fn project(
        &mut self,
        stores: &Stores,
        project: &ProjectDetail,
        folder: &str,
    ) -> ProjectOutcome {
        let group = group_of(project);
        let mut outcome = ProjectOutcome {
            id: project.id.clone(),
            name: project.name.clone(),
            ..ProjectOutcome::default()
        };
        for document in &project.documents {
            if let Some((reason, detail)) = document_problem(&document.file_path) {
                self.issue(
                    project,
                    IssueItem::Document,
                    &document.file_path,
                    (reason, detail),
                );
                continue;
            }
            match ensure_file_source(&stores.sources, &document.name, &group, &document.file_path) {
                Ok(id) => outcome.document_sources.push(id),
                Err(e) => self.issue(
                    project,
                    IssueItem::Document,
                    &document.file_path,
                    (IssueReason::Failed, Some(e.to_string())),
                ),
            }
        }
        outcome.instructions_source = self.instructions(stores, project, &group, folder);
        let mut checked = outcome.document_sources.clone();
        checked.extend(outcome.instructions_source.clone());
        if !checked.is_empty() {
            outcome.agents_checked = self.check_for_agents(stores, project, &checked);
        }
        outcome.folder_connection = self.working_folder(stores, project);
        outcome.memory = self.memory(project);
        outcome
    }

    /// Writes the instructions of `project` to their file and makes it a
    /// source; `None` when the project has none or the write failed.
    fn instructions(
        &mut self,
        stores: &Stores,
        project: &ProjectDetail,
        group: &str,
        folder: &str,
    ) -> Option<String> {
        let text = project.instructions.as_deref().map(str::trim)?;
        if text.is_empty() {
            return None;
        }
        let path = self.instructions_path(folder);
        let subject = path.display().to_string();
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                let unchanged = std::fs::read_to_string(&path).is_ok_and(|old| old == text);
                if unchanged {
                    Ok(())
                } else {
                    std::fs::write(&path, text)
                }
            });
        if let Err(e) = written {
            self.issue(
                project,
                IssueItem::Instructions,
                &subject,
                (IssueReason::Failed, Some(e.to_string())),
            );
            return None;
        }
        match ensure_file_source(&stores.sources, INSTRUCTIONS_FILE_NAME, group, &subject) {
            Ok(id) => Some(id),
            Err(e) => {
                self.issue(
                    project,
                    IssueItem::Instructions,
                    &subject,
                    (IssueReason::Failed, Some(e.to_string())),
                );
                None
            }
        }
    }

    fn instructions_path(&self, folder: &str) -> PathBuf {
        self.data_dir
            .join(SOURCES_DIR_NAME)
            .join(PROJECTS_SOURCES_DIR)
            .join(folder)
            .join(INSTRUCTIONS_FILE_NAME)
    }

    /// Checks `sources` for the Apollia agent and the agents of `project`,
    /// keeping what each had checked; returns the agents now checking them.
    fn check_for_agents(
        &mut self,
        stores: &Stores,
        project: &ProjectDetail,
        sources: &[String],
    ) -> Vec<String> {
        let mut checked = Vec::new();
        for agent in agents_of(project) {
            let resolved =
                stores
                    .agents
                    .resolved(&agent, None, crate::agents::global_default_autonomy());
            let current = match resolved {
                Ok(Some(resolved)) => resolved.sources,
                Ok(None) => {
                    self.issue(
                        project,
                        IssueItem::Agent,
                        &agent,
                        (IssueReason::AgentNotInstalled, None),
                    );
                    continue;
                }
                Err(e) => {
                    self.issue(
                        project,
                        IssueItem::Agent,
                        &agent,
                        (IssueReason::Failed, Some(e.to_string())),
                    );
                    continue;
                }
            };
            let mut list = current.clone();
            for id in sources {
                if !list.contains(id) {
                    list.push(id.clone());
                }
            }
            if list != current {
                let value = Value::from(list);
                if let Err(e) = stores
                    .agents
                    .set_override(&agent, OverrideField::Sources, &value)
                {
                    self.issue(
                        project,
                        IssueItem::Agent,
                        &agent,
                        (IssueReason::Failed, Some(e.to_string())),
                    );
                    continue;
                }
            }
            checked.push(agent);
        }
        checked
    }

    /// Makes the working folder of `project` a folder connection, granted to
    /// no agent.
    fn working_folder(&mut self, stores: &Stores, project: &ProjectDetail) -> Option<String> {
        let path = project.workspace_path.as_deref().map(str::trim)?;
        if path.is_empty() {
            return None;
        }
        match ensure_folder_connection(&stores.folders, path, &group_of(project)) {
            Ok(id) => Some(id),
            Err(FolderConnectionError::NotAFolder { .. }) => {
                let reason = if Path::new(path).exists() {
                    IssueReason::WrongKind
                } else {
                    IssueReason::Missing
                };
                self.issue(project, IssueItem::Folder, path, (reason, None));
                None
            }
            Err(e) => {
                self.issue(
                    project,
                    IssueItem::Folder,
                    path,
                    (IssueReason::Failed, Some(e.to_string())),
                );
                None
            }
        }
    }

    /// Merges every memory space `<project id>:<space>` into `<space>`.
    fn memory(&mut self, project: &ProjectDetail) -> Vec<MemoryMerge> {
        let dir = crate::memory_maintenance::memory_dir(self.data_dir);
        let prefix = format!("{}:", project.id);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut spaces: Vec<String> = entries
            .flatten()
            .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
            .filter_map(|name| name.strip_suffix(".db").map(str::to_owned))
            .filter(|stem| stem.starts_with(&prefix) && stem.len() > prefix.len())
            .collect();
        spaces.sort();
        let mut merged = Vec::new();
        for from in spaces {
            let Some(into) = from.strip_prefix(&prefix).map(str::to_owned) else {
                continue;
            };
            let outcome = export_namespace(&dir, &from)
                .and_then(|export| import_namespace(&dir, &into, &export, ImportMode::Merge));
            match outcome {
                Ok(entries_added) => merged.push(MemoryMerge {
                    from,
                    into,
                    entries_added,
                }),
                Err(e) => self.issue(
                    project,
                    IssueItem::Memory,
                    &from,
                    (IssueReason::Failed, Some(e.to_string())),
                ),
            }
        }
        merged
    }

    /// Detaches every conversation from its project. A free conversation is
    /// the Apollia agent's; a conversation with an agent keeps it.
    fn conversations(&mut self) {
        let path = DataFile::Chat.path(self.data_dir);
        if !path.exists() {
            return;
        }
        let outcome =
            ChatSessionRepository::open(&path).and_then(|repo| repo.detach_all_projects());
        match outcome {
            Ok(count) => self.report.conversations_detached = count,
            Err(e) => self.report.issues.push(MigrationIssue {
                item: IssueItem::Conversations,
                subject: DataFile::Chat.file_name().to_owned(),
                reason: IssueReason::Failed,
                detail: Some(e.to_string()),
                blocking: true,
                ..MigrationIssue::default()
            }),
        }
    }

    fn store_failed(&mut self, subject: &str, detail: String) {
        self.report.issues.push(MigrationIssue {
            item: IssueItem::Store,
            subject: subject.to_owned(),
            reason: IssueReason::Failed,
            detail: Some(detail),
            blocking: true,
            ..MigrationIssue::default()
        });
    }

    fn issue(
        &mut self,
        project: &ProjectDetail,
        item: IssueItem,
        subject: &str,
        (reason, detail): (IssueReason, Option<String>),
    ) {
        let blocking = reason == IssueReason::Failed;
        tracing::event!(
            tracing::Level::WARN,
            project_id = %project.id,
            item = ?item,
            reason = ?reason,
            blocking,
            "project_migration.item.skipped"
        );
        self.report.issues.push(MigrationIssue {
            project_id: Some(project.id.clone()),
            project_name: Some(project.name.clone()),
            item,
            subject: subject.to_owned(),
            reason,
            detail,
            blocking,
        });
    }
}
