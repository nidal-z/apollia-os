use super::*;

// GIVEN a full JSON produced by the LLM
// WHEN ToolCallRationale::parse()
// THEN every field is deserialized
#[test]
fn rationale_parses_full_payload() {
    let raw = r#"{
            "summary": "Lire le fichier de config avant l'édition.",
            "inputs_recap": [["path", "/tmp/cfg.toml"], ["offset", "0"]],
            "expected_outcome": "Obtenir le contenu courant pour décider l'édition.",
            "performance_hint": "Durée attendue: 50ms"
        }"#;
    let r = ToolCallRationale::parse(raw).expect("parse ok");
    assert_eq!(r.inputs_recap.len(), 2);
    assert_eq!(r.inputs_recap[0].0, "path");
    assert!(r.performance_hint.is_some());
}

// GIVEN a JSON without performance_hint or inputs_recap (both optional)
// WHEN parse()
// THEN default values (None / empty vec)
#[test]
fn rationale_defaults_optional_fields() {
    let raw = r#"{
            "summary": "s",
            "expected_outcome": "o"
        }"#;
    let r = ToolCallRationale::parse(raw).expect("parse ok");
    assert!(r.inputs_recap.is_empty());
    assert!(r.performance_hint.is_none());
}

// GIVEN a response with Markdown fences
// WHEN parse()
// THEN the fences are stripped
#[test]
fn rationale_strips_markdown_fences() {
    let raw = "```json\n{\"summary\":\"s\",\"expected_outcome\":\"o\"}\n```";
    let r = ToolCallRationale::parse(raw).expect("parse ok");
    assert_eq!(r.summary, "s");
}

// GIVEN a rationale attached to a ChatToolCallStarted
// WHEN a serde JSON roundtrip
// THEN the fields are preserved
#[test]
fn chat_tool_call_started_roundtrips_with_rationale() {
    let rationale = ToolCallRationale {
        summary: "why".into(),
        inputs_recap: vec![("k".into(), "v".into())],
        expected_outcome: "what".into(),
        performance_hint: Some("hint".into()),
    };
    let evt = RuntimeEvent::ChatToolCallStarted {
        session_id: "s".into(),
        message_id: "m".into(),
        tool_name: "bash_executor".into(),
        input_preview: "ls".into(),
        rationale: Some(rationale.clone()),
    };
    let json = serde_json::to_string(&evt).expect("serialize");
    let back: RuntimeEvent = serde_json::from_str(&json).expect("deserialize");
    match back {
        RuntimeEvent::ChatToolCallStarted {
            rationale: Some(r), ..
        } => {
            assert_eq!(r, rationale);
        }
        _ => panic!("wrong variant"),
    }
}
