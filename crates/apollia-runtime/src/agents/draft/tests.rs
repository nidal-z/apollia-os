use super::*;
use apollia_tools::tools::{file_read, file_write};
use apollia_tools::ToolRegistryHandle;
use serde_json::json;

fn access(id: &str, description: &str, family: ActionFamily, by_default: bool) -> OfferedAccess {
    OfferedAccess {
        id: id.into(),
        description: description.into(),
        family: family.as_str().into(),
        sensitive: is_sensitive(family),
        by_default,
    }
}

fn offer() -> DraftOffer {
    DraftOffer {
        accesses: vec![
            access("web_search", "Search the web.", ActionFamily::Read, true),
            access("notes_lookup", "Look a note up.", ActionFamily::Read, false),
            access(
                "file_write",
                "Write a file.",
                ActionFamily::WriteOrDelete,
                false,
            ),
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

fn sheet(value: serde_json::Value) -> ProposedSheet {
    serde_json::from_value(value).unwrap()
}

#[test]
fn test_a_complete_sheet_yields_a_draft_of_the_same_six_fields() {
    // GIVEN a sheet carrying every field, its name padded and an example
    // given twice
    let proposed = sheet(json!({
        "name": "  Veille presse ",
        "mission": "Summarise the press every morning.",
        "examples": ["What is new today?", " ", "What is new today?"],
        "model": "qwen",
        "access": ["notes_lookup"],
        "sources": ["profile", "src-notes"],
    }));

    // WHEN it is checked
    let draft = check_draft(&offer(), proposed).unwrap();

    // THEN the draft holds the six fields, trimmed, the default access added
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

#[test]
fn test_what_the_machine_lacks_is_left_out_of_the_draft() {
    // GIVEN a sheet naming an access, a source and a model the machine lacks
    let proposed = sheet(json!({
        "name": "Courrier",
        "mission": "Answer the mail.",
        "model": "gpt-9",
        "access": ["web_search", "mail_send"],
        "sources": ["src-missing", "memory"],
    }));

    // WHEN it is checked
    let draft = check_draft(&offer(), proposed).unwrap();

    // THEN none of them is in the draft and each is named as unavailable
    let ids: Vec<&str> = draft.access.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, ["web_search"]);
    assert_eq!(draft.sources, ["memory"]);
    assert_eq!(draft.model, None);
    assert_eq!(
        draft.unavailable,
        ["mail_send", "source:src-missing", "model:gpt-9"]
    );
}

#[test]
fn test_a_sensitive_access_is_unchecked_by_default() {
    // GIVEN a sheet proposing a sensitive access beside a reading one
    let proposed = sheet(json!({
        "name": "Rangement",
        "mission": "Tidy the notes.",
        "access": ["file_write", "web_search"],
    }));

    // WHEN it is checked
    let draft = check_draft(&offer(), proposed).unwrap();

    // THEN the sensitive access is in the draft, unchecked, and the reading
    // one is checked
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

#[test]
fn test_the_reading_family_is_added_to_every_draft() {
    // GIVEN a sheet that names no access, on a machine holding a reading tool
    let proposed = sheet(json!({"name": "Veille", "mission": "Watch the press."}));

    // WHEN it is checked
    let draft = check_draft(&offer(), proposed).unwrap();

    // THEN the draft holds the default access, checked and marked as such,
    // and nothing else
    assert_eq!(
        draft.access,
        [DraftAccess {
            id: "web_search".into(),
            checked: true,
            sensitive: false,
            by_default: true,
        }]
    );
}

#[test]
fn test_a_sheet_without_a_name_or_a_mission_yields_no_draft() {
    // GIVEN a sheet with a blank mission and one with a blank name
    let no_mission = sheet(json!({"name": "Courrier", "mission": "   "}));
    let no_name = sheet(json!({"name": " ", "mission": "Answer the mail."}));

    // WHEN each is checked
    let refusals = [
        check_draft(&offer(), no_mission).unwrap_err(),
        check_draft(&offer(), no_name).unwrap_err(),
    ];

    // THEN each is refused, the reason naming the field
    assert_eq!(
        refusals,
        [DraftRefusal::EmptyMission, DraftRefusal::EmptyName]
    );
    assert_eq!(refusals[0].to_string(), "field 'mission' is empty");
}

#[test]
fn test_every_family_but_reading_is_sensitive() {
    // GIVEN every action family
    let families = [
        ActionFamily::Read,
        ActionFamily::Exempt,
        ActionFamily::ReadPersonal,
        ActionFamily::WriteOrDelete,
        ActionFamily::SendExternal,
        ActionFamily::ExecuteCode,
        ActionFamily::Unclassified,
    ];

    // WHEN asking which are sensitive
    let verdicts: Vec<bool> = families.into_iter().map(is_sensitive).collect();

    // THEN plain reading and the exempt family are not, every other one is
    assert_eq!(verdicts, [false, false, true, true, true, true, true]);
}

#[tokio::test]
async fn test_the_offer_is_what_the_registry_holds() {
    // GIVEN a registry holding a reading tool, a writing tool and a disabled
    // one
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

    // WHEN the offer is read
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

    // THEN it lists the two active tools with their family and their
    // sensitivity, each described in one line, the built-in sources first,
    // and the models handed in
    let accesses: Vec<(&str, &str, bool, bool)> = offer
        .accesses
        .iter()
        .map(|a| (a.id.as_str(), a.family.as_str(), a.sensitive, a.by_default))
        .collect();
    assert_eq!(
        accesses,
        [
            ("file_read", "read", false, true),
            ("file_write", "write_or_delete", true, false)
        ]
    );
    assert!(offer
        .accesses
        .iter()
        .all(|a| !a.description.is_empty() && !a.description.contains('\n')));
    let sources: Vec<&str> = offer.sources.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(sources, ["profile", "memory", "src-notes"]);
    assert_eq!(offer.models, ["qwen"]);
    registry.shutdown().await;
}

#[tokio::test]
async fn test_a_widened_default_is_checked_in_every_draft() {
    // GIVEN a default access widened with the writing tool by name, on a
    // machine holding a reading tool, the writing tool and a code tool
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

    // WHEN the offer is read, then a sheet asking for the code tool and the
    // writing tool is checked
    let offer = DraftOffer::read(&registry, &[], &default, Vec::new(), Vec::new()).await;
    let draft = check_draft(
        &offer,
        sheet(json!({
            "name": "Notes", "mission": "Keep the notes.", "access": ["bash", "file_write"],
        })),
    )
    .unwrap();

    // THEN the writing tool is offered as a default though sensitive, and the
    // draft opens with it and the reading tool checked, marked as defaults,
    // the code tool unchecked
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
    registry.shutdown().await;
}
