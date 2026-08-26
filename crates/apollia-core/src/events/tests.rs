use super::*;

#[test]
fn test_all_variants_exist_and_clone() {
    // GIVEN / WHEN: instantiate each variant and clone it
    let variants: Vec<RuntimeEvent> = vec![
        RuntimeEvent::AgentRegistered("agent-1".into()),
        RuntimeEvent::AgentReady("agent-1".into()),
        RuntimeEvent::AgentDegraded {
            agent_id: "agent-1".into(),
            reason: "tool missing".into(),
        },
        RuntimeEvent::AgentStopping("agent-1".into()),
        RuntimeEvent::AgentStopped("agent-1".into()),
        RuntimeEvent::TaskStarted {
            agent_id: "agent-1".into(),
            task_id: "task-1".into(),
        },
        RuntimeEvent::TaskCompleted {
            agent_id: "agent-1".into(),
            task_id: "task-1".into(),
            success: true,
            output: None,
        },
        RuntimeEvent::TaskCanceled {
            task_id: "task-1".into(),
        },
        RuntimeEvent::ToolCircuitBroken {
            tool_name: "bash_executor".into(),
        },
        RuntimeEvent::ToolCircuitRestored {
            tool_name: "bash_executor".into(),
        },
        RuntimeEvent::AllReady,
        RuntimeEvent::ShutdownRequested,
        RuntimeEvent::AgentLoadFailed {
            name: "broken-agent".into(),
            error: "module not found".into(),
        },
        RuntimeEvent::AgentInstalled {
            name: "mon-agent".into(),
            version: "0.1.0".into(),
        },
        RuntimeEvent::AgentUninstalled {
            name: "mon-agent".into(),
        },
        RuntimeEvent::AgentEnabled {
            name: "mon-agent".into(),
        },
        RuntimeEvent::AgentDisabled {
            name: "mon-agent".into(),
        },
        RuntimeEvent::LlmModelLoading {
            backend: "local".into(),
            model_path: "/tmp/model.gguf".into(),
        },
        RuntimeEvent::LlmModelReady {
            backend: "local".into(),
            model_id: "llama3.2-q4".into(),
        },
        RuntimeEvent::LlmModelFailed {
            backend: "local".into(),
            reason: "file not found".into(),
        },
        RuntimeEvent::LlmCallCompleted {
            backend: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            task_id: Some("task-42".into()),
            step_id: None,
            prompt_tokens: 100,
            completion_tokens: 50,
            latency_ms: 250,
            cost_usd: Some(0.001),
            run_id: None,
        },
        RuntimeEvent::LlmCallFailed {
            backend: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            task_id: Some("task-42".into()),
            step_id: None,
            error: "rate limit exceeded".into(),
            analysis: crate::error_analysis::ErrorAnalysis::new(
                crate::error_analysis::ErrorCategory::LlmError,
                "The AI service rate-limited the request.",
                "rate limit exceeded",
            ),
        },
        RuntimeEvent::TriggerFired {
            trigger_id: "rapport-hebdo".into(),
            agent: "rapport-agent".into(),
            task_id: "task-1".into(),
        },
        RuntimeEvent::TriggerSkipped {
            trigger_id: "rapport-hebdo".into(),
            reason: "agent busy, on_busy=drop".into(),
        },
        RuntimeEvent::TriggerError {
            trigger_id: "rapport-hebdo".into(),
            error: "agent not found".into(),
        },
        RuntimeEvent::TriggerQueueFull {
            trigger_id: "rapport-hebdo".into(),
        },
        RuntimeEvent::TriggerEnabled {
            trigger_id: "rapport-hebdo".into(),
        },
        RuntimeEvent::TriggerDisabled {
            trigger_id: "rapport-hebdo".into(),
        },
        RuntimeEvent::TriggersReloaded { count: 3 },
        // ── Orchestrated mode ────────────────────────────────
        RuntimeEvent::PlanGenerated {
            task_id: "task-1".into(),
            agent_name: "mon-agent".into(),
            plan_id: "plan-abc".into(),
            step_count: 4,
            run_id: None,
        },
        RuntimeEvent::StepStarted {
            task_id: "task-1".into(),
            plan_id: "plan-abc".into(),
            step_id: "s1".into(),
            step_num: 1,
            total: 4,
            desc: "Read the file".into(),
        },
        RuntimeEvent::StepCompleted {
            task_id: "task-1".into(),
            plan_id: "plan-abc".into(),
            step_id: "s1".into(),
            duration_ms: 1200,
        },
        RuntimeEvent::StepFailed {
            task_id: "task-1".into(),
            plan_id: "plan-abc".into(),
            step_id: "s2".into(),
            error: "timeout".into(),
            retryable: true,
        },
        RuntimeEvent::PlanReplanning {
            task_id: "task-1".into(),
            plan_id: "plan-abc".into(),
            attempt: 1,
            failed_step: "s2".into(),
            reason: "timeout".into(),
        },
        RuntimeEvent::PlanCompleted {
            task_id: "task-1".into(),
            plan_id: "plan-abc".into(),
            step_count: 4,
            duration_ms: 15900,
        },
        RuntimeEvent::PlanFailed {
            task_id: "task-1".into(),
            plan_id: "plan-abc".into(),
            reason: "MAX_REPLAN_EXCEEDED".into(),
        },
        // ── HITL ──────────────────────────────────────────
        RuntimeEvent::TaskApprovalTimeout {
            task_id: "task-1".into(),
            after_secs: 86400,
        },
        RuntimeEvent::TaskInputRequired {
            task_id: "task-1".into(),
            prompt: "Confirmer l'envoi ?".into(),
            step_id: None,
        },
        RuntimeEvent::TaskResumed {
            task_id: "task-1".into(),
            approved: true,
        },
        // ── Chat ────────────────────────────────
        RuntimeEvent::ChatSessionCreated {
            session_id: "sess-001".into(),
            mode: "libre".into(),
            agent_name: None,
        },
        RuntimeEvent::ChatSessionClosed {
            session_id: "sess-001".into(),
        },
        RuntimeEvent::ChatMessageSent {
            session_id: "sess-001".into(),
            message_id: "msg-001".into(),
        },
        RuntimeEvent::ChatResponseStarted {
            session_id: "sess-001".into(),
            message_id: "msg-002".into(),
            run_id: None,
        },
        RuntimeEvent::ChatToken {
            session_id: "sess-001".into(),
            message_id: "msg-002".into(),
            token: "Hello".into(),
        },
        RuntimeEvent::ChatResponseCompleted {
            session_id: "sess-001".into(),
            message_id: "msg-002".into(),
            content: "Hello, world!".into(),
            run_id: None,
        },
        RuntimeEvent::ChatError {
            session_id: "sess-001".into(),
            message_id: Some("msg-003".into()),
            error: "LLM timeout".into(),
        },
        RuntimeEvent::ChatToolCallStarted {
            session_id: "sess-001".into(),
            message_id: "msg-004".into(),
            tool_name: "bash_executor".into(),
            input_preview: "ls -la".into(),
            rationale: None,
        },
        RuntimeEvent::ChatToolCallCompleted {
            session_id: "sess-001".into(),
            message_id: "msg-004".into(),
            tool_name: "bash_executor".into(),
            success: true,
            output_preview: Some("file.txt".into()),
            analysis: None,
        },
        RuntimeEvent::ChatApprovalRequired {
            session_id: "sess-001".into(),
            message_id: "msg-005".into(),
            tool_call_id: "call-005".into(),
            tool_name: "bash_executor".into(),
            prompt: "Allow bash execution?".into(),
        },
        RuntimeEvent::ChatApprovalResolved {
            session_id: "sess-001".into(),
            message_id: "msg-005".into(),
            tool_call_id: "call-005".into(),
            tool_name: "bash_executor".into(),
            decision: "accept".into(),
        },
        RuntimeEvent::ChatApprovalTimeout {
            session_id: "sess-001".into(),
            message_id: "msg-005".into(),
            tool_call_id: "call-005".into(),
            tool_name: "bash_executor".into(),
        },
        // ── User Input (ask_user) ────────────────────────
        RuntimeEvent::ChatUserInputRequired {
            request_id: "req-001".into(),
            session_id: "sess-001".into(),
            message_id: "msg-006".into(),
            questions_json: "[]".into(),
            context: Some("Need project details".into()),
        },
        RuntimeEvent::ChatUserInputResolved {
            request_id: "req-001".into(),
            session_id: "sess-001".into(),
        },
        // ── Plan Cache ────────────────────────
        RuntimeEvent::PlanCacheHit {
            task_id: "task-1".into(),
            cache_key: "abc123def456".into(),
        },
        // ── A2A Invocation ────────────────────────
        RuntimeEvent::A2AInvocationStarted {
            caller: "director".into(),
            target: "excel-worker".into(),
            skill_id: "read-excel".into(),
        },
        RuntimeEvent::A2AInvocationCompleted {
            caller: "director".into(),
            target: "excel-worker".into(),
            skill_id: "read-excel".into(),
            status: "completed".into(),
            duration_ms: 350,
        },
        // ── A2A Guard ────────────────────────────
        RuntimeEvent::A2AGuardTriggered {
            guard_type: "max_depth".into(),
            caller: "director".into(),
            skill_id: "read-excel".into(),
            detail: "depth 3 reaches max_depth 3".into(),
        },
        // ── Onboarding ──────────────────────────
        RuntimeEvent::OnboardingRequired,
        RuntimeEvent::OnboardingStarted {
            session_id: "sess-123".into(),
            mode: "full".into(),
            topic: None,
        },
        RuntimeEvent::OnboardingCompleted {
            profile: "operator".into(),
            duration_sec: 1200,
            actions_count: 18,
        },
        // ── STT ──────────────────────────────────
        RuntimeEvent::SttRecordingStarted,
        RuntimeEvent::SttRecordingStopped {
            audio_duration_ms: 3200,
        },
        RuntimeEvent::SttModelLoaded {
            backend: "whisper-cpp".into(),
            model_path: "/tmp/model.bin".into(),
            model_name: "whisper-large-v3".into(),
        },
        RuntimeEvent::SttTranscribed {
            text: "Hello world".into(),
            language: Some("fr".into()),
            source: "hotkey".into(),
            duration_ms: 3000,
            processing_time_ms: 800,
        },
        RuntimeEvent::SttTranscriptionFailed {
            reason: "model not loaded".into(),
        },
        // ── Token Budget ─────────────────────────────────
        RuntimeEvent::TokenBudgetUpdated {
            session_cost_usd: 0.0023,
            total_input_tokens: 300,
            total_output_tokens: 150,
            total_cache_read_tokens: 240,
            threshold_usd: 0.50,
            threshold_exceeded: false,
        },
        // ── Thinking / Reasoning transparency ─────────────
        RuntimeEvent::ThinkingStarted {
            turn_id: "turn-001".into(),
            ts_ms: 1_700_000_000_000,
        },
        RuntimeEvent::ThinkingEnded {
            turn_id: "turn-001".into(),
            ts_ms: 1_700_000_001_500,
            duration_ms: 1_500,
            raw_content: "Let me think about this...".into(),
            tokens: 120,
        },
        // ── Meta LLM Orchestrator ─────────────────────────
        RuntimeEvent::MetaLlmBudgetExceeded {
            session_id: "sess-meta".into(),
            tokens_used: 10_500,
            budget: 10_000,
        },
        // ── Context Manager ───────────────────────────────
        RuntimeEvent::ContextCompacted {
            summary_chars: 3800,
            original_messages: 42,
        },
        // ── File Path Extraction ──────────────────────────
        RuntimeEvent::BashFilePathsExtracted {
            paths: vec![
                std::path::PathBuf::from("src/main.rs"),
                std::path::PathBuf::from("/tmp/out.txt"),
            ],
        },
        // ── File Timestamp Cache ──────────────────────────
        RuntimeEvent::FileModifiedSinceRead {
            path: std::path::PathBuf::from("/tmp/config.toml"),
            old_mtime_ms: 1_700_000_000_000,
            new_mtime_ms: 1_700_000_060_000,
        },
        // ── HITL filesystem ───────────────────────────────
        RuntimeEvent::HitlFilesystemRequired {
            request_id: "req-001".into(),
            session_id: "sess-001".into(),
            level: "medium".into(),
            op: "write".into(),
            path: "/home/alice/notes.md".into(),
            preview: FilesystemPreview::Diff {
                before: "old content".into(),
                after: "new content".into(),
                truncated: false,
            },
        },
        RuntimeEvent::AgentLog {
            task_id: "task-1".into(),
            agent_id: "agent-1".into(),
            level: "info".into(),
            message: "hello".into(),
            extra_fields_json: Some("{\"key\":\"value\"}".into()),
        },
        RuntimeEvent::Thought {
            task_id: "task-1".into(),
            agent_id: "agent-1".into(),
            step_num: 3,
            text: "I should call web_search first.".into(),
        },
        RuntimeEvent::LlmCallStarted {
            task_id: "task-1".into(),
            agent_id: "agent-1".into(),
            step_id: None,
            backend: "anthropic".into(),
            model: "claude-opus-4-7".into(),
            messages_count: 5,
            prompt_chars: 4321,
            run_id: None,
        },
        RuntimeEvent::ToolCallStarted {
            event_id: "01900000-0000-7000-8000-000000000001".into(),
            task_id: "task-1".into(),
            agent_id: "agent-1".into(),
            tool_name: "web_search".into(),
            args_json: Some("{\"query\":\"hello\"}".into()),
            run_id: None,
        },
        RuntimeEvent::ToolCallCompleted {
            parent_event_id: "01900000-0000-7000-8000-000000000001".into(),
            task_id: "task-1".into(),
            agent_id: "agent-1".into(),
            tool_name: "web_search".into(),
            output_json: Some("{\"results\":[]}".into()),
            exit_code: Some(0),
            duration_ms: 412,
            success: true,
            run_id: None,
        },
        RuntimeEvent::ToolCallDenied {
            parent_event_id: "01900000-0000-7000-8000-000000000002".into(),
            task_id: "task-1".into(),
            agent_id: "agent-1".into(),
            tool_name: "bash_executor".into(),
            reason: "permission_denied".into(),
            detail: Some("user rejected".into()),
        },
        RuntimeEvent::A2AInvokeStarted {
            event_id: "01900000-0000-7000-8000-000000000003".into(),
            correlation_id: "01900000-0000-7000-8000-c00000000001".into(),
            task_id: "task-1".into(),
            caller_agent_id: "agent-1".into(),
            skill_id: "search-and-extract".into(),
            child_task_id: Some("task-2".into()),
        },
        RuntimeEvent::A2AInvokeCompleted {
            parent_event_id: "01900000-0000-7000-8000-000000000003".into(),
            task_id: "task-1".into(),
            skill_id: "search-and-extract".into(),
            success: true,
            output_summary: Some("3 articles".into()),
            duration_ms: 1850,
        },
        RuntimeEvent::Retry {
            task_id: "task-1".into(),
            agent_id: "agent-1".into(),
            step_num: 4,
            cause: "action_parse_error".into(),
            attempt: 1,
        },
        RuntimeEvent::ActionParseError {
            task_id: "task-1".into(),
            agent_id: "agent-1".into(),
            step_num: 4,
            raw_content: "{not json".into(),
            repair_attempted: true,
        },
    ];

    // THEN: every variant is clonable and debuggable
    for event in &variants {
        let cloned = event.clone();
        let debug_str = format!("{:?}", cloned);
        assert!(!debug_str.is_empty());
    }
}

#[test]
fn test_runtime_event_debug_format() {
    // GIVEN
    let event = RuntimeEvent::AgentRegistered("agent-42".into());
    // WHEN
    let s = format!("{:?}", event);
    // THEN
    assert!(s.contains("agent-42"));
}

// ── JSON serialization ─────────────────────────────────────────

#[test]
fn test_serialisation_plan_generated() {
    // GIVEN
    let event = RuntimeEvent::PlanGenerated {
        task_id: "task-001".into(),
        agent_name: "mon-agent".into(),
        plan_id: "plan-abc".into(),
        step_count: 4,
        run_id: None,
    };
    // WHEN
    let json = serde_json::to_string(&event).expect("serialisation must succeed");
    // THEN
    assert!(json.contains("plan-abc"));
    assert!(json.contains("\"step_count\":4"));
}

#[test]
fn test_serialisation_step_started() {
    // GIVEN
    let event = RuntimeEvent::StepStarted {
        task_id: "task-001".into(),
        plan_id: "plan-abc".into(),
        step_id: "s1".into(),
        step_num: 1,
        total: 4,
        desc: "Read the file".into(),
    };
    // WHEN
    let json = serde_json::to_string(&event).expect("serialisation must succeed");
    // THEN
    assert!(json.contains("\"step_num\":1"));
    assert!(json.contains("\"total\":4"));
}

#[test]
fn test_serialisation_step_failed() {
    // GIVEN
    let event = RuntimeEvent::StepFailed {
        task_id: "task-001".into(),
        plan_id: "plan-abc".into(),
        step_id: "s2".into(),
        error: "timeout".into(),
        retryable: true,
    };
    // WHEN
    let json = serde_json::to_string(&event).expect("serialisation must succeed");
    // THEN
    assert!(json.contains("\"retryable\":true"));
}

// ── broadcast via EventBus ─────────────────────────────────────

#[tokio::test]
async fn test_broadcast_plan_generated() {
    // GIVEN
    let (tx, mut rx) = tokio::sync::broadcast::channel::<RuntimeEvent>(16);
    let event = RuntimeEvent::PlanGenerated {
        task_id: "t-001".into(),
        agent_name: "agent".into(),
        plan_id: "plan-001".into(),
        step_count: 3,
        run_id: None,
    };
    // WHEN
    tx.send(event).expect("send must succeed");
    // THEN
    let received = rx.recv().await.expect("receive must succeed");
    if let RuntimeEvent::PlanGenerated { step_count, .. } = received {
        assert_eq!(step_count, 3);
    } else {
        panic!("wrong event received");
    }
}

// ── Onboarding ─────────────────────────────────────────────────

#[test]
fn test_onboarding_required_event_serialization() {
    // GIVEN
    let event = RuntimeEvent::OnboardingRequired;
    // WHEN
    let json = serde_json::to_string(&event).expect("serialization failed");
    let restored: RuntimeEvent = serde_json::from_str(&json).expect("deserialization failed");
    // THEN
    assert!(json.contains("OnboardingRequired"));
    assert!(matches!(restored, RuntimeEvent::OnboardingRequired));
}

// ── round-trip deserialization ────────────────────────────────

#[test]
fn test_round_trip_step_failed() {
    // GIVEN
    let original = RuntimeEvent::StepFailed {
        task_id: "task-001".into(),
        plan_id: "plan-abc".into(),
        step_id: "s3".into(),
        error: "memory timeout".into(),
        retryable: true,
    };
    // WHEN
    let json = serde_json::to_string(&original).expect("serialisation must succeed");
    let deserialized: RuntimeEvent =
        serde_json::from_str(&json).expect("deserialisation must succeed");
    // THEN
    if let RuntimeEvent::StepFailed { retryable, .. } = deserialized {
        assert!(retryable);
    } else {
        panic!("wrong variant after deserialisation");
    }
}

// ── RunId ──────────────────────────────────────────────────────

#[test]
fn test_run_id_uniqueness() {
    // GIVEN two successive RunId generations
    let id1 = RunId::new();
    let id2 = RunId::new();
    // WHEN they are compared
    // THEN they differ
    assert_ne!(id1, id2);
}

#[test]
fn test_run_id_is_valid_uuid() {
    // GIVEN a generated RunId
    let id = RunId::new();
    // WHEN its string form is parsed
    // THEN it is a valid UUID v4
    uuid::Uuid::parse_str(id.as_str()).expect("run_id must be a valid UUID");
}

#[test]
fn test_plan_generated_deserializes_without_run_id() {
    // GIVEN a PlanGenerated JSON produced before run_id existed (no run_id key)
    let json =
        r#"{"PlanGenerated":{"task_id":"t1","agent_name":"foo","plan_id":"p1","step_count":3}}"#;
    // WHEN it is deserialized
    let event: RuntimeEvent = serde_json::from_str(json).expect("legacy event must deserialize");
    // THEN run_id defaults to None and the other fields are intact
    if let RuntimeEvent::PlanGenerated {
        run_id, step_count, ..
    } = event
    {
        assert!(run_id.is_none());
        assert_eq!(step_count, 3);
    } else {
        panic!("expected PlanGenerated variant");
    }
}

// ── Conversational plan-mode events ────────────────────────────

fn sample_session_plan() -> crate::plan::Plan {
    crate::plan::Plan {
        plan_id: "p-session-1".into(),
        scope: crate::plan::PlanScope::Session("sess-1".into()),
        revision: 0,
        status: crate::plan::PlanStatus::Draft,
        steps: vec![crate::plan::PlanStep::new("s1", "do the thing")],
    }
}

#[test]
fn test_plan_updated_round_trips() {
    // GIVEN a session-keyed PlanUpdated carrying a plan and its mutation
    let event = RuntimeEvent::PlanUpdated {
        session_id: "sess-1".into(),
        plan: Box::new(sample_session_plan()),
        mutation: Box::new(crate::plan::PlanMutation {
            kind: crate::plan::PlanMutationKind::Propose,
            step_id: None,
            reason: Some("first draft".into()),
            before: None,
            after: None,
            at: 1_700_000_000,
        }),
    };
    // WHEN serialized then deserialized
    let json = serde_json::to_string(&event).expect("serialize");
    let back: RuntimeEvent = serde_json::from_str(&json).expect("deserialize");
    // THEN the session, plan and mutation survive the round trip
    match back {
        RuntimeEvent::PlanUpdated {
            session_id,
            plan,
            mutation,
        } => {
            assert_eq!(session_id, "sess-1");
            assert_eq!(plan.steps.len(), 1);
            assert_eq!(mutation.kind, crate::plan::PlanMutationKind::Propose);
        }
        _ => panic!("expected PlanUpdated variant"),
    }
}

#[test]
fn test_chat_plan_decisions_construct_and_clone() {
    // GIVEN the submit, approve and reject session-keyed variants
    let events = vec![
        RuntimeEvent::PlanSubmitted {
            session_id: "sess-1".into(),
            plan: Box::new(sample_session_plan()),
        },
        RuntimeEvent::ChatPlanApproved {
            session_id: "sess-1".into(),
        },
        RuntimeEvent::ChatPlanRejected {
            session_id: "sess-1".into(),
            reason: Some("too risky".into()),
        },
    ];
    // WHEN each is cloned and serialized
    // THEN every variant round-trips without error
    for event in &events {
        let json = serde_json::to_string(&event.clone()).expect("serialize");
        let _back: RuntimeEvent = serde_json::from_str(&json).expect("deserialize");
    }
}

#[test]
fn test_chat_plan_rejected_defaults_reason_to_none() {
    // GIVEN a ChatPlanRejected JSON produced with no operator reason
    let json = r#"{"ChatPlanRejected":{"session_id":"sess-1","reason":null}}"#;
    // WHEN it is deserialized
    let event: RuntimeEvent = serde_json::from_str(json).expect("deserialize");
    // THEN reason is None and the session id is intact
    match event {
        RuntimeEvent::ChatPlanRejected { session_id, reason } => {
            assert_eq!(session_id, "sess-1");
            assert!(reason.is_none());
        }
        _ => panic!("expected ChatPlanRejected variant"),
    }
}

#[test]
fn test_plan_generated_with_run_id_round_trips() {
    // GIVEN a PlanGenerated carrying a run_id
    let run_id = RunId::new();
    let event = RuntimeEvent::PlanGenerated {
        task_id: "t1".into(),
        agent_name: "foo".into(),
        plan_id: "p1".into(),
        step_count: 2,
        run_id: Some(run_id.clone()),
    };
    // WHEN serialized then deserialized
    let json = serde_json::to_string(&event).expect("serialization failed");
    let restored: RuntimeEvent = serde_json::from_str(&json).expect("deserialization failed");
    // THEN the run_id survives the round trip
    if let RuntimeEvent::PlanGenerated { run_id: got, .. } = restored {
        assert_eq!(got, Some(run_id));
    } else {
        panic!("expected PlanGenerated variant");
    }
}
