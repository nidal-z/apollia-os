use super::*;
use apollia_tools::tools::{file_read, file_write};
use serde_json::json;

fn offer() -> DraftOffer {
    DraftOffer {
        accesses: vec![
            OfferedAccess {
                id: "web_search".into(),
                description: "Search the web.".into(),
                sensitive: false,
                by_default: true,
            },
            OfferedAccess {
                id: "notes_lookup".into(),
                description: "Look a note up.".into(),
                sensitive: false,
                by_default: false,
            },
            OfferedAccess {
                id: "file_write".into(),
                description: "Write a file.\nSecond line.".into(),
                sensitive: true,
                by_default: false,
            },
        ],
        sources: vec![
            OfferedSource {
                id: "profile".into(),
                label: "the profile of the person".into(),
            },
            OfferedSource {
                id: "memory".into(),
                label: "the memory the agent keeps across its runs".into(),
            },
            OfferedSource {
                id: "src-notes".into(),
                label: "Notes".into(),
            },
        ],
        models: vec!["qwen".into()],
    }
}

// GIVEN the spec the model is offered
// WHEN its parameters are read
// THEN the draft is an object of the six fields of an agent sheet, the name
// and the mission required, the lists made of strings
#[test]
fn test_the_schema_of_the_draft_holds_the_six_fields_of_a_sheet() {
    let spec = agent_draft_spec();

    assert_eq!(spec.name, AGENT_DRAFT_TOOL_NAME);
    assert_eq!(spec.parameters["type"], "object");
    assert_eq!(spec.parameters["required"], json!(["name", "mission"]));
    let properties = spec.parameters["properties"].as_object().unwrap();
    let mut fields: Vec<&str> = properties.keys().map(String::as_str).collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        ["access", "examples", "mission", "model", "name", "sources"]
    );
    for text in ["name", "mission", "model"] {
        assert_eq!(properties[text]["type"], "string", "{text}");
    }
    for list in ["examples", "access", "sources"] {
        assert_eq!(properties[list]["type"], "array", "{list}");
        assert_eq!(properties[list]["items"]["type"], "string", "{list}");
    }
}

// GIVEN a call carrying every field of the schema
// WHEN the tool runs
// THEN the answer is a draft with the same six fields, read back whole
#[test]
fn test_a_complete_call_yields_a_structured_draft() {
    let result = run_agent_draft(
        &offer(),
        &json!({
            "name": "  Veille presse ",
            "mission": "Summarise the press every morning.",
            "examples": ["What is new today?", " ", "What is new today?"],
            "model": "qwen",
            "access": ["notes_lookup"],
            "sources": ["profile", "src-notes"],
        }),
    );

    assert!(result.ok);
    assert!(result.error.is_none());
    let wire = serde_json::to_value(&result).unwrap();
    let draft: AgentDraft = serde_json::from_value(wire["draft"].clone()).unwrap();
    assert_eq!(
        draft,
        AgentDraft {
            name: "Veille presse".into(),
            mission: "Summarise the press every morning.".into(),
            examples: vec!["What is new today?".into()],
            model: Some("qwen".into()),
            access: vec![
                DraftAccess {
                    id: "notes_lookup".into(),
                    checked: true,
                    sensitive: false,
                    by_default: false,
                },
                DraftAccess {
                    id: "web_search".into(),
                    checked: true,
                    sensitive: false,
                    by_default: true,
                },
            ],
            sources: vec!["profile".into(), "src-notes".into()],
            unavailable: Vec::new(),
        }
    );
}

// GIVEN a call naming an access, a source and a model the machine lacks
// WHEN the tool runs
// THEN none of them is in the draft and each is named as unavailable
#[test]
fn test_what_the_machine_lacks_is_left_out_of_the_draft() {
    let result = run_agent_draft(
        &offer(),
        &json!({
            "name": "Courrier",
            "mission": "Answer the mail.",
            "model": "gpt-9",
            "access": ["web_search", "mail_send"],
            "sources": ["src-missing", "memory"],
        }),
    );

    let draft = result.draft.unwrap();
    let ids: Vec<&str> = draft.access.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, ["web_search"]);
    assert_eq!(draft.sources, ["memory"]);
    assert_eq!(draft.model, None);
    assert_eq!(
        draft.unavailable,
        ["mail_send", "source:src-missing", "model:gpt-9"]
    );
    assert!(result.note.unwrap().contains("unavailable"));
}

// GIVEN a call proposing a sensitive access beside a reading one
// WHEN the tool runs
// THEN the sensitive access is in the draft, unchecked, and the reading one
// is checked
#[test]
fn test_a_sensitive_access_is_unchecked_by_default() {
    let result = run_agent_draft(
        &offer(),
        &json!({
            "name": "Rangement",
            "mission": "Tidy the notes.",
            "access": ["file_write", "web_search"],
        }),
    );

    let draft = result.draft.unwrap();
    assert_eq!(
        draft.access,
        [
            DraftAccess {
                id: "file_write".into(),
                checked: false,
                sensitive: true,
                by_default: false,
            },
            DraftAccess {
                id: "web_search".into(),
                checked: true,
                sensitive: false,
                by_default: false,
            },
        ]
    );
}

// GIVEN a call that names no access, on a machine holding a reading tool
// WHEN the tool runs
// THEN the draft holds the reading family, checked and marked as a default,
// and nothing else
#[test]
fn test_the_reading_family_is_added_to_every_draft() {
    let result = run_agent_draft(
        &offer(),
        &json!({"name": "Veille", "mission": "Watch the press."}),
    );

    assert_eq!(
        result.draft.unwrap().access,
        [DraftAccess {
            id: "web_search".into(),
            checked: true,
            sensitive: false,
            by_default: true,
        }]
    );
}

// GIVEN calls without a name, with an empty mission, and with a wrong shape
// WHEN the tool runs
// THEN each is answered with an error and no draft
#[test]
fn test_a_call_without_a_name_or_a_mission_proposes_nothing() {
    for args in [
        json!({"mission": "Answer the mail."}),
        json!({"name": "Courrier", "mission": "   "}),
        json!({"name": " ", "mission": "Answer the mail."}),
        json!({"name": "Courrier", "mission": "Answer.", "access": "web_search"}),
    ] {
        let result = run_agent_draft(&offer(), &args);

        assert!(!result.ok, "{args}");
        assert!(result.draft.is_none(), "{args}");
        assert!(result.error.unwrap().starts_with("invalid payload"));
    }
}

// GIVEN every action family
// WHEN asking which are sensitive
// THEN plain reading and the exempt family are not, every other one is
#[test]
fn test_every_family_but_reading_is_sensitive() {
    let verdicts: Vec<bool> = [
        ActionFamily::Read,
        ActionFamily::Exempt,
        ActionFamily::ReadPersonal,
        ActionFamily::WriteOrDelete,
        ActionFamily::SendExternal,
        ActionFamily::ExecuteCode,
        ActionFamily::Unclassified,
    ]
    .into_iter()
    .map(is_sensitive)
    .collect();

    assert_eq!(verdicts, [false, false, true, true, true, true, true]);
}

// GIVEN a registry holding a reading tool, a writing tool and a disabled one
// WHEN the offer is read
// THEN it lists the two active tools with their sensitivity, the built-in
// sources first, and the models handed in
#[tokio::test]
async fn test_the_offer_is_what_the_registry_holds() {
    let registry = ToolRegistryHandle::start();
    registry
        .register(file_read::FileRead::descriptor())
        .await
        .unwrap();
    registry
        .register(file_write::FileWrite::descriptor())
        .await
        .unwrap();
    let mut off = file_read::FileRead::descriptor();
    off.name = "switched_off".into();
    registry.register(off).await.unwrap();

    let offer = DraftOffer::read(
        &registry,
        &["switched_off".to_owned()],
        &DefaultAccess::default(),
        vec![OfferedSource {
            id: "src-notes".into(),
            label: "Notes".into(),
        }],
        vec!["qwen".into()],
    )
    .await;

    let accesses: Vec<(&str, bool, bool)> = offer
        .accesses
        .iter()
        .map(|a| (a.id.as_str(), a.sensitive, a.by_default))
        .collect();
    assert_eq!(
        accesses,
        [("file_read", false, true), ("file_write", true, false)]
    );
    let sources: Vec<&str> = offer.sources.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(sources, ["profile", "memory", "src-notes"]);
    assert_eq!(offer.models, ["qwen"]);
}

// GIVEN an offer with a sensitive access whose description spans two lines
// WHEN the instructions of the conversation are built
// THEN they name the tool, each access on one line with its mark, the
// sources and the models
#[test]
fn test_the_instructions_list_what_exists_on_the_machine() {
    let prompt = creation_prompt(&offer());

    assert!(prompt.contains("`propose_agent`"));
    assert!(prompt.contains("- `web_search` (default): Search the web.\n"));
    assert!(prompt.contains("- `notes_lookup`: Look a note up.\n"));
    assert!(prompt.contains("- `file_write` (sensitive): Write a file.\n"));
    assert!(!prompt.contains("Second line."));
    assert!(prompt.contains("- `src-notes`: Notes\n"));
    assert!(prompt.contains("- `qwen`\n"));
}

// GIVEN the tool list of a conversation
// WHEN asking whether it drafts an agent
// THEN only a list naming the draft tool does
#[test]
fn test_only_a_list_naming_the_tool_makes_a_draft_session() {
    assert!(is_draft_session(&[AGENT_DRAFT_TOOL_NAME.to_owned()]));
    assert!(!is_draft_session(&["web_search".to_owned()]));
    assert!(!is_draft_session(&[]));
}

// GIVEN a default access widened with the writing tool by name, on a machine
//   holding a reading tool, the writing tool and a code tool
// WHEN the offer is read, then a draft that names no access is proposed
// THEN the writing tool is offered as a default though sensitive, and the
//   draft opens with the reading tool and the writing tool checked, marked as
//   defaults, the code tool left out
#[tokio::test]
async fn test_a_widened_default_is_checked_in_every_draft() {
    let registry = ToolRegistryHandle::start();
    let mut bash = file_write::FileWrite::descriptor();
    bash.name = "bash".into();
    bash.family = ActionFamily::ExecuteCode;
    for descriptor in [
        file_read::FileRead::descriptor(),
        file_write::FileWrite::descriptor(),
        bash,
    ] {
        registry.register(descriptor).await.unwrap();
    }
    let default = DefaultAccess {
        families: vec![ActionFamily::Read],
        tools: vec!["file_write".into()],
    };

    let offer = DraftOffer::read(&registry, &[], &default, Vec::new(), Vec::new()).await;
    let draft = run_agent_draft(
        &offer,
        &json!({"name": "Notes", "mission": "Keep the notes.", "access": ["bash", "file_write"]}),
    )
    .draft
    .unwrap();

    let offered: Vec<(&str, bool, bool)> = offer
        .accesses
        .iter()
        .map(|a| (a.id.as_str(), a.sensitive, a.by_default))
        .collect();
    assert_eq!(
        offered,
        [
            ("bash", true, false),
            ("file_read", false, true),
            ("file_write", true, true)
        ]
    );
    let proposed: Vec<(&str, bool, bool)> = draft
        .access
        .iter()
        .map(|a| (a.id.as_str(), a.checked, a.by_default))
        .collect();
    assert_eq!(
        proposed,
        [
            ("bash", false, false),
            ("file_write", true, true),
            ("file_read", true, true)
        ]
    );
    assert!(creation_prompt(&offer).contains("- `file_write` (default, sensitive): "));
    registry.shutdown().await;
}
