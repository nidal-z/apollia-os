//! One parsing test per leaf command that had none.
//!
//! `crates/apollia-cli/AGENTS.md` section 4 states that every sub-command has
//! parsing tests. Sixty-one of the 199 leaves the binary publishes had no
//! `parse_from` sequence anywhere in the crate, so the statement held for the
//! total (247 sequences, target 150+) and not for the leaves. The sequences
//! below close that gap; they drive the real top-level `Cli`, so a leaf whose
//! path, flag name or argument arity moves fails here rather than in a user's
//! shell. `scripts/check_cli_parse_tests.py` keeps the floor at zero.

use clap::Parser;

use crate::commands::a2a::A2aCommand;
use crate::commands::agent::{AgentCommand, PackageCommand};
use crate::commands::audit::AuditCommand;
use crate::commands::chat::ChatHygieneCommand;
use crate::commands::chat_config::ChatConfigCommand;
use crate::commands::config::ConfigCommand;
use crate::commands::digest::DigestWindow;
use crate::commands::llm::{LlmBackendsCommand, LlmCommand};
use crate::commands::mcp::McpCommand;
use crate::commands::memory::{MemoryCommand, MemoryType};
use crate::commands::permissions::PermissionsCommand;
use crate::commands::project::{ProjectAgentsCommand, ProjectCommand, ProjectTemplatesCommand};
use crate::commands::task::TaskCommand;
use crate::commands::tools::{
    ToolsApprovalsCmd, ToolsCommand, ToolsConfigCmd, ToolsCredentialsCmd,
};
use crate::commands::user_memory::UserMemoryCommand;
use crate::{Cli, Commands};

fn parse(args: &[&str]) -> Cli {
    Cli::parse_from(args)
}

// ─── agent ───────────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_agent_package_list() {
    // GIVEN "apollia-os agent package list"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "agent", "package", "list"]);
    // THEN the package sub-command is List
    let Commands::Agent {
        command: AgentCommand::Package { cmd },
    } = cli.command
    else {
        panic!("expected agent package");
    };
    assert!(matches!(cmd, PackageCommand::List));
}

#[test]
fn test_cli_parses_agent_package_show() {
    // GIVEN "apollia-os agent package show invoice-reader"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "agent", "package", "show", "invoice-reader"]);
    // THEN the package name is carried through
    let Commands::Agent {
        command: AgentCommand::Package { cmd },
    } = cli.command
    else {
        panic!("expected agent package");
    };
    let PackageCommand::Show { name } = cmd else {
        panic!("expected package show");
    };
    assert_eq!(name, "invoice-reader");
}

#[test]
fn test_cli_parses_agent_package_uninstall_with_confirm() {
    // GIVEN "apollia-os agent package uninstall invoice-reader --confirm"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "agent",
        "package",
        "uninstall",
        "invoice-reader",
        "--confirm",
    ]);
    // THEN the name is carried and the destruction flag is set
    let Commands::Agent {
        command: AgentCommand::Package { cmd },
    } = cli.command
    else {
        panic!("expected agent package");
    };
    let PackageCommand::Uninstall { name, confirm } = cmd else {
        panic!("expected package uninstall");
    };
    assert_eq!(name, "invoice-reader");
    assert!(confirm);
}

#[test]
fn test_cli_parses_agent_repair() {
    // GIVEN "apollia-os agent repair invoice-reader"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "agent", "repair", "invoice-reader"]);
    // THEN AgentCommand::Repair carries the agent name
    let Commands::Agent {
        command: AgentCommand::Repair { name },
    } = cli.command
    else {
        panic!("expected agent repair");
    };
    assert_eq!(name, "invoice-reader");
}

// ─── a2a ─────────────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_a2a_skills() {
    // GIVEN "apollia-os a2a skills"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "a2a", "skills"]);
    // THEN A2aCommand::Skills, which takes no argument
    assert!(matches!(
        cli.command,
        Commands::A2a {
            command: A2aCommand::Skills
        }
    ));
}

#[test]
fn test_cli_parses_a2a_invoke_with_inline_args() {
    // GIVEN "apollia-os a2a invoke pdf.read_text --args {\"path\":\"a.pdf\"} --timeout 30"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "a2a",
        "invoke",
        "pdf.read_text",
        "--args",
        "{\"path\":\"a.pdf\"}",
        "--timeout",
        "30",
    ]);
    // THEN the skill id, the inline payload and the timeout are carried
    let Commands::A2a {
        command:
            A2aCommand::Invoke {
                skill_id,
                args,
                args_file,
                timeout,
                caller,
            },
    } = cli.command
    else {
        panic!("expected a2a invoke");
    };
    assert_eq!(skill_id, "pdf.read_text");
    assert_eq!(args.as_deref(), Some("{\"path\":\"a.pdf\"}"));
    assert!(args_file.is_none());
    assert_eq!(timeout, Some(30));
    assert!(caller.is_none());
}

// ─── task ────────────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_task_resume_approve() {
    // GIVEN "apollia-os task resume t-0042 --approve"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "task", "resume", "t-0042", "--approve"]);
    // THEN the decision group carries approve, not reject
    let Commands::Task {
        command:
            TaskCommand::Resume {
                task_id,
                approve,
                reject,
                reason,
            },
    } = cli.command
    else {
        panic!("expected task resume");
    };
    assert_eq!(task_id, "t-0042");
    assert!(approve);
    assert!(!reject);
    assert!(reason.is_none());
}

#[test]
fn test_cli_parses_task_approvals_pending() {
    // GIVEN "apollia-os task approvals --pending"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "task", "approvals", "--pending"]);
    // THEN TaskCommand::Approvals with the filter set
    let Commands::Task {
        command: TaskCommand::Approvals { pending },
    } = cli.command
    else {
        panic!("expected task approvals");
    };
    assert!(pending);
}

// ─── tools ───────────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_tools_enable() {
    // GIVEN "apollia-os tools enable web_search"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "tools", "enable", "web_search"]);
    // THEN ToolsCommand::Enable carries the tool name
    let Commands::Tools {
        command: ToolsCommand::Enable { name },
    } = cli.command
    else {
        panic!("expected tools enable");
    };
    assert_eq!(name, "web_search");
}

#[test]
fn test_cli_parses_tools_disable() {
    // GIVEN "apollia-os tools disable web_search"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "tools", "disable", "web_search"]);
    // THEN ToolsCommand::Disable carries the tool name
    let Commands::Tools {
        command: ToolsCommand::Disable { name },
    } = cli.command
    else {
        panic!("expected tools disable");
    };
    assert_eq!(name, "web_search");
}

#[test]
fn test_cli_parses_tools_config_get() {
    // GIVEN "apollia-os tools config get web_search"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "tools", "config", "get", "web_search"]);
    // THEN the nested config sub-command is Get
    let Commands::Tools {
        command: ToolsCommand::Config { command },
    } = cli.command
    else {
        panic!("expected tools config");
    };
    let ToolsConfigCmd::Get { name } = command else {
        panic!("expected tools config get");
    };
    assert_eq!(name, "web_search");
}

#[test]
fn test_cli_parses_tools_config_set() {
    // GIVEN "apollia-os tools config set web_search.backend duckduckgo"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "tools",
        "config",
        "set",
        "web_search.backend",
        "duckduckgo",
    ]);
    // THEN the dotted key and the value are two positional arguments
    let Commands::Tools {
        command: ToolsCommand::Config { command },
    } = cli.command
    else {
        panic!("expected tools config");
    };
    let ToolsConfigCmd::Set { key_path, value } = command else {
        panic!("expected tools config set");
    };
    assert_eq!(key_path, "web_search.backend");
    assert_eq!(value, "duckduckgo");
}

#[test]
fn test_cli_parses_tools_reload() {
    // GIVEN "apollia-os tools reload"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "tools", "reload"]);
    // THEN ToolsCommand::Reload, which takes no argument
    assert!(matches!(
        cli.command,
        Commands::Tools {
            command: ToolsCommand::Reload
        }
    ));
}

#[test]
fn test_cli_parses_tools_credentials_list_filtered() {
    // GIVEN "apollia-os tools credentials list web_search"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "tools", "credentials", "list", "web_search"]);
    // THEN the optional tool filter is carried
    let Commands::Tools {
        command: ToolsCommand::Credentials { command },
    } = cli.command
    else {
        panic!("expected tools credentials");
    };
    let ToolsCredentialsCmd::List { tool } = command else {
        panic!("expected tools credentials list");
    };
    assert_eq!(tool.as_deref(), Some("web_search"));
}

#[test]
fn test_cli_parses_tools_credentials_set() {
    // GIVEN "apollia-os tools credentials set web_search brave.api_key"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "tools",
        "credentials",
        "set",
        "web_search",
        "brave.api_key",
    ]);
    // THEN the owning tool and the logical key are two positional arguments
    let Commands::Tools {
        command: ToolsCommand::Credentials { command },
    } = cli.command
    else {
        panic!("expected tools credentials");
    };
    let ToolsCredentialsCmd::Set { tool, key } = command else {
        panic!("expected tools credentials set");
    };
    assert_eq!(tool, "web_search");
    assert_eq!(key, "brave.api_key");
}

#[test]
fn test_cli_parses_tools_credentials_delete_with_confirm() {
    // GIVEN "apollia-os tools credentials delete web_search brave.api_key --confirm"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "tools",
        "credentials",
        "delete",
        "web_search",
        "brave.api_key",
        "--confirm",
    ]);
    // THEN the pair is carried and the destruction flag is set
    let Commands::Tools {
        command: ToolsCommand::Credentials { command },
    } = cli.command
    else {
        panic!("expected tools credentials");
    };
    let ToolsCredentialsCmd::Delete { tool, key, confirm } = command else {
        panic!("expected tools credentials delete");
    };
    assert_eq!(tool, "web_search");
    assert_eq!(key, "brave.api_key");
    assert!(confirm);
}

#[test]
fn test_cli_parses_tools_credentials_test() {
    // GIVEN "apollia-os tools credentials test web_search"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "tools", "credentials", "test", "web_search"]);
    // THEN ToolsCredentialsCmd::Test carries the tool name
    let Commands::Tools {
        command: ToolsCommand::Credentials { command },
    } = cli.command
    else {
        panic!("expected tools credentials");
    };
    let ToolsCredentialsCmd::Test { tool } = command else {
        panic!("expected tools credentials test");
    };
    assert_eq!(tool, "web_search");
}

#[test]
fn test_cli_parses_tools_approvals_pending() {
    // GIVEN "apollia-os tools approvals pending"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "tools", "approvals", "pending"]);
    // THEN the nested approvals sub-command is Pending
    let Commands::Tools {
        command: ToolsCommand::Approvals { command },
    } = cli.command
    else {
        panic!("expected tools approvals");
    };
    assert!(matches!(command, ToolsApprovalsCmd::Pending));
}

#[test]
fn test_cli_parses_tools_approvals_resolved_with_window() {
    // GIVEN "apollia-os tools approvals resolved --days 3 --limit 5"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "tools",
        "approvals",
        "resolved",
        "--days",
        "3",
        "--limit",
        "5",
    ]);
    // THEN the window overrides the defaults of 7 days and 50 entries
    let Commands::Tools {
        command: ToolsCommand::Approvals { command },
    } = cli.command
    else {
        panic!("expected tools approvals");
    };
    let ToolsApprovalsCmd::Resolved { days, limit } = command else {
        panic!("expected tools approvals resolved");
    };
    assert_eq!(days, 3);
    assert_eq!(limit, 5);
}

// ─── audit ───────────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_audit_journal_with_page() {
    // GIVEN "apollia-os audit journal --limit 5 --offset 10"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "audit",
        "journal",
        "--limit",
        "5",
        "--offset",
        "10",
    ]);
    // THEN both paging arguments override their defaults
    let Commands::Audit {
        command: AuditCommand::Journal { limit, offset },
    } = cli.command
    else {
        panic!("expected audit journal");
    };
    assert_eq!(limit, 5);
    assert_eq!(offset, 10);
}

#[test]
fn test_cli_parses_audit_show() {
    // GIVEN "apollia-os audit show run-0042"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "audit", "show", "run-0042"]);
    // THEN the run or task identifier is carried
    let Commands::Audit {
        command: AuditCommand::Show { run },
    } = cli.command
    else {
        panic!("expected audit show");
    };
    assert_eq!(run, "run-0042");
}

// ─── memory ──────────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_memory_list_for_one_agent() {
    // GIVEN "apollia-os memory list --agent invoice-reader"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "memory", "list", "--agent", "invoice-reader"]);
    // THEN the agent filter is carried and the data directory stays default
    let Commands::Memory {
        command: MemoryCommand::List { agent, data_dir },
    } = cli.command
    else {
        panic!("expected memory list");
    };
    assert_eq!(agent.as_deref(), Some("invoice-reader"));
    assert!(data_dir.is_none());
}

#[test]
fn test_cli_parses_memory_clear_with_confirm() {
    // GIVEN "apollia-os memory clear --agent invoice-reader --type episodic --confirm"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "memory",
        "clear",
        "--agent",
        "invoice-reader",
        "--type",
        "episodic",
        "--confirm",
    ]);
    // THEN the agent, the memory type and the destruction flag are carried
    let Commands::Memory {
        command:
            MemoryCommand::Clear {
                agent,
                r#type,
                confirm,
                data_dir,
            },
    } = cli.command
    else {
        panic!("expected memory clear");
    };
    assert_eq!(agent, "invoice-reader");
    assert!(matches!(r#type, MemoryType::Episodic));
    assert!(confirm);
    assert!(data_dir.is_none());
}

#[test]
fn test_cli_parses_memory_purge_with_age() {
    // GIVEN "apollia-os memory purge --namespace invoice-reader --older-than 30 --confirm"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "memory",
        "purge",
        "--namespace",
        "invoice-reader",
        "--older-than",
        "30",
        "--confirm",
    ]);
    // THEN the namespace, the age threshold and the destruction flag are carried
    let Commands::Memory {
        command:
            MemoryCommand::Purge {
                namespace,
                older_than,
                r#type,
                data_dir,
                confirm,
            },
    } = cli.command
    else {
        panic!("expected memory purge");
    };
    assert_eq!(namespace, "invoice-reader");
    assert_eq!(older_than, 30);
    assert!(r#type.is_none());
    assert!(data_dir.is_none());
    assert!(confirm);
}

#[test]
fn test_cli_parses_memory_learn_procedure_with_inline_steps() {
    // GIVEN "apollia-os memory learn-procedure --namespace n --trigger t --steps s"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "memory",
        "learn-procedure",
        "--namespace",
        "invoice-reader",
        "--trigger",
        "invoice received",
        "--steps",
        "read, extract, file",
    ]);
    // THEN the inline steps satisfy the requirement that --file would also meet
    let Commands::Memory {
        command:
            MemoryCommand::LearnProcedure {
                namespace,
                trigger,
                steps,
                file,
                data_dir,
            },
    } = cli.command
    else {
        panic!("expected memory learn-procedure");
    };
    assert_eq!(namespace, "invoice-reader");
    assert_eq!(trigger, "invoice received");
    assert_eq!(steps.as_deref(), Some("read, extract, file"));
    assert!(file.is_none());
    assert!(data_dir.is_none());
}

#[test]
fn test_cli_parses_memory_export_to_file() {
    // GIVEN "apollia-os memory export --namespace invoice-reader --output dump.json"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "memory",
        "export",
        "--namespace",
        "invoice-reader",
        "--output",
        "dump.json",
    ]);
    // THEN the namespace and the destination file are carried
    let Commands::Memory {
        command:
            MemoryCommand::Export {
                namespace,
                output,
                data_dir,
            },
    } = cli.command
    else {
        panic!("expected memory export");
    };
    assert_eq!(namespace, "invoice-reader");
    assert_eq!(output, Some(std::path::PathBuf::from("dump.json")));
    assert!(data_dir.is_none());
}

#[test]
fn test_cli_parses_memory_import_merging() {
    // GIVEN "apollia-os memory import --namespace n --input dump.json --merge"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "memory",
        "import",
        "--namespace",
        "invoice-reader",
        "--input",
        "dump.json",
        "--merge",
    ]);
    // THEN merge is set and replace, which conflicts with it, is not
    let Commands::Memory {
        command:
            MemoryCommand::Import {
                namespace,
                input,
                replace,
                merge,
                data_dir,
            },
    } = cli.command
    else {
        panic!("expected memory import");
    };
    assert_eq!(namespace, "invoice-reader");
    assert_eq!(input, std::path::PathBuf::from("dump.json"));
    assert!(!replace);
    assert!(merge);
    assert!(data_dir.is_none());
}

// ─── llm ─────────────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_llm_backends_show() {
    // GIVEN "apollia-os llm backends show local"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "llm", "backends", "show", "local"]);
    // THEN the nested backends sub-command carries the backend name
    let Commands::Llm {
        command: LlmCommand::Backends { command },
    } = cli.command
    else {
        panic!("expected llm backends");
    };
    let LlmBackendsCommand::Show { name } = command else {
        panic!("expected llm backends show");
    };
    assert_eq!(name, "local");
}

#[test]
fn test_cli_parses_llm_reload() {
    // GIVEN "apollia-os llm reload"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "llm", "reload"]);
    // THEN LlmCommand::Reload, which takes no argument
    assert!(matches!(
        cli.command,
        Commands::Llm {
            command: LlmCommand::Reload
        }
    ));
}

// ─── permissions ─────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_permissions_list_scoped() {
    // GIVEN "apollia-os permissions list --scope global --tool bash"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "permissions",
        "list",
        "--scope",
        "global",
        "--tool",
        "bash",
    ]);
    // THEN both filters are carried
    let Commands::Permissions {
        command: PermissionsCommand::List { scope, tool },
    } = cli.command
    else {
        panic!("expected permissions list");
    };
    assert_eq!(scope.as_deref(), Some("global"));
    assert_eq!(tool.as_deref(), Some("bash"));
}

#[test]
fn test_cli_parses_permissions_revoke_one_with_yes() {
    // GIVEN "apollia-os permissions revoke perm-7 --yes"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "permissions", "revoke", "perm-7", "--yes"]);
    // THEN the identifier is carried, --all stays unset, --yes is the flag this
    // leaf published before the --confirm rule
    let Commands::Permissions {
        command:
            PermissionsCommand::Revoke {
                id,
                all,
                scope,
                yes,
            },
    } = cli.command
    else {
        panic!("expected permissions revoke");
    };
    assert_eq!(id.as_deref(), Some("perm-7"));
    assert!(!all);
    assert!(scope.is_none());
    assert!(yes);
}

#[test]
fn test_cli_parses_permissions_audit_filtered() {
    // GIVEN "apollia-os permissions audit --tool bash --limit 5"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "permissions",
        "audit",
        "--tool",
        "bash",
        "--limit",
        "5",
    ]);
    // THEN the tool filter is carried and the limit overrides its default of 50
    let Commands::Permissions {
        command: PermissionsCommand::Audit { tool, limit },
    } = cli.command
    else {
        panic!("expected permissions audit");
    };
    assert_eq!(tool.as_deref(), Some("bash"));
    assert_eq!(limit, 5);
}

// ─── chat config ─────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_chat_config_reset_with_confirm() {
    // GIVEN "apollia-os chat config reset --confirm"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "chat", "config", "reset", "--confirm"]);
    // THEN the chat sub-command nests the chat-config reset with its flag
    let Commands::Chat {
        resume,
        list,
        command,
    } = cli.command
    else {
        panic!("expected chat");
    };
    assert!(resume.is_none());
    assert!(!list);
    let Some(ChatHygieneCommand::Config { command }) = command else {
        panic!("expected chat config");
    };
    let ChatConfigCommand::Reset { confirm, db } = command else {
        panic!("expected chat config reset");
    };
    assert!(confirm);
    assert!(db.is_none());
}

// ─── mcp ─────────────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_mcp_list_with_discovery() {
    // GIVEN "apollia-os mcp list --discover"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "mcp", "list", "--discover"]);
    // THEN the mDNS scan flag is set and the config path stays default
    let Commands::Mcp {
        command:
            McpCommand::List {
                discover,
                config,
                json,
            },
    } = cli.command
    else {
        panic!("expected mcp list");
    };
    assert!(discover);
    assert!(config.is_none());
    assert!(!json);
}

#[test]
fn test_cli_parses_mcp_set_approval_with_ttl() {
    // GIVEN "apollia-os mcp set-approval files read_file --ttl-hours 1"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "mcp",
        "set-approval",
        "files",
        "read_file",
        "--ttl-hours",
        "1",
    ]);
    // THEN the server and tool are positional and the TTL overrides its default
    let Commands::Mcp {
        command:
            McpCommand::SetApproval {
                server,
                tool,
                db,
                ttl_hours,
                json,
            },
    } = cli.command
    else {
        panic!("expected mcp set-approval");
    };
    assert_eq!(server, "files");
    assert_eq!(tool, "read_file");
    assert!(db.is_none());
    assert_eq!(ttl_hours, 1);
    assert!(!json);
}

#[test]
fn test_cli_parses_mcp_list_pending() {
    // GIVEN "apollia-os mcp list-pending --json"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "mcp", "list-pending", "--json"]);
    // THEN the leaf-level json flag is set and the database stays default
    let Commands::Mcp {
        command: McpCommand::ListPending { db, json },
    } = cli.command
    else {
        panic!("expected mcp list-pending");
    };
    assert!(db.is_none());
    assert!(json);
}

#[test]
fn test_cli_parses_mcp_revoke_approval() {
    // GIVEN "apollia-os mcp revoke-approval files read_file"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "mcp", "revoke-approval", "files", "read_file"]);
    // THEN the server and tool are carried
    let Commands::Mcp {
        command:
            McpCommand::RevokeApproval {
                server,
                tool,
                db,
                json,
            },
    } = cli.command
    else {
        panic!("expected mcp revoke-approval");
    };
    assert_eq!(server, "files");
    assert_eq!(tool, "read_file");
    assert!(db.is_none());
    assert!(!json);
}

#[test]
fn test_cli_parses_mcp_server_with_runtime() {
    // GIVEN "apollia-os mcp server --with-runtime --sandbox-root /tmp/box"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "mcp",
        "server",
        "--with-runtime",
        "--sandbox-root",
        "/tmp/box",
    ]);
    // THEN the server arguments are carried in their flattened struct
    let Commands::Mcp {
        command: McpCommand::Server(args),
    } = cli.command
    else {
        panic!("expected mcp server");
    };
    assert!(args.with_runtime);
    assert_eq!(
        args.sandbox_root,
        Some(std::path::PathBuf::from("/tmp/box"))
    );
}

// ─── config ──────────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_config_validate_with_file() {
    // GIVEN "apollia-os config validate --file apollia.toml"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "config", "validate", "--file", "apollia.toml"]);
    // THEN the path override is carried
    let Commands::Config {
        command: ConfigCommand::Validate { file },
    } = cli.command
    else {
        panic!("expected config validate");
    };
    assert_eq!(file, Some(std::path::PathBuf::from("apollia.toml")));
}

#[test]
fn test_cli_parses_config_edit() {
    // GIVEN "apollia-os config edit"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "config", "edit"]);
    // THEN ConfigCommand::Edit with no path override
    let Commands::Config {
        command: ConfigCommand::Edit { file },
    } = cli.command
    else {
        panic!("expected config edit");
    };
    assert!(file.is_none());
}

#[test]
fn test_cli_parses_config_show() {
    // GIVEN "apollia-os config show"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "config", "show"]);
    // THEN ConfigCommand::Show with no path override
    let Commands::Config {
        command: ConfigCommand::Show { file },
    } = cli.command
    else {
        panic!("expected config show");
    };
    assert!(file.is_none());
}

// ─── profile ─────────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_profile_reset_with_confirm() {
    // GIVEN "apollia-os profile reset --confirm"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "profile", "reset", "--confirm"]);
    // THEN the destruction flag is set and the database stays default
    let Commands::Profile {
        command: UserMemoryCommand::Reset { confirm, db },
    } = cli.command
    else {
        panic!("expected profile reset");
    };
    assert!(confirm);
    assert!(db.is_none());
}

#[test]
fn test_cli_parses_profile_schema() {
    // GIVEN "apollia-os profile schema"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "profile", "schema"]);
    // THEN UserMemoryCommand::Schema with no database override
    let Commands::Profile {
        command: UserMemoryCommand::Schema { db },
    } = cli.command
    else {
        panic!("expected profile schema");
    };
    assert!(db.is_none());
}

#[test]
fn test_cli_parses_profile_export_to_file() {
    // GIVEN "apollia-os profile export --output profile.json"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "profile",
        "export",
        "--output",
        "profile.json",
    ]);
    // THEN the destination file is carried
    let Commands::Profile {
        command: UserMemoryCommand::Export { output, db },
    } = cli.command
    else {
        panic!("expected profile export");
    };
    assert_eq!(output, Some(std::path::PathBuf::from("profile.json")));
    assert!(db.is_none());
}

#[test]
fn test_cli_parses_profile_import_overwriting() {
    // GIVEN "apollia-os profile import --input profile.json --overwrite"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "profile",
        "import",
        "--input",
        "profile.json",
        "--overwrite",
    ]);
    // THEN the source file and the overwrite flag are carried
    let Commands::Profile {
        command:
            UserMemoryCommand::Import {
                input,
                overwrite,
                db,
            },
    } = cli.command
    else {
        panic!("expected profile import");
    };
    assert_eq!(input, std::path::PathBuf::from("profile.json"));
    assert!(overwrite);
    assert!(db.is_none());
}

// ─── project ─────────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_project_update_renaming() {
    // GIVEN "apollia-os project update p-1 --name Facturation"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "project",
        "update",
        "p-1",
        "--name",
        "Facturation",
    ]);
    // THEN only the field given on the command line is Some
    let Commands::Project {
        command:
            ProjectCommand::Update {
                id,
                name,
                description,
                instructions,
                workspace,
                db,
            },
    } = cli.command
    else {
        panic!("expected project update");
    };
    assert_eq!(id, "p-1");
    assert_eq!(name.as_deref(), Some("Facturation"));
    assert!(description.is_none());
    assert!(instructions.is_none());
    assert!(workspace.is_none());
    assert!(db.is_none());
}

#[test]
fn test_cli_parses_project_delete_with_confirm() {
    // GIVEN "apollia-os project delete p-1 --confirm"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "project", "delete", "p-1", "--confirm"]);
    // THEN the identifier is carried and the destruction flag is set
    let Commands::Project {
        command: ProjectCommand::Delete { id, confirm, db },
    } = cli.command
    else {
        panic!("expected project delete");
    };
    assert_eq!(id, "p-1");
    assert!(confirm);
    assert!(db.is_none());
}

#[test]
fn test_cli_parses_project_agents_list() {
    // GIVEN "apollia-os project agents list p-1"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "project", "agents", "list", "p-1"]);
    // THEN the nested agents sub-command carries the project
    let Commands::Project {
        command: ProjectCommand::Agents { command },
    } = cli.command
    else {
        panic!("expected project agents");
    };
    let ProjectAgentsCommand::List { project, db } = command else {
        panic!("expected project agents list");
    };
    assert_eq!(project, "p-1");
    assert!(db.is_none());
}

#[test]
fn test_cli_parses_project_agents_remove_with_confirm() {
    // GIVEN "apollia-os project agents remove p-1 invoice-reader --confirm"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "project",
        "agents",
        "remove",
        "p-1",
        "invoice-reader",
        "--confirm",
    ]);
    // THEN the pair is positional and the destruction flag is set
    let Commands::Project {
        command: ProjectCommand::Agents { command },
    } = cli.command
    else {
        panic!("expected project agents");
    };
    let ProjectAgentsCommand::Remove {
        project,
        agent,
        confirm,
        db,
    } = command
    else {
        panic!("expected project agents remove");
    };
    assert_eq!(project, "p-1");
    assert_eq!(agent, "invoice-reader");
    assert!(confirm);
    assert!(db.is_none());
}

#[test]
fn test_cli_parses_project_templates_list() {
    // GIVEN "apollia-os project templates list"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "project", "templates", "list"]);
    // THEN the nested templates sub-command is List
    let Commands::Project {
        command: ProjectCommand::Templates { command },
    } = cli.command
    else {
        panic!("expected project templates");
    };
    let ProjectTemplatesCommand::List { db } = command else {
        panic!("expected project templates list");
    };
    assert!(db.is_none());
}

#[test]
fn test_cli_parses_project_templates_seed_builtins() {
    // GIVEN "apollia-os project templates seed-builtins"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "project", "templates", "seed-builtins"]);
    // THEN the kebab-case leaf resolves to SeedBuiltins
    let Commands::Project {
        command: ProjectCommand::Templates { command },
    } = cli.command
    else {
        panic!("expected project templates");
    };
    let ProjectTemplatesCommand::SeedBuiltins { db } = command else {
        panic!("expected project templates seed-builtins");
    };
    assert!(db.is_none());
}

// ─── bare verbs ──────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_update_check_only() {
    // GIVEN "apollia-os update --check"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "update", "--check"]);
    // THEN the check flag is set and --yes, the flag this leaf published before
    // the confirmation rule, is not
    let Commands::Update(args) = cli.command else {
        panic!("expected update");
    };
    assert!(args.check);
    assert!(!args.yes);
}

#[test]
fn test_cli_parses_review_of_a_task() {
    // GIVEN "apollia-os review --task t-0042"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "review", "--task", "t-0042"]);
    // THEN the task source is carried and the other two sources stay unset
    let Commands::Review(args) = cli.command else {
        panic!("expected review");
    };
    assert_eq!(args.task.as_deref(), Some("t-0042"));
    assert!(args.pr.is_none());
    assert!(args.diff.is_none());
}

#[test]
fn test_cli_parses_doctor() {
    // GIVEN "apollia-os doctor"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "doctor"]);
    // THEN Commands::Doctor, which takes no argument
    assert!(matches!(cli.command, Commands::Doctor));
}

#[test]
fn test_cli_parses_version_subcommand() {
    // GIVEN "apollia-os version", the sub-command, not the --version flag
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "version"]);
    // THEN Commands::Version, which takes no argument
    assert!(matches!(cli.command, Commands::Version));
}

#[test]
fn test_cli_parses_trace_with_json_format() {
    // GIVEN "apollia-os trace t-0042 --format json"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "trace", "t-0042", "--format", "json"]);
    // THEN the task id is positional and the format overrides its human default
    let Commands::Trace { task_id, format } = cli.command else {
        panic!("expected trace");
    };
    assert_eq!(task_id, "t-0042");
    assert_eq!(format, "json");
}

#[test]
fn test_cli_parses_digest_over_a_week() {
    // GIVEN "apollia-os digest --since 7d"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "digest", "--since", "7d"]);
    // THEN the window is the seven-day variant, not the 24h default
    let Commands::Digest { since } = cli.command else {
        panic!("expected digest");
    };
    assert!(matches!(since, DigestWindow::Week));
}

#[test]
fn test_cli_parses_completions_for_bash() {
    // GIVEN "apollia-os completions bash"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "completions", "bash"]);
    // THEN the shell is the positional argument
    let Commands::Completions { shell } = cli.command else {
        panic!("expected completions");
    };
    assert!(matches!(shell, clap_complete::Shell::Bash));
}

#[test]
fn test_cli_parses_guide_with_topic() {
    // GIVEN "apollia-os guide memory"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "guide", "memory"]);
    // THEN the optional topic is carried
    let Commands::Guide { topic } = cli.command else {
        panic!("expected guide");
    };
    assert_eq!(topic.as_deref(), Some("memory"));
}

#[test]
fn test_cli_parses_do_with_short_yes() {
    // GIVEN "apollia-os do 'summarise my inbox' -y"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "do", "summarise my inbox", "-y"]);
    // THEN the request is positional and the short form of --yes is set
    let Commands::Do { request, yes } = cli.command else {
        panic!("expected do");
    };
    assert_eq!(request, "summarise my inbox");
    assert!(yes);
}

#[test]
fn test_cli_parses_explain() {
    // GIVEN "apollia-os explain 'what is a StepBudget'"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "explain", "what is a StepBudget"]);
    // THEN the text to explain is the positional argument
    let Commands::Explain { text } = cli.command else {
        panic!("expected explain");
    };
    assert_eq!(text, "what is a StepBudget");
}
