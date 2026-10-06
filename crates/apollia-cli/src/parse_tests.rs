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
use crate::commands::approvals::ApprovalsCommand;
use crate::commands::audit::AuditCommand;
use crate::commands::chat::{ChatHygieneCommand, ChatNotesCommand, ChatSettingsCommand};
use crate::commands::chat_config::ChatConfigCommand;
use crate::commands::config::ConfigCommand;
use crate::commands::digest::DigestWindow;
use crate::commands::llm::{LlmBackendsCommand, LlmCommand};
use crate::commands::mcp::McpCommand;
use crate::commands::memory::{MemoryCommand, MemoryType};
use crate::commands::package::{PackageCommand as ApoPackageCommand, TrustCommand};
use crate::commands::permissions::PermissionsCommand;
use crate::commands::policy::PolicyCommand;
use crate::commands::service::ServiceCommand;
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
fn test_cli_parses_agent_set_with_list_values() {
    // GIVEN "apollia-os agent set invoices access file_read web_fetch"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "agent",
        "set",
        "invoices",
        "access",
        "file_read",
        "web_fetch",
    ]);
    // THEN the agent, the field and every value are carried through
    let Commands::Agent {
        command:
            AgentCommand::Set {
                agent_id,
                field,
                values,
            },
    } = cli.command
    else {
        panic!("expected agent set");
    };
    assert_eq!(agent_id, "invoices");
    assert_eq!(field, "access");
    assert_eq!(values, vec!["file_read", "web_fetch"]);
}

#[test]
fn test_cli_parses_agent_set_with_no_value() {
    // GIVEN "apollia-os agent set invoices sources" (an empty list)
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "agent", "set", "invoices", "sources"]);
    // THEN the values are empty
    let Commands::Agent {
        command: AgentCommand::Set { values, .. },
    } = cli.command
    else {
        panic!("expected agent set");
    };
    assert!(values.is_empty());
}

#[test]
fn test_cli_parses_agent_reset() {
    // GIVEN "apollia-os agent reset invoices --field mission --confirm"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "agent",
        "reset",
        "invoices",
        "--field",
        "mission",
        "--confirm",
    ]);
    // THEN the field and the confirmation are carried through
    let Commands::Agent {
        command:
            AgentCommand::Reset {
                agent_id,
                field,
                confirm,
            },
    } = cli.command
    else {
        panic!("expected agent reset");
    };
    assert_eq!(agent_id, "invoices");
    assert_eq!(field.as_deref(), Some("mission"));
    assert!(confirm);
}

#[test]
fn test_cli_parses_agent_export() {
    // GIVEN "apollia-os agent export veille --sign --publisher Ada -o out.apo"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "agent",
        "export",
        "veille",
        "--sign",
        "--publisher",
        "Ada",
        "-o",
        "out.apo",
    ]);
    // THEN the agent, the signature, the publisher and the file are carried
    let Commands::Agent {
        command:
            AgentCommand::Export {
                agent_id,
                sign,
                publisher,
                output,
            },
    } = cli.command
    else {
        panic!("expected agent export");
    };
    assert_eq!(agent_id, "veille");
    assert!(sign);
    assert_eq!(publisher.as_deref(), Some("Ada"));
    assert_eq!(output, Some(std::path::PathBuf::from("out.apo")));
}

#[test]
fn test_cli_parses_agent_export_unsigned_by_default() {
    // GIVEN "apollia-os agent export veille"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "agent", "export", "veille"]);
    // THEN nothing is signed and the setting names the publisher
    let Commands::Agent {
        command: AgentCommand::Export {
            sign, publisher, ..
        },
    } = cli.command
    else {
        panic!("expected agent export");
    };
    assert!(!sign);
    assert_eq!(publisher, None);
}

#[test]
fn test_cli_parses_agent_list_definitions() {
    // GIVEN "apollia-os agent list --definitions"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "agent", "list", "--definitions"]);
    // THEN the stored agents are asked for
    let Commands::Agent {
        command: AgentCommand::List { definitions, .. },
    } = cli.command
    else {
        panic!("expected agent list");
    };
    assert!(definitions);
}

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
    let PackageCommand::Uninstall {
        name,
        confirm,
        clear_memory,
        clear_history,
    } = cmd
    else {
        panic!("expected package uninstall");
    };
    assert_eq!(name, "invoice-reader");
    assert!(confirm);
    // AND the memory and the history are kept unless asked
    assert!(!clear_memory && !clear_history);
}

#[test]
fn test_cli_parses_agent_package_uninstall_erasing_memory_and_history() {
    // GIVEN "apollia-os agent package uninstall invoice-reader --clear-memory
    // --clear-history --confirm"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "agent",
        "package",
        "uninstall",
        "invoice-reader",
        "--clear-memory",
        "--clear-history",
        "--confirm",
    ]);
    // THEN both erasures are carried
    let Commands::Agent {
        command: AgentCommand::Package { cmd },
    } = cli.command
    else {
        panic!("expected agent package");
    };
    let PackageCommand::Uninstall {
        clear_memory,
        clear_history,
        ..
    } = cmd
    else {
        panic!("expected package uninstall");
    };
    assert!(clear_memory && clear_history);
}

#[test]
fn test_cli_parses_agent_uninstall_erasing_memory_only() {
    // GIVEN "apollia-os agent uninstall notes --clear-memory --confirm"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "agent",
        "uninstall",
        "notes",
        "--clear-memory",
        "--confirm",
    ]);
    // THEN the memory is erased and the history kept
    let Commands::Agent {
        command:
            AgentCommand::Uninstall {
                name,
                clear_memory,
                clear_history,
                ..
            },
    } = cli.command
    else {
        panic!("expected agent uninstall");
    };
    assert_eq!(name, "notes");
    assert!(clear_memory);
    assert!(!clear_history);
}

#[test]
fn test_cli_parses_package_new() {
    // GIVEN "apollia-os package new invoice-sorter --output-dir /tmp/work"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "package",
        "new",
        "invoice-sorter",
        "--output-dir",
        "/tmp/work",
    ]);
    // THEN PackageCommand::New carries the name and the folder
    let Commands::Package {
        command: ApoPackageCommand::New { name, output_dir },
    } = cli.command
    else {
        panic!("expected package new");
    };
    assert_eq!(name, "invoice-sorter");
    assert_eq!(output_dir, Some(std::path::PathBuf::from("/tmp/work")));
}

#[test]
fn test_cli_no_longer_parses_the_verbs_of_the_former_agent_contract() {
    // GIVEN the verbs that installed, checked, scaffolded or repaired an
    // agent written as a Python class, and the command that inspected one
    let retired: [&[&str]; 6] = [
        &["apollia-os", "agent", "install", "./agent.py"],
        &["apollia-os", "agent", "update", "notes", "./agent.py"],
        &["apollia-os", "agent", "create", "notes"],
        &["apollia-os", "agent", "validate", "./agent.py"],
        &["apollia-os", "agent", "repair", "notes"],
        &["apollia-os", "inspect", "./agent.py"],
    ];

    for args in retired {
        // WHEN the top-level parser reads one
        let parsed = Cli::try_parse_from(args);

        // THEN it is refused: there is one way to write an agent, a package
        assert!(parsed.is_err(), "{args:?} still parses");
    }
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
        command:
            AuditCommand::Journal {
                limit,
                offset,
                agents,
                families,
                natures,
                results,
                since,
                until,
            },
    } = cli.command
    else {
        panic!("expected audit journal");
    };
    assert_eq!(limit, 5);
    assert_eq!(offset, 10);
    // AND no filter is set
    assert!(agents.is_empty() && families.is_empty() && natures.is_empty());
    assert!(results.is_empty() && since.is_none() && until.is_none());
}

#[test]
fn test_cli_parses_audit_journal_with_every_filter() {
    // GIVEN "apollia-os audit journal" with a family twice, a nature, a
    // result and a period
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "audit",
        "journal",
        "--family",
        "read",
        "--family",
        "send_external",
        "--nature",
        "action",
        "--result",
        "refused",
        "--since",
        "2026-10-01",
        "--until",
        "2026-10-04",
    ]);
    // THEN each filter is carried as given, the repeated one in order
    let Commands::Audit {
        command:
            AuditCommand::Journal {
                families,
                natures,
                results,
                since,
                until,
                ..
            },
    } = cli.command
    else {
        panic!("expected audit journal");
    };
    assert_eq!(families, vec!["read", "send_external"]);
    assert_eq!(natures, vec!["action"]);
    assert_eq!(results, vec!["refused"]);
    assert_eq!(since.as_deref(), Some("2026-10-01"));
    assert_eq!(until.as_deref(), Some("2026-10-04"));
}

#[test]
fn test_cli_parses_audit_journal_with_agents() {
    // GIVEN "apollia-os audit journal --agent flux-quotes --agent flux-invoices"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "audit",
        "journal",
        "--agent",
        "flux-quotes",
        "--agent",
        "flux-invoices",
    ]);
    // THEN both agents are carried, in order
    let Commands::Audit {
        command: AuditCommand::Journal { agents, .. },
    } = cli.command
    else {
        panic!("expected audit journal");
    };
    assert_eq!(agents, vec!["flux-quotes", "flux-invoices"]);
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

#[test]
fn test_cli_parses_chat_capabilities_with_its_session() {
    // GIVEN "apollia-os chat capabilities s-1"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "chat", "capabilities", "s-1"]);
    // THEN the chat sub-command carries the session whose model is described
    let Commands::Chat { command, .. } = cli.command else {
        panic!("expected chat");
    };
    let Some(ChatHygieneCommand::Capabilities { session_id }) = command else {
        panic!("expected chat capabilities");
    };
    assert_eq!(session_id, "s-1");
}

#[test]
fn test_cli_refuses_chat_capabilities_without_a_session() {
    // GIVEN "apollia-os chat capabilities" with no session id
    // WHEN the top-level parser reads it
    let parsed = Cli::try_parse_from(["apollia-os", "chat", "capabilities"]);
    // THEN it is refused: the session is required
    assert!(parsed.is_err());
}

#[test]
fn test_cli_parses_chat_settings_get_and_set() {
    // GIVEN "apollia-os chat settings get s-1" and the matching set
    // WHEN the top-level parser reads each
    let get = parse(&["apollia-os", "chat", "settings", "get", "s-1"]);
    let set = parse(&[
        "apollia-os",
        "chat",
        "settings",
        "set",
        "s-1",
        r#"{"max_tokens":256}"#,
    ]);
    // THEN each carries its session, and the set its document
    let Commands::Chat {
        command: Some(ChatHygieneCommand::Settings { command }),
        ..
    } = get.command
    else {
        panic!("expected chat settings");
    };
    let ChatSettingsCommand::Get { session_id } = command else {
        panic!("expected chat settings get");
    };
    assert_eq!(session_id, "s-1");
    let Commands::Chat {
        command: Some(ChatHygieneCommand::Settings { command }),
        ..
    } = set.command
    else {
        panic!("expected chat settings");
    };
    let ChatSettingsCommand::Set {
        session_id,
        settings,
    } = command
    else {
        panic!("expected chat settings set");
    };
    assert_eq!(session_id, "s-1");
    assert_eq!(settings, r#"{"max_tokens":256}"#);
}

#[test]
fn test_cli_refuses_chat_settings_set_without_a_document() {
    // GIVEN "apollia-os chat settings set s-1" with no settings
    // WHEN the top-level parser reads it
    let parsed = Cli::try_parse_from(["apollia-os", "chat", "settings", "set", "s-1"]);
    // THEN it is refused: the document is required
    assert!(parsed.is_err());
}

#[test]
fn test_cli_parses_chat_notes_get_and_set() {
    // GIVEN "apollia-os chat notes get s-1" and the matching set
    // WHEN the top-level parser reads each
    let get = parse(&["apollia-os", "chat", "notes", "get", "s-1"]);
    let set = parse(&[
        "apollia-os",
        "chat",
        "notes",
        "set",
        "s-1",
        "ask about March",
    ]);
    // THEN each carries its session, and the set its text
    let Commands::Chat {
        command: Some(ChatHygieneCommand::Notes { command }),
        ..
    } = get.command
    else {
        panic!("expected chat notes");
    };
    let ChatNotesCommand::Get { session_id } = command else {
        panic!("expected chat notes get");
    };
    assert_eq!(session_id, "s-1");
    let Commands::Chat {
        command: Some(ChatHygieneCommand::Notes { command }),
        ..
    } = set.command
    else {
        panic!("expected chat notes");
    };
    let ChatNotesCommand::Set { session_id, notes } = command else {
        panic!("expected chat notes set");
    };
    assert_eq!(session_id, "s-1");
    assert_eq!(notes, "ask about March");
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
fn test_cli_parses_mcp_server() {
    // GIVEN "apollia-os mcp server"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "mcp", "server"]);
    // THEN it is the bridge, which takes no tool flag
    let Commands::Mcp {
        command: McpCommand::Server,
    } = cli.command
    else {
        panic!("expected mcp server");
    };
}

#[test]
fn test_cli_refuses_the_retired_mcp_server_flags() {
    // GIVEN the flags the native-tool server used to take
    for flag in ["--with-runtime", "--sandbox-root"] {
        // WHEN the parser reads them
        let parsed = Cli::try_parse_from(["apollia-os", "mcp", "server", flag]);
        // THEN they are refused
        assert!(parsed.is_err(), "{flag} must be refused");
    }
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

#[test]
fn test_cli_parses_config_keys() {
    // GIVEN "apollia-os config keys --json"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "config", "keys", "--json"]);
    // THEN ConfigCommand::Keys, with the global JSON switch on
    assert!(cli.json);
    assert!(matches!(
        cli.command,
        Commands::Config {
            command: ConfigCommand::Keys
        }
    ));
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

// ─── project (retired) ───────────────────────────────────────────────────────

#[test]
fn test_cli_refuses_the_retired_project_noun() {
    // GIVEN the projects, which became sources
    // WHEN `project list` and `project link` are parsed
    let list = Cli::try_parse_from(["apollia-os", "project", "list"]);
    let link = Cli::try_parse_from(["apollia-os", "project", "link", "p-1", "--session", "s-1"]);
    // THEN clap refuses both as an unknown subcommand
    for parsed in [list, link] {
        let kind = parsed.err().map(|err| err.kind());
        assert_eq!(kind, Some(clap::error::ErrorKind::InvalidSubcommand));
    }
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

// ─── package ─────────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_package_build() {
    // GIVEN "apollia-os package build ./pkg --sign"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "package", "build", "./pkg", "--sign"]);
    // THEN the folder and the signing request are carried, with the default key
    let Commands::Package {
        command: ApoPackageCommand::Build { dir, sign, key, .. },
    } = cli.command
    else {
        panic!("expected package build");
    };
    assert_eq!(dir, std::path::PathBuf::from("./pkg"));
    assert!(sign);
    assert_eq!(key, "default");
}

#[test]
fn test_cli_parses_package_inspect() {
    // GIVEN "apollia-os package inspect lone.apo --json"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "package", "inspect", "lone.apo", "--json"]);
    // THEN the archive path is carried and --json is global
    assert!(cli.json);
    let Commands::Package {
        command: ApoPackageCommand::Inspect { file },
    } = cli.command
    else {
        panic!("expected package inspect");
    };
    assert_eq!(file, std::path::PathBuf::from("lone.apo"));
}

#[test]
fn test_cli_parses_package_preview() {
    // GIVEN "apollia-os package preview crm.apo"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "package", "preview", "crm.apo"]);
    // THEN the archive path is carried
    let Commands::Package {
        command: ApoPackageCommand::Preview { file },
    } = cli.command
    else {
        panic!("expected package preview");
    };
    assert_eq!(file, std::path::PathBuf::from("crm.apo"));
}

#[test]
fn test_cli_parses_package_install_with_every_confirmation() {
    // GIVEN an install with every confirmation flag
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "package",
        "install",
        "crm.apo",
        "--confirm-deps",
        "--confirm-changes",
        "--allow-downgrade",
    ]);
    // THEN each flag is carried
    let Commands::Package {
        command:
            ApoPackageCommand::Install {
                file,
                confirm_deps,
                confirm_changes,
                allow_downgrade,
            },
    } = cli.command
    else {
        panic!("expected package install");
    };
    assert_eq!(file, std::path::PathBuf::from("crm.apo"));
    assert!(confirm_deps && confirm_changes && allow_downgrade);
}

#[test]
fn test_cli_parses_package_trust_import() {
    // GIVEN "apollia-os package trust import <key> --label Acme"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "package",
        "trust",
        "import",
        "acme.pub",
        "--label",
        "Acme",
    ]);
    // THEN the key and the label are carried
    let Commands::Package {
        command:
            ApoPackageCommand::Trust {
                command: TrustCommand::Import { key, label },
            },
    } = cli.command
    else {
        panic!("expected package trust import");
    };
    assert_eq!(key, "acme.pub");
    assert_eq!(label, "Acme");
}

#[test]
fn test_cli_refuses_package_trust_import_without_label() {
    // GIVEN an import without its label
    // WHEN the top-level parser reads it
    let parsed = Cli::try_parse_from(["apollia-os", "package", "trust", "import", "acme.pub"]);
    // THEN clap refuses it
    assert!(parsed.is_err());
}

#[test]
fn test_cli_parses_package_trust_list() {
    // GIVEN "apollia-os package trust list --json"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "package", "trust", "list", "--json"]);
    // THEN the list verb is selected and --json is global
    assert!(cli.json);
    assert!(matches!(
        cli.command,
        Commands::Package {
            command: ApoPackageCommand::Trust {
                command: TrustCommand::List
            }
        }
    ));
}

#[test]
fn test_cli_parses_package_trust_remove() {
    // GIVEN "apollia-os package trust remove <fingerprint> --confirm"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "package",
        "trust",
        "remove",
        "ff00",
        "--confirm",
    ]);
    // THEN the fingerprint and the confirmation are carried
    let Commands::Package {
        command:
            ApoPackageCommand::Trust {
                command:
                    TrustCommand::Remove {
                        fingerprint,
                        confirm,
                    },
            },
    } = cli.command
    else {
        panic!("expected package trust remove");
    };
    assert_eq!(fingerprint, "ff00");
    assert!(confirm);
}

// ─── policy ──────────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_policy_keygen() {
    // GIVEN "apollia-os policy keygen --out ./keys"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "policy", "keygen", "--out", "./keys"]);
    // THEN the output folder is carried
    let Commands::Policy {
        command: PolicyCommand::Keygen { out },
    } = cli.command
    else {
        panic!("expected policy keygen");
    };
    assert_eq!(out, std::path::PathBuf::from("./keys"));
}

#[test]
fn test_cli_parses_policy_init() {
    // GIVEN "apollia-os policy init ./policy"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "policy", "init", "./policy"]);
    // THEN the folder is carried
    let Commands::Policy {
        command: PolicyCommand::Init { dir },
    } = cli.command
    else {
        panic!("expected policy init");
    };
    assert_eq!(dir, std::path::PathBuf::from("./policy"));
}

#[test]
fn test_cli_parses_policy_sign() {
    // GIVEN "apollia-os policy sign --key ./keys/org.key ./policy"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "policy",
        "sign",
        "--key",
        "./keys/org.key",
        "./policy",
    ]);
    // THEN the key, the folder and the default publisher are carried
    let Commands::Policy {
        command:
            PolicyCommand::Sign {
                key,
                publisher,
                dir,
            },
    } = cli.command
    else {
        panic!("expected policy sign");
    };
    assert_eq!(key, std::path::PathBuf::from("./keys/org.key"));
    assert_eq!(dir, std::path::PathBuf::from("./policy"));
    assert_eq!(publisher, crate::commands::policy::DEFAULT_PUBLISHER);
}

#[test]
fn test_cli_parses_policy_verify() {
    // GIVEN "apollia-os policy verify ./policy --json"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "policy", "verify", "./policy", "--json"]);
    // THEN the folder is carried and --json is global
    assert!(cli.json);
    let Commands::Policy {
        command: PolicyCommand::Verify { dir },
    } = cli.command
    else {
        panic!("expected policy verify");
    };
    assert_eq!(dir, std::path::PathBuf::from("./policy"));
}

#[test]
fn test_cli_parses_policy_show() {
    // GIVEN "apollia-os policy show"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "policy", "show"]);
    // THEN the show leaf is selected
    assert!(matches!(
        cli.command,
        Commands::Policy {
            command: PolicyCommand::Show
        }
    ));
}

#[test]
fn test_cli_policy_sign_requires_a_key() {
    // GIVEN "apollia-os policy sign ./policy" without --key
    // WHEN the top-level parser reads it
    let result = Cli::try_parse_from(["apollia-os", "policy", "sign", "./policy"]);
    // THEN it is a usage error
    assert!(result.is_err());
}

// ─── service ─────────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_service_install_with_no_load() {
    // GIVEN "apollia-os service install --no-load"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "service", "install", "--no-load"]);
    // THEN the install verb carries the write-only flag
    let Commands::Service { command } = cli.command else {
        panic!("expected service");
    };
    assert!(matches!(command, ServiceCommand::Install { no_load: true }));
}

#[test]
fn test_cli_parses_service_install_loads_by_default() {
    // GIVEN "apollia-os service install"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "service", "install"]);
    // THEN the service is handed to the manager unless asked otherwise
    let Commands::Service { command } = cli.command else {
        panic!("expected service");
    };
    assert!(matches!(
        command,
        ServiceCommand::Install { no_load: false }
    ));
}

#[test]
fn test_cli_parses_service_uninstall_with_confirm() {
    // GIVEN "apollia-os service uninstall --confirm"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "service", "uninstall", "--confirm"]);
    // THEN the destruction is confirmed
    let Commands::Service { command } = cli.command else {
        panic!("expected service");
    };
    assert!(matches!(
        command,
        ServiceCommand::Uninstall { confirm: true }
    ));
}

#[test]
fn test_cli_parses_service_status() {
    // GIVEN "apollia-os service status"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "service", "status"]);
    // THEN the status verb is selected
    let Commands::Service { command } = cli.command else {
        panic!("expected service");
    };
    assert!(matches!(command, ServiceCommand::Status));
}

// ─── approvals ──────────────────────────────────────────────────────────────

#[test]
fn test_cli_parses_approvals_pending() {
    // GIVEN "apollia-os approvals pending --json"
    // WHEN the top-level parser reads it
    let cli = parse(&["apollia-os", "approvals", "pending", "--json"]);
    // THEN the pending verb of the approvals noun is selected, in JSON
    assert!(cli.json);
    let Commands::Approvals(args) = cli.command else {
        panic!("expected approvals");
    };
    assert!(matches!(args.command, ApprovalsCommand::Pending));
}

#[test]
fn test_cli_parses_approvals_decide() {
    // GIVEN "apollia-os approvals decide <id> answer --value 180.50"
    // WHEN the top-level parser reads it
    let cli = parse(&[
        "apollia-os",
        "approvals",
        "decide",
        "workflow-question:r-1",
        "answer",
        "--value",
        "180.50",
    ]);
    // THEN the decide verb carries the item, the answer and its value
    let Commands::Approvals(args) = cli.command else {
        panic!("expected approvals");
    };
    let ApprovalsCommand::Decide {
        id,
        answer,
        value,
        reason,
    } = args.command
    else {
        panic!("expected decide");
    };
    assert_eq!(id, "workflow-question:r-1");
    assert_eq!(answer, crate::commands::approvals::DecideAnswer::Answer);
    assert_eq!(value.as_deref(), Some("180.50"));
    assert!(reason.is_none());
}
