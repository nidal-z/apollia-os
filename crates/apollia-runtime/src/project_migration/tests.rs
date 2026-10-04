use std::path::{Path, PathBuf};

use apollia_core::folder_connection::FolderConnectionStore;
use apollia_core::paths::DataFile;
use apollia_core::source_registry::{Source, SourceKind, SourceRegistryStore};
use apollia_memory::export::{export_namespace, import_namespace, ImportMode, MemoryExport};
use apollia_tools::{AgentOrigin, AgentRepository, ProjectRepository, StoredAgent};
use serde_json::json;

use super::*;
use crate::chat::repository::ChatSessionRepository;

/// A data directory holding one project, `Dossier Durand`, and what it
/// points at, in a second directory standing for the user's files.
struct Fixture {
    data: tempfile::TempDir,
    files: tempfile::TempDir,
    project_id: String,
}

impl Fixture {
    fn data_dir(&self) -> &Path {
        self.data.path()
    }

    fn file(&self, name: &str) -> String {
        self.files.path().join(name).display().to_string()
    }

    fn sources(&self) -> Vec<Source> {
        SourceRegistryStore::open(&DataFile::System.path(self.data_dir()))
            .unwrap()
            .list()
            .unwrap()
    }

    fn checked_by(&self, agent: &str) -> Vec<String> {
        AgentRepository::open(&DataFile::Agents.path(self.data_dir()))
            .unwrap()
            .resolved(agent, None, crate::agents::global_default_autonomy())
            .unwrap()
            .unwrap()
            .sources
    }

    fn session_projects(&self) -> Vec<(String, Option<String>, Option<String>)> {
        let conn = rusqlite::Connection::open(DataFile::Chat.path(self.data_dir())).unwrap();
        let mut stmt = conn
            .prepare("SELECT id, agent_name, project_id FROM chat_sessions ORDER BY id")
            .unwrap();
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    fn memory_semantic(&self, namespace: &str) -> usize {
        export_namespace(
            &crate::memory_maintenance::memory_dir(self.data_dir()),
            namespace,
        )
        .unwrap()
        .semantic
        .len()
    }
}

fn memory_export(namespace: &str, ids: &[&str]) -> MemoryExport {
    MemoryExport {
        format_version: 1,
        namespace: namespace.to_owned(),
        exported_at: "2026-10-04T00:00:00Z".into(),
        episodic: Vec::new(),
        semantic: ids
            .iter()
            .map(|id| {
                json!({
                    "id": id, "key": id, "value": "\"v\"", "confidence": 1.0,
                    "created_at": "2026-10-04T00:00:00Z", "updated_at": "2026-10-04T00:00:00Z",
                })
            })
            .collect(),
        procedural: Vec::new(),
    }
}

/// The project has two readable documents, one gone, instructions, the
/// installed agent `veille` and the uninstalled `ancien`, its working folder,
/// one memory space, and two conversations: a free one and one with `veille`.
fn fixture() -> Fixture {
    let data = tempfile::tempdir().unwrap();
    let files = tempfile::tempdir().unwrap();
    std::fs::write(files.path().join("contrat.pdf"), b"%PDF-1.4").unwrap();
    std::fs::write(files.path().join("notes.txt"), "notes").unwrap();
    let mut fx = Fixture {
        data,
        files,
        project_id: String::new(),
    };

    let projects = ProjectRepository::open(&DataFile::Projects.path(fx.data_dir())).unwrap();
    let id = projects
        .create_project(
            "Dossier Durand",
            None,
            Some("Always address the client formally.".into()),
            Some(fx.files.path().display().to_string()),
        )
        .unwrap();
    projects
        .add_document(&id, "contrat.pdf", fx.file("contrat.pdf"), 8)
        .unwrap();
    projects
        .add_document(&id, "notes.txt", fx.file("notes.txt"), 5)
        .unwrap();
    projects
        .add_document(&id, "parti.txt", fx.file("parti.txt"), 3)
        .unwrap();
    projects.add_agent(&id, "veille").unwrap();
    projects.add_agent(&id, "ancien").unwrap();

    AgentRepository::open(&DataFile::Agents.path(fx.data_dir()))
        .unwrap()
        .save_definition(&StoredAgent {
            name: "veille".into(),
            origin: AgentOrigin::Package,
            package: Some("presse".into()),
            version: "1.0.0".into(),
            definition: json!({ "id": "veille", "name": "Veille", "defaults": { "sources": ["profile"] } }),
            enabled: true,
            installed_at: "2026-10-04T00:00:00Z".into(),
            updated_at: "2026-10-04T00:00:00Z".into(),
        })
        .unwrap();

    let memory = crate::memory_maintenance::memory_dir(fx.data_dir());
    import_namespace(
        &memory,
        &format!("{id}:veille"),
        &memory_export("x", &["m-projet"]),
        ImportMode::Merge,
    )
    .unwrap();
    import_namespace(
        &memory,
        "veille",
        &memory_export("x", &["m-global"]),
        ImportMode::Merge,
    )
    .unwrap();

    drop(ChatSessionRepository::open(&DataFile::Chat.path(fx.data_dir())).unwrap());
    let conn = rusqlite::Connection::open(DataFile::Chat.path(fx.data_dir())).unwrap();
    conn.execute(
        "INSERT INTO chat_sessions (id, mode, agent_name, created_at, project_id) VALUES
         ('s-libre', 'libre', NULL, '2026-10-04T00:00:00Z', ?1),
         ('s-agent', 'agent', 'veille', '2026-10-04T00:00:00Z', ?1)",
        [&id],
    )
    .unwrap();

    fx.project_id = id;
    fx
}

fn instructions_path(fx: &Fixture) -> PathBuf {
    fx.data_dir()
        .join("sources")
        .join("projets")
        .join("Dossier Durand")
        .join("instructions.md")
}

#[test]
fn test_each_readable_document_becomes_a_file_source_grouped_under_the_project() {
    // GIVEN a project with two readable documents
    let fx = fixture();

    // WHEN the migration runs
    let report = migrate(fx.data_dir());

    // THEN each document is a file source of the group `Dossier Durand`
    let documents: Vec<_> = fx
        .sources()
        .into_iter()
        .filter(|s| s.label != INSTRUCTIONS_FILE_NAME)
        .collect();
    assert_eq!(documents.len(), 2);
    for source in &documents {
        assert_eq!(source.kind, SourceKind::File);
        assert_eq!(source.group.as_deref(), Some("Dossier Durand"));
    }
    let references: Vec<_> = documents
        .iter()
        .filter_map(|s| s.reference.clone())
        .collect();
    assert!(references.contains(&fx.file("contrat.pdf")));
    assert!(references.contains(&fx.file("notes.txt")));
    assert_eq!(report.projects[0].document_sources.len(), 2);
}

#[test]
fn test_the_instructions_become_a_file_source_of_the_same_group() {
    // GIVEN a project with instructions
    let fx = fixture();

    // WHEN the migration runs
    let report = migrate(fx.data_dir());

    // THEN they are written to instructions.md, a file source of the group
    let path = instructions_path(&fx);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "Always address the client formally."
    );
    let source = fx
        .sources()
        .into_iter()
        .find(|s| s.label == INSTRUCTIONS_FILE_NAME)
        .unwrap();
    assert_eq!(source.kind, SourceKind::File);
    assert_eq!(source.group.as_deref(), Some("Dossier Durand"));
    assert_eq!(source.reference, Some(path.display().to_string()));
    assert_eq!(report.projects[0].instructions_source, Some(source.id));
}

#[test]
fn test_the_sources_are_checked_for_apollia_and_the_linked_agents() {
    // GIVEN a project linked to `veille`, which already checks the profile
    let fx = fixture();

    // WHEN the migration runs
    let report = migrate(fx.data_dir());

    // THEN Apollia and `veille` check every source of the project, and
    // `veille` keeps the profile
    let ids: Vec<String> = fx.sources().into_iter().map(|s| s.id).collect();
    assert_eq!(ids.len(), 3);
    let apollia = fx.checked_by(APOLLIA_AGENT_ID);
    let veille = fx.checked_by("veille");
    for id in &ids {
        assert!(apollia.contains(id));
        assert!(veille.contains(id));
    }
    assert_eq!(veille[0], "profile");
    assert_eq!(
        report.projects[0].agents_checked,
        vec![APOLLIA_AGENT_ID.to_owned(), "veille".to_owned()]
    );
}

#[test]
fn test_conversations_lose_their_project_and_keep_their_agent() {
    // GIVEN a free conversation and a conversation with `veille`, both in the
    // project
    let fx = fixture();

    // WHEN the migration runs
    let report = migrate(fx.data_dir());

    // THEN neither has a project; the free one stays Apollia's, the other
    // stays with `veille`
    assert_eq!(
        fx.session_projects(),
        vec![
            ("s-agent".to_owned(), Some("veille".to_owned()), None),
            ("s-libre".to_owned(), None, None),
        ]
    );
    assert_eq!(report.conversations_detached, 2);
}

#[test]
fn test_the_memory_spaces_of_the_project_merge_into_their_space() {
    // GIVEN a project space `<id>:veille` with one entry, and `veille` with
    // another
    let fx = fixture();

    // WHEN the migration runs
    let report = migrate(fx.data_dir());

    // THEN `veille` holds both
    assert_eq!(fx.memory_semantic("veille"), 2);
    assert_eq!(
        report.projects[0].memory,
        vec![MemoryMerge {
            from: format!("{}:veille", fx.project_id),
            into: "veille".into(),
            entries_added: 1,
        }]
    );
}

#[test]
fn test_the_working_folder_becomes_a_folder_connection() {
    // GIVEN a project whose working folder exists
    let fx = fixture();

    // WHEN the migration runs
    let report = migrate(fx.data_dir());

    // THEN it is a folder connection, labelled after the project
    let folders = FolderConnectionStore::open(&DataFile::System.path(fx.data_dir()))
        .unwrap()
        .list()
        .unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].label, "Dossier Durand");
    assert_eq!(
        folders[0].path,
        std::fs::canonicalize(fx.files.path())
            .unwrap()
            .display()
            .to_string()
    );
    assert_eq!(
        report.projects[0].folder_connection,
        Some(folders[0].id.clone())
    );
}

#[test]
fn test_a_missing_document_or_folder_is_listed_never_dropped() {
    // GIVEN a project with a document gone, an uninstalled agent, and a
    // second project whose working folder and document are not what they say
    let fx = fixture();
    let projects = ProjectRepository::open(&DataFile::Projects.path(fx.data_dir())).unwrap();
    let other = projects
        .create_project("Other", None, None, Some(fx.file("contrat.pdf")))
        .unwrap();
    projects
        .add_document(&other, "dossier", fx.files.path().display().to_string(), 0)
        .unwrap();

    // WHEN the migration runs
    let report = migrate(fx.data_dir());

    // THEN each is in the report, none of them blocks the migration
    let listed: Vec<_> = report
        .issues
        .iter()
        .map(|i| (i.item, i.subject.clone(), i.reason, i.blocking))
        .collect();
    assert!(listed.contains(&(
        IssueItem::Document,
        fx.file("parti.txt"),
        IssueReason::Missing,
        false
    )));
    assert!(listed.contains(&(
        IssueItem::Agent,
        "ancien".to_owned(),
        IssueReason::AgentNotInstalled,
        false
    )));
    assert!(listed.contains(&(
        IssueItem::Document,
        fx.files.path().display().to_string(),
        IssueReason::WrongKind,
        false
    )));
    assert!(listed.contains(&(
        IssueItem::Folder,
        fx.file("contrat.pdf"),
        IssueReason::WrongKind,
        false
    )));
    assert_eq!(report.status, MigrationStatus::Complete);
}

#[cfg(unix)]
#[test]
fn test_an_unreadable_document_is_listed() {
    use std::os::unix::fs::PermissionsExt;

    // GIVEN a document the user can no longer read
    let fx = fixture();
    let path = fx.file("notes.txt");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    let readable_anyway = std::fs::File::open(&path).is_ok();

    // WHEN the migration runs
    let report = migrate(fx.data_dir());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

    // THEN it is listed as unreadable, unless the process reads it anyway
    // (a superuser does)
    let listed = report
        .issues
        .iter()
        .any(|i| i.subject == path && i.reason == IssueReason::Unreadable);
    assert_eq!(listed, !readable_anyway);
}

#[test]
fn test_a_partial_run_keeps_projects_db_and_a_second_run_completes_it() {
    // GIVEN a chat database that cannot be opened
    let fx = fixture();
    let chat = DataFile::Chat.path(fx.data_dir());
    std::fs::remove_file(&chat).unwrap();
    std::fs::write(&chat, b"not a database at all, not even close").unwrap();

    // WHEN the migration runs at boot, then again once the file is gone
    let first = run_if_needed(fx.data_dir()).unwrap();
    let kept = DataFile::Projects.path(fx.data_dir()).exists();
    let read_back = read_report(fx.data_dir()).unwrap().unwrap();
    let sources_after_first = fx.sources().len();
    std::fs::remove_file(&chat).unwrap();
    let second = run_if_needed(fx.data_dir()).unwrap();

    // THEN the first is partial, written, and leaves projects.db; the second
    // completes without adding a source twice
    assert_eq!(first.status, MigrationStatus::Partial);
    assert!(first
        .issues
        .iter()
        .any(|i| i.item == IssueItem::Conversations && i.blocking));
    assert!(kept);
    assert_eq!(read_back, first);
    assert_eq!(second.status, MigrationStatus::Complete);
    assert_eq!(fx.sources().len(), sources_after_first);
    assert!(DataFile::Projects.path(fx.data_dir()).exists());
}

#[test]
fn test_a_second_run_gives_the_same_state() {
    // GIVEN a data directory already migrated once
    let fx = fixture();
    let first = migrate(fx.data_dir());
    let state = |fx: &Fixture| {
        (
            fx.sources(),
            fx.checked_by(APOLLIA_AGENT_ID),
            fx.checked_by("veille"),
            FolderConnectionStore::open(&DataFile::System.path(fx.data_dir()))
                .unwrap()
                .list()
                .unwrap(),
            fx.memory_semantic("veille"),
            fx.session_projects(),
            std::fs::read_to_string(instructions_path(fx)).unwrap(),
        )
    };
    let before = state(&fx);

    // WHEN it runs again
    let second = migrate(fx.data_dir());

    // THEN nothing changed, and the report says the same but for what this
    // run added
    assert_eq!(state(&fx), before);
    assert_eq!(second.projects.len(), first.projects.len());
    assert_eq!(
        second.projects[0].document_sources,
        first.projects[0].document_sources
    );
    assert_eq!(second.projects[0].memory[0].entries_added, 0);
    assert_eq!(second.conversations_detached, 0);
    assert_eq!(second.status, first.status);
}

#[test]
fn test_a_complete_migration_is_not_replayed() {
    // GIVEN a completed migration, after which the user removed a source
    let fx = fixture();
    let first = run_if_needed(fx.data_dir()).unwrap();
    assert_eq!(first.status, MigrationStatus::Complete);
    let removed = fx.sources()[0].id.clone();
    SourceRegistryStore::open(&DataFile::System.path(fx.data_dir()))
        .unwrap()
        .delete(&removed)
        .unwrap();

    // WHEN the runtime boots again
    let again = run_if_needed(fx.data_dir()).unwrap();

    // THEN the report is the first one and the source stays removed
    assert_eq!(again, first);
    assert!(fx.sources().iter().all(|s| s.id != removed));
}

#[test]
fn test_nothing_runs_without_projects_db() {
    // GIVEN a data directory without projects.db
    let data = tempfile::tempdir().unwrap();

    // WHEN the boot step runs
    let report = run_if_needed(data.path());

    // THEN there is no report
    assert!(report.is_none());
    assert!(read_report(data.path()).unwrap().is_none());
}

#[test]
fn test_two_projects_of_the_same_name_keep_their_instructions_apart() {
    // GIVEN two projects named `Dossier Durand`, each with instructions
    let fx = fixture();
    ProjectRepository::open(&DataFile::Projects.path(fx.data_dir()))
        .unwrap()
        .create_project(
            "Dossier Durand",
            None,
            Some("Another instruction.".into()),
            None,
        )
        .unwrap();

    // WHEN the migration runs
    migrate(fx.data_dir());

    // THEN each has its own instructions file
    let dir = fx.data_dir().join("sources").join("projets");
    let mut written: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| std::fs::read_to_string(e.path().join(INSTRUCTIONS_FILE_NAME)).unwrap())
        .collect();
    written.sort();
    assert_eq!(
        written,
        vec![
            "Always address the client formally.".to_owned(),
            "Another instruction.".to_owned()
        ]
    );
}

/// Adds to the project of `fx` an enabled provider of each kind it had.
fn with_providers(fx: &Fixture, kinds: &[&str]) {
    let projects = ProjectRepository::open(&DataFile::Projects.path(fx.data_dir())).unwrap();
    for (priority, kind) in kinds.iter().enumerate() {
        let path = (*kind == "script").then(|| fx.file("context.sh"));
        projects
            .set_provider(
                None,
                &fx.project_id,
                *kind,
                format!("my {kind}"),
                "{}",
                path,
                true,
                u8::try_from(priority).unwrap(),
            )
            .unwrap();
    }
}

#[test]
fn test_a_project_reading_its_code_gets_a_code_folder_on_its_working_folder() {
    // GIVEN a project whose working folder fed the git, rules and tree
    // providers, and a script and a style provider beside them
    let fx = fixture();
    with_providers(&fx, &["git", "rules", "tree", "script", "style"]);

    // WHEN the migration runs
    let report = migrate(fx.data_dir());

    // THEN one code folder source points at the folder connection of the
    // working folder, checked for Apollia and the linked agent
    let outcome = &report.projects[0];
    let code: Vec<_> = fx
        .sources()
        .into_iter()
        .filter(|s| s.kind == SourceKind::CodeFolder)
        .collect();
    assert_eq!(code.len(), 1);
    assert_eq!(code[0].reference, outcome.folder_connection);
    assert_eq!(code[0].group.as_deref(), Some("Dossier Durand"));
    assert_eq!(outcome.code_folder_source, Some(code[0].id.clone()));
    assert!(fx.checked_by(APOLLIA_AGENT_ID).contains(&code[0].id));
    assert!(fx.checked_by("veille").contains(&code[0].id));

    // AND the script and the style are listed, never dropped, without
    // keeping the migration partial
    let providers: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.item == IssueItem::Provider)
        .collect();
    assert_eq!(providers.len(), 2, "{providers:?}");
    assert!(providers
        .iter()
        .all(|i| i.reason == IssueReason::NotCarriedOver && !i.blocking));
    assert!(providers.iter().any(|i| i.subject == "my script (script)"));
    assert!(providers.iter().any(|i| i.subject == "my style (style)"));
    assert_eq!(report.status, MigrationStatus::Complete);
}

#[test]
fn test_a_project_without_code_providers_gets_no_code_folder() {
    // GIVEN a project with a working folder and no provider
    let fx = fixture();

    // WHEN the migration runs
    let report = migrate(fx.data_dir());

    // THEN its folder is a connection and nothing more
    assert!(report.projects[0].folder_connection.is_some());
    assert_eq!(report.projects[0].code_folder_source, None);
    assert!(fx
        .sources()
        .iter()
        .all(|s| s.kind != SourceKind::CodeFolder));
}

#[test]
fn test_a_code_provider_without_a_working_folder_is_listed() {
    // GIVEN a project with a git provider and no working folder
    let fx = fixture();
    let projects = ProjectRepository::open(&DataFile::Projects.path(fx.data_dir())).unwrap();
    let bare = projects
        .create_project("No folder", None, None, None)
        .unwrap();
    projects
        .set_provider(None, &bare, "git", "git", "{}", None, true, 0)
        .unwrap();

    // WHEN the migration runs
    let report = migrate(fx.data_dir());

    // THEN the provider is listed for that project and no code folder is made
    let listed: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.project_id.as_deref() == Some(bare.as_str()))
        .collect();
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(listed[0].item, IssueItem::Provider);
    assert_eq!(listed[0].reason, IssueReason::NotCarriedOver);
    assert!(fx
        .sources()
        .iter()
        .all(|s| s.kind != SourceKind::CodeFolder));
}

#[test]
fn test_a_second_run_keeps_a_single_code_folder() {
    // GIVEN a project reading its code, migrated once
    let fx = fixture();
    with_providers(&fx, &["git"]);
    let first = migrate(fx.data_dir());

    // WHEN the migration runs again
    let second = migrate(fx.data_dir());

    // THEN the same code folder source is found, not a second one
    let code = fx
        .sources()
        .into_iter()
        .filter(|s| s.kind == SourceKind::CodeFolder)
        .count();
    assert_eq!(code, 1);
    assert_eq!(
        second.projects[0].code_folder_source,
        first.projects[0].code_folder_source
    );
}
