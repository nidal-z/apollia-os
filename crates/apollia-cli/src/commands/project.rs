//! `apollia-os project` subcommands — local-first project management.
//!
//! Operates directly on `~/.apollia/projects.db` through
//! [`apollia_tools::ProjectRepository`] without requiring the runtime.

use std::path::{Path, PathBuf};

use clap::Subcommand;

use apollia_tools::{ProjectPatch, ProjectRepository};

use crate::exit_codes;

/// Top-level subcommands of `apollia-os project`.
#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// List every registered project (alphabetical).
    List {
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },

    /// Create a new project and print its id.
    Create {
        /// Project display name.
        name: String,
        /// Optional one-line description.
        #[arg(long)]
        description: Option<String>,
        /// Optional initial instructions (Markdown).
        #[arg(long)]
        instructions: Option<String>,
        /// Optional workspace directory used by context providers.
        #[arg(long, value_name = "DIR")]
        workspace: Option<String>,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },

    /// Print the full detail of a project (documents, providers, agents).
    Show {
        /// Project id (UUID).
        id: String,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },

    /// Update one or more mutable fields on an existing project.
    Update {
        /// Project id.
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        instructions: Option<String>,
        #[arg(long, value_name = "DIR")]
        workspace: Option<String>,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },

    /// Delete a project and cascade its documents/providers.
    Delete {
        /// Project id.
        id: String,
        /// Skip the confirmation prompt.
        #[arg(long)]
        confirm: bool,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },

    /// List the agents linked to a project.
    Agents {
        /// Agent subcommand.
        #[command(subcommand)]
        command: ProjectAgentsCommand,
    },

    /// List or seed the available project templates.
    Templates {
        /// Templates subcommand.
        #[command(subcommand)]
        command: ProjectTemplatesCommand,
    },

    /// Link (or unlink) a chat session to a project.
    ///
    /// Writes `chat_sessions.project_id` directly via
    /// `apollia_runtime::chat::ChatSessionRepository` so the runtime does
    /// not need to be running. Pass `--unlink` to clear the session's
    /// project link instead of setting it; the project_id positional is
    /// then ignored.
    Link {
        /// Project id (UUID returned by `project list`).
        project_id: String,
        /// Chat session id (returned by `chat --list`).
        #[arg(long, value_name = "ID")]
        session: String,
        /// Clear the session's project_id instead of setting it.
        #[arg(long)]
        unlink: bool,
        /// Override the chat database path (default: `~/.apollia/chat.db`).
        #[arg(long, value_name = "PATH")]
        chat_db: Option<PathBuf>,
    },

    /// List chat sessions linked to a project.
    Chats {
        /// Project id (UUID).
        project_id: String,
        /// Override the chat database path.
        #[arg(long, value_name = "PATH")]
        chat_db: Option<PathBuf>,
    },
}

/// Subcommands of `apollia-os project agents`.
#[derive(Debug, Subcommand)]
pub enum ProjectAgentsCommand {
    /// List agent names linked to a project.
    List {
        /// Project id.
        project: String,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Link an agent to a project.
    Add {
        /// Project id.
        project: String,
        /// Agent name.
        agent: String,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Unlink an agent from a project.
    Remove {
        /// Project id.
        project: String,
        /// Agent name.
        agent: String,
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
}

/// Subcommands of `apollia-os project templates`.
#[derive(Debug, Subcommand)]
pub enum ProjectTemplatesCommand {
    /// List the available templates (builtin + custom).
    List {
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
    /// Re-seed the builtin templates into the database.
    SeedBuiltins {
        #[arg(long, value_name = "PATH")]
        db: Option<PathBuf>,
    },
}

/// Entry point for `apollia-os project <verb>`.
pub fn run(cmd: &ProjectCommand, json: bool) -> i32 {
    match cmd {
        ProjectCommand::List { db } => run_list(db.as_deref(), json),
        ProjectCommand::Create {
            name,
            description,
            instructions,
            workspace,
            db,
        } => run_create(
            db.as_deref(),
            name,
            description.clone(),
            instructions.clone(),
            workspace.clone(),
            json,
        ),
        ProjectCommand::Show { id, db } => run_show(db.as_deref(), id, json),
        ProjectCommand::Update {
            id,
            name,
            description,
            instructions,
            workspace,
            db,
        } => run_update(
            db.as_deref(),
            id,
            name.clone(),
            description.clone(),
            instructions.clone(),
            workspace.clone(),
            json,
        ),
        ProjectCommand::Delete { id, confirm, db } => run_delete(db.as_deref(), id, *confirm, json),
        ProjectCommand::Agents { command } => run_agents(command, json),
        ProjectCommand::Templates { command } => run_templates(command, json),
        ProjectCommand::Link {
            project_id,
            session,
            unlink,
            chat_db,
        } => run_link(project_id, session, *unlink, chat_db.as_deref(), json),
        ProjectCommand::Chats {
            project_id,
            chat_db,
        } => run_chats(project_id, chat_db.as_deref(), json),
    }
}

// ─── link / chats ────────────────────────────────────────────────────────────

fn resolve_chat_db(override_path: Option<&Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".apollia")
        .join("chat.db")
}

fn open_chat_repo(
    db: Option<&Path>,
    json: bool,
) -> Option<apollia_runtime::chat::ChatSessionRepository> {
    let path = resolve_chat_db(db);
    if !path.exists() {
        emit_error(
            format!(
                "chat database not found at {} (no chat session has ever been persisted)",
                path.display()
            ),
            json,
        );
        return None;
    }
    match apollia_runtime::chat::ChatSessionRepository::open(&path) {
        Ok(r) => Some(r),
        Err(e) => {
            emit_error(format!("open {} failed: {e}", path.display()), json);
            None
        }
    }
}

fn run_link(
    project_id: &str,
    session: &str,
    unlink: bool,
    chat_db: Option<&Path>,
    json: bool,
) -> i32 {
    if session.trim().is_empty() {
        emit_error("--session must not be empty", json);
        return exit_codes::GENERAL_ERROR;
    }
    let Some(repo) = open_chat_repo(chat_db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let target = if unlink {
        None
    } else if project_id.trim().is_empty() {
        emit_error("project_id must not be empty (use --unlink to clear)", json);
        return exit_codes::GENERAL_ERROR;
    } else {
        Some(project_id)
    };
    match repo.set_session_project(session, target) {
        Ok(()) => {
            if json {
                let body = serde_json::json!({
                    "session_id": session,
                    "project_id": target,
                    "unlinked": unlink,
                });
                println!("{}", serde_json::to_string_pretty(&body).unwrap_or_default());
            } else if unlink {
                println!("  * session '{session}' unlinked from any project");
            } else {
                println!("  * session '{session}' linked to project {project_id}");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("link failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

fn run_chats(project_id: &str, chat_db: Option<&Path>, json: bool) -> i32 {
    if project_id.trim().is_empty() {
        emit_error("project_id must not be empty", json);
        return exit_codes::GENERAL_ERROR;
    }
    let Some(repo) = open_chat_repo(chat_db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let sessions = match repo.list_sessions_by_project(project_id) {
        Ok(s) => s,
        Err(e) => {
            emit_error(format!("list_sessions_by_project: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };

    if json {
        let array: Vec<serde_json::Value> = sessions
            .iter()
            .map(|s| {
                serde_json::json!({
                    "id": s.id,
                    "title": s.title,
                    "mode": s.mode,
                    "status": s.status,
                    "agent_name": s.agent_name,
                    "created_at": s.created_at,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::Value::Array(array)).unwrap_or_default()
        );
    } else if sessions.is_empty() {
        println!("No chat sessions linked to project {project_id}.");
    } else {
        println!(
            "  Chat sessions linked to {project_id} ({}):",
            sessions.len()
        );
        println!(
            "  {:<24} {:<8} {:<10} {:<20} TITLE",
            "ID", "MODE", "STATUS", "CREATED_AT"
        );
        for s in &sessions {
            let title = s
                .title
                .as_deref()
                .filter(|t| !t.is_empty())
                .unwrap_or("(untitled)");
            println!(
                "  {:<24} {:<8} {:<10} {:<20} {}",
                s.id, s.mode, s.status, s.created_at, title
            );
        }
    }
    exit_codes::SUCCESS
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn resolve_db(db: Option<&Path>) -> PathBuf {
    if let Some(p) = db {
        return p.to_path_buf();
    }
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    home.join(".apollia").join("projects.db")
}

fn open_repo(db: Option<&Path>, json: bool) -> Option<ProjectRepository> {
    let path = resolve_db(db);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match ProjectRepository::open(&path) {
        Ok(r) => Some(r),
        Err(e) => {
            emit_error(format!("open {} failed: {e}", path.display()), json);
            None
        }
    }
}

fn emit_error(msg: impl Into<String>, json: bool) {
    let s = msg.into();
    if json {
        let out = serde_json::json!({"error": s});
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
    } else {
        eprintln!("Error: {s}");
    }
}

// ─── list ─────────────────────────────────────────────────────────────────────

fn run_list(db: Option<&Path>, json: bool) -> i32 {
    let Some(repo) = open_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let projects = match repo.list_projects() {
        Ok(v) => v,
        Err(e) => {
            emit_error(format!("list failed: {e}"), json);
            return exit_codes::GENERAL_ERROR;
        }
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&projects).unwrap_or_default()
        );
    } else if projects.is_empty() {
        println!("  (no projects)");
    } else {
        println!("  {:<38} {:<24} {}", "ID", "NAME", "WORKSPACE");
        for p in &projects {
            let ws = p.workspace_path.as_deref().unwrap_or("-");
            println!("  {:<38} {:<24} {}", p.id, p.name, ws);
        }
    }
    exit_codes::SUCCESS
}

// ─── create ───────────────────────────────────────────────────────────────────

fn run_create(
    db: Option<&Path>,
    name: &str,
    description: Option<String>,
    instructions: Option<String>,
    workspace: Option<String>,
    json: bool,
) -> i32 {
    let Some(repo) = open_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match repo.create_project(name, description, instructions, workspace) {
        Ok(id) => {
            if json {
                let out = serde_json::json!({"id": id, "name": name});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * created '{name}' -> {id}");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("create failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

// ─── show ─────────────────────────────────────────────────────────────────────

fn run_show(db: Option<&Path>, id: &str, json: bool) -> i32 {
    let Some(repo) = open_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match repo.get_project(id) {
        Ok(detail) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&detail).unwrap_or_default()
                );
            } else {
                println!("  ID           {}", detail.id);
                println!("  Name         {}", detail.name);
                if let Some(d) = &detail.description {
                    println!("  Description  {d}");
                }
                if let Some(w) = &detail.workspace_path {
                    println!("  Workspace    {w}");
                }
                println!("  Documents    {}", detail.documents.len());
                println!("  Providers    {}", detail.providers.len());
                println!("  Agents       {}", detail.agents.join(", "));
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("show failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

// ─── update ───────────────────────────────────────────────────────────────────

fn run_update(
    db: Option<&Path>,
    id: &str,
    name: Option<String>,
    description: Option<String>,
    instructions: Option<String>,
    workspace: Option<String>,
    json: bool,
) -> i32 {
    if name.is_none() && description.is_none() && instructions.is_none() && workspace.is_none() {
        emit_error(
            "provide at least one of --name, --description, --instructions, --workspace",
            json,
        );
        return exit_codes::GENERAL_ERROR;
    }
    let Some(repo) = open_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    let patch = ProjectPatch {
        name,
        description: description.map(Some),
        instructions: instructions.map(Some),
        workspace_path: workspace.map(Some),
    };
    match repo.update_project(id, patch) {
        Ok(true) => {
            if json {
                let out = serde_json::json!({"id": id, "updated": true});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * updated {id}");
            }
            exit_codes::SUCCESS
        }
        Ok(false) => {
            emit_error(format!("project '{id}' not found"), json);
            exit_codes::GENERAL_ERROR
        }
        Err(e) => {
            emit_error(format!("update failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

// ─── delete ───────────────────────────────────────────────────────────────────

fn run_delete(db: Option<&Path>, id: &str, confirm: bool, json: bool) -> i32 {
    if !confirm {
        emit_error(format!("use --confirm to delete '{id}'"), json);
        return exit_codes::GENERAL_ERROR;
    }
    let Some(repo) = open_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match repo.delete_project(id) {
        Ok(true) => {
            if json {
                let out = serde_json::json!({"id": id, "deleted": true});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * deleted {id}");
            }
            exit_codes::SUCCESS
        }
        Ok(false) => {
            emit_error(format!("project '{id}' not found"), json);
            exit_codes::GENERAL_ERROR
        }
        Err(e) => {
            emit_error(format!("delete failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

// ─── agents ───────────────────────────────────────────────────────────────────

fn run_agents(cmd: &ProjectAgentsCommand, json: bool) -> i32 {
    match cmd {
        ProjectAgentsCommand::List { project, db } => run_agents_list(db.as_deref(), project, json),
        ProjectAgentsCommand::Add { project, agent, db } => {
            run_agents_add(db.as_deref(), project, agent, json)
        }
        ProjectAgentsCommand::Remove { project, agent, db } => {
            run_agents_remove(db.as_deref(), project, agent, json)
        }
    }
}

fn run_agents_list(db: Option<&Path>, project: &str, json: bool) -> i32 {
    let Some(repo) = open_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match repo.list_agents(project) {
        Ok(agents) => {
            if json {
                let out = serde_json::json!({"project": project, "agents": agents});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else if agents.is_empty() {
                println!("  (no agents linked)");
            } else {
                for a in &agents {
                    println!("  * {a}");
                }
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("list_agents failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

fn run_agents_add(db: Option<&Path>, project: &str, agent: &str, json: bool) -> i32 {
    let Some(repo) = open_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match repo.add_agent(project, agent) {
        Ok(()) => {
            if json {
                let out = serde_json::json!({"project": project, "added": agent});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * linked {agent} -> {project}");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("add_agent failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

fn run_agents_remove(db: Option<&Path>, project: &str, agent: &str, json: bool) -> i32 {
    let Some(repo) = open_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match repo.remove_agent(project, agent) {
        Ok(true) => {
            if json {
                let out = serde_json::json!({"project": project, "removed": agent});
                println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
            } else {
                println!("  * unlinked {agent} from {project}");
            }
            exit_codes::SUCCESS
        }
        Ok(false) => {
            emit_error(format!("agent '{agent}' not linked to '{project}'"), json);
            exit_codes::GENERAL_ERROR
        }
        Err(e) => {
            emit_error(format!("remove_agent failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

// ─── templates ────────────────────────────────────────────────────────────────

fn run_templates(cmd: &ProjectTemplatesCommand, json: bool) -> i32 {
    match cmd {
        ProjectTemplatesCommand::List { db } => run_templates_list(db.as_deref(), json),
        ProjectTemplatesCommand::SeedBuiltins { db } => run_templates_seed(db.as_deref(), json),
    }
}

fn run_templates_list(db: Option<&Path>, json: bool) -> i32 {
    let Some(repo) = open_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match repo.list_templates() {
        Ok(templates) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&templates).unwrap_or_default()
                );
            } else if templates.is_empty() {
                println!("  (no templates — run `apollia-os project templates seed-builtins`)");
            } else {
                println!("  {:<24} {:<10} {}", "ID", "KIND", "NAME");
                for t in &templates {
                    let kind = if t.is_builtin { "builtin" } else { "custom" };
                    println!("  {:<24} {:<10} {}", t.id, kind, t.name);
                }
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("list_templates failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

fn run_templates_seed(db: Option<&Path>, json: bool) -> i32 {
    let Some(repo) = open_repo(db, json) else {
        return exit_codes::GENERAL_ERROR;
    };
    match repo.seed_builtin_templates() {
        Ok(()) => {
            if json {
                println!("{{\"seeded\":true}}");
            } else {
                println!("  * builtin templates seeded");
            }
            exit_codes::SUCCESS
        }
        Err(e) => {
            emit_error(format!("seed failed: {e}"), json);
            exit_codes::GENERAL_ERROR
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        cmd: ProjectCommand,
    }

    #[test]
    fn parses_list() {
        let cli = TestCli::parse_from(["x", "list"]);
        assert!(matches!(cli.cmd, ProjectCommand::List { .. }));
    }

    #[test]
    fn parses_create_minimal() {
        let cli = TestCli::parse_from(["x", "create", "Acme"]);
        match cli.cmd {
            ProjectCommand::Create { name, .. } => assert_eq!(name, "Acme"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_create_with_workspace() {
        let cli = TestCli::parse_from(["x", "create", "X", "--workspace", "/srv/x"]);
        match cli.cmd {
            ProjectCommand::Create { workspace, .. } => {
                assert_eq!(workspace.as_deref(), Some("/srv/x"))
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_show() {
        let cli = TestCli::parse_from(["x", "show", "abc"]);
        match cli.cmd {
            ProjectCommand::Show { id, .. } => assert_eq!(id, "abc"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_agents_add() {
        let cli = TestCli::parse_from(["x", "agents", "add", "p", "a"]);
        match cli.cmd {
            ProjectCommand::Agents {
                command: ProjectAgentsCommand::Add { project, agent, .. },
            } => {
                assert_eq!(project, "p");
                assert_eq!(agent, "a");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn create_then_show_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("p.db");
        let code = run_create(
            Some(&db),
            "Acme",
            Some("desc".to_string()),
            None,
            None,
            true,
        );
        assert_eq!(code, exit_codes::SUCCESS);
        // list should now have one entry
        assert_eq!(run_list(Some(&db), true), exit_codes::SUCCESS);
    }

    #[test]
    fn delete_without_confirm_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("p.db");
        assert_eq!(
            run_delete(Some(&db), "doesnotmatter", false, true),
            exit_codes::GENERAL_ERROR
        );
    }

    #[test]
    fn update_with_no_fields_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("p.db");
        assert_eq!(
            run_update(Some(&db), "abc", None, None, None, None, true),
            exit_codes::GENERAL_ERROR
        );
    }

    #[test]
    fn agents_add_then_list_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("p.db");
        let repo = ProjectRepository::open(&db).unwrap();
        let pid = repo.create_project("X", None, None, None).unwrap();
        assert_eq!(
            run_agents_add(Some(&db), &pid, "agent-a", true),
            exit_codes::SUCCESS
        );
        assert_eq!(run_agents_list(Some(&db), &pid, true), exit_codes::SUCCESS);
    }

    #[test]
    fn parses_link_with_unlink() {
        use clap::Parser;
        #[derive(Parser, Debug)]
        struct TestCli {
            #[command(subcommand)]
            cmd: ProjectCommand,
        }
        let cli =
            TestCli::parse_from(["x", "link", "proj-uuid", "--session", "sess-id", "--unlink"]);
        match cli.cmd {
            ProjectCommand::Link {
                project_id,
                session,
                unlink,
                ..
            } => {
                assert_eq!(project_id, "proj-uuid");
                assert_eq!(session, "sess-id");
                assert!(unlink);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_link_default() {
        use clap::Parser;
        #[derive(Parser, Debug)]
        struct TestCli {
            #[command(subcommand)]
            cmd: ProjectCommand,
        }
        let cli = TestCli::parse_from(["x", "link", "proj-uuid", "--session", "sess"]);
        match cli.cmd {
            ProjectCommand::Link {
                project_id,
                session,
                unlink,
                ..
            } => {
                assert_eq!(project_id, "proj-uuid");
                assert_eq!(session, "sess");
                assert!(!unlink);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn parses_chats() {
        use clap::Parser;
        #[derive(Parser, Debug)]
        struct TestCli {
            #[command(subcommand)]
            cmd: ProjectCommand,
        }
        let cli = TestCli::parse_from(["x", "chats", "proj-uuid"]);
        match cli.cmd {
            ProjectCommand::Chats { project_id, .. } => assert_eq!(project_id, "proj-uuid"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn link_rejects_missing_chat_db() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does_not_exist.db");
        let code = run_link("p", "s", false, Some(&missing), true);
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }

    #[test]
    fn chats_rejects_missing_chat_db() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does_not_exist.db");
        let code = run_chats("p", Some(&missing), true);
        assert_eq!(code, exit_codes::GENERAL_ERROR);
    }
}
