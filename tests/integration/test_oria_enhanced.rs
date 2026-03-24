//! Integration tests — Sprint 20 ORIA Enhanced features (STORY-237).
//!
//! 6 independent test scenarios covering:
//! 1. Tool describe E2E
//! 2. Multi-model dispatch via LlmRouter
//! 3. Per-step observation (episodic memory per step)
//! 4. Plan cache hit
//! 5. Weighted classifier (Observer classify)
//! 6. Agent mailbox round-trip
//!
//! These tests do NOT require Python. They run in all CI environments.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use apollia_core::{
    AIPInput, AIPPart, AIPTask, AgentManifest, EventBusSender, RuntimeEvent, SandboxProfile,
    TextPart,
};
use apollia_llm::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, LlmRouter,
    ObservabilityConfig, StreamChunk, TokenUsage,
};
use apollia_memory::episodic::EpisodicMemory;
use apollia_memory::manager::MemoryManager;
use apollia_oria::observer::{classify, ExecutionMode};
use apollia_oria::plan::ExecutionPlan;
use apollia_oria::plan_cache::{compute_cache_key, PlanCacheRepository};
use apollia_runtime::mailbox::AgentMailboxHandle;
use apollia_tools::descriptor::{ToolDescriptor, ToolKind};
use apollia_tools::registry::ToolRegistryHandle;

// ─────────────────────────────────────────────
// Mock CompletionModel for multi-model dispatch
// ─────────────────────────────────────────────

/// Mock backend that records its name in the response content so tests can
/// verify which backend was actually called.
struct TrackableBackend {
    name: String,
}

#[async_trait::async_trait]
impl CompletionModel for TrackableBackend {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            content: format!("response-from-{}", self.name),
            tool_calls: vec![],
            usage: TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                cost_usd: None,
            },
            finish_reason: FinishReason::Stop,
            latency_ms: 1,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
    {
        Ok(Box::pin(futures::stream::once(async {
            Ok(StreamChunk::Text("mock".to_owned()))
        })))
    }

    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &str {
        &self.name
    }

    fn model_id(&self) -> &str {
        &self.name
    }
}

// ─────────────────────────────────────────────
// Helper: build a minimal AgentManifest
// ─────────────────────────────────────────────

fn make_manifest(
    name: &str,
    tools_required: Vec<String>,
    max_steps: u32,
    tags: Vec<String>,
    execution_mode: &str,
) -> AgentManifest {
    AgentManifest {
        name: name.to_owned(),
        version: "1.0.0".to_owned(),
        description: format!("Test agent {name}"),
        tools_required,
        tools_optional: vec![],
        supports_streaming: false,
        supports_a2a: false,
        memory_namespace: None,
        shared_memory_namespaces: vec![],
        max_concurrent_tasks: 1,
        step_budget: Some(apollia_core::StepBudgetConfig {
            max_steps,
            max_tool_calls: 20,
            wall_clock_secs: 300,
        }),
        network_allowlist: None,
        dangerous_tools_allowed: false,
        tags,
        skills: vec![],
        execution_mode: execution_mode.to_owned(),
        system_prompt: None,
        tools_requiring_approval: vec![],
    }
}

fn make_task(text: &str) -> AIPTask {
    AIPTask {
        task_id: uuid::Uuid::new_v4().to_string(),
        context_id: "ctx-test".to_owned(),
        input: AIPInput {
            parts: vec![AIPPart::Text(TextPart {
                text: text.to_owned(),
            })],
        },
        history: vec![],
        timeout_seconds: None,
        is_resumed: false,
        input_response: None,
    }
}

fn make_event_bus() -> EventBusSender {
    let (tx, _) = tokio::sync::broadcast::channel(128);
    tx
}

// ─────────────────────────────────────────────
// Test 1: Tool describe E2E
// ─────────────────────────────────────────────

#[tokio::test]
async fn test_tool_describe_e2e() {
    // GIVEN a ToolRegistry with a registered tool carrying a JSON schema
    let registry = ToolRegistryHandle::start();
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "command": { "type": "string" },
            "timeout_ms": { "type": "integer" }
        },
        "required": ["command"]
    });

    let descriptor = ToolDescriptor {
        name: "integration_tool".to_owned(),
        version: "2.1.0".to_owned(),
        description: "Tool for integration testing".to_owned(),
        kind: ToolKind::Native,
        input_schema: schema.clone(),
        output_schema: Some(serde_json::json!({ "type": "string" })),
        sandbox_profile: SandboxProfile::FileSystem,
        tags: vec!["test".to_owned(), "integration".to_owned()],
        dangerous: false,
    };

    registry
        .register(descriptor)
        .await
        .expect("register should succeed");

    // WHEN we call describe via the handle
    let result = registry.describe("integration_tool").await;

    // THEN the returned descriptor matches the registered one
    let desc = result.expect("describe should return Some for a registered tool");
    assert_eq!(desc.name, "integration_tool");
    assert_eq!(desc.version, "2.1.0");
    assert_eq!(desc.description, "Tool for integration testing");
    assert_eq!(desc.input_schema, schema);
    assert_eq!(
        desc.output_schema,
        Some(serde_json::json!({ "type": "string" }))
    );
    assert_eq!(desc.tags, vec!["test", "integration"]);

    // AND describe for unknown tool returns None
    let unknown = registry.describe("nonexistent").await;
    assert!(
        unknown.is_none(),
        "describe should return None for unknown tool"
    );

    registry.shutdown().await;
}

// ─────────────────────────────────────────────
// Test 2: Multi-model dispatch
// ─────────────────────────────────────────────

#[tokio::test]
async fn test_multi_model_dispatch() {
    // GIVEN a LlmRouter with 2 backends: "backend-a" (default) and "backend-b"
    let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
    backends.insert(
        "backend-a".into(),
        Arc::new(TrackableBackend {
            name: "backend-a".to_owned(),
        }),
    );
    backends.insert(
        "backend-b".into(),
        Arc::new(TrackableBackend {
            name: "backend-b".to_owned(),
        }),
    );
    let router = LlmRouter::with_backends(backends, "backend-a");
    let obs = ObservabilityConfig::default();
    let bus = make_event_bus();

    // WHEN we submit a request targeting "backend-b" (not the default)
    let response = router
        .complete_with_observability(
            Some("backend-b"),
            CompletionRequest::default(),
            Some(&bus),
            &obs,
        )
        .await
        .expect("completion should succeed");

    // THEN the response comes from backend-b (verified via content marker)
    assert_eq!(
        response.content, "response-from-backend-b",
        "request should have been routed to backend-b, not the default"
    );

    // AND the default backend returns its own marker when no hint is given
    let default_response = router
        .complete_with_observability(None, CompletionRequest::default(), Some(&bus), &obs)
        .await
        .expect("default completion should succeed");
    assert_eq!(
        default_response.content, "response-from-backend-a",
        "request without hint should go to the default backend"
    );
}

// ─────────────────────────────────────────────
// Test 3: Per-step observation (episodic memory)
// ─────────────────────────────────────────────

#[tokio::test]
async fn test_per_step_observation() {
    // GIVEN a MemoryManager with a writable namespace and 3 step records
    let tmp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let namespace = "test-agent-ns";
    let mut mm = MemoryManager::new(tmp_dir.path(), Some(namespace.to_owned()), vec![]);

    let store = mm
        .store(namespace)
        .expect("store should open for primary namespace");
    let episodic = EpisodicMemory::new(store);

    // Simulate 3 steps recording episodic memory (as ActorLoop.record_step_memory does)
    let steps = vec![
        ("s1", "Fetch user data", "user={name: Alice}"),
        ("s2", "Analyze preferences", "prefs=[coding, music]"),
        ("s3", "Generate report", "report=Summary for Alice"),
    ];

    for (step_id, description, output) in &steps {
        let content = format!("step {step_id}: {description} -> {output}");
        let metadata = serde_json::json!({
            "source": "oria_orchestrated",
            "step_id": step_id,
        });
        episodic
            .record(
                namespace,
                "test-agent",
                &content,
                0.6, // STEP_MEMORY_IMPORTANCE
                Some("task-42"),
                None,
                Some(&metadata),
            )
            .expect("episodic record should succeed");
    }

    // WHEN we read back the episodic history
    let entries = episodic
        .history(namespace, 10, None)
        .expect("history query should succeed");

    // THEN there is exactly one entry per step
    assert_eq!(
        entries.len(),
        3,
        "expected one episodic entry per step, got {}",
        entries.len()
    );

    // AND each entry contains the step content with correct metadata
    for entry in &entries {
        assert!(
            entry.content.starts_with("step s"),
            "entry should start with 'step s': {}",
            entry.content
        );
        assert_eq!(entry.task_id.as_deref(), Some("task-42"));
        let meta: serde_json::Value = serde_json::from_str(&entry.metadata.to_string())
            .expect("metadata should be valid JSON");
        assert_eq!(meta["source"], "oria_orchestrated");
        assert!(
            meta["step_id"].as_str().is_some(),
            "metadata should contain step_id"
        );
    }
}

// ─────────────────────────────────────────────
// Test 4: Plan cache hit
// ─────────────────────────────────────────────

#[tokio::test]
async fn test_plan_cache_hit() {
    // GIVEN a PlanCacheRepository with a stored plan
    let tmp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = tmp_dir.path().join("plan_cache.db");
    let cache = PlanCacheRepository::open(&db_path).expect("cache open should succeed");

    let tools = vec!["tool_a".to_owned(), "tool_b".to_owned()];
    let task_text = "Generate a summary report for Q1 results";

    let cache_key = compute_cache_key("my-agent", "1.0.0", &tools, task_text);

    let plan = ExecutionPlan {
        plan_id: "plan-original".to_owned(),
        task_id: "task-1".to_owned(),
        steps: vec![
            apollia_oria::plan::PlanStep {
                step_id: "s1".to_owned(),
                description: "Fetch Q1 data".to_owned(),
                tool_hint: Some("tool_a".to_owned()),
                depends_on: vec![],
                model_hint: None,
            },
            apollia_oria::plan::PlanStep {
                step_id: "s2".to_owned(),
                description: "Generate summary".to_owned(),
                tool_hint: Some("tool_b".to_owned()),
                depends_on: vec!["s1".to_owned()],
                model_hint: None,
            },
        ],
    };

    cache
        .store(&cache_key, &plan, "my-agent", "1.0.0")
        .expect("store should succeed");

    // WHEN we look up the same cache key
    let lookup_result = cache.lookup(&cache_key).expect("lookup should not error");

    // THEN we get a cache hit with the stored plan
    let cached_plan = lookup_result.expect("lookup should return Some (cache hit)");
    assert_eq!(cached_plan.steps.len(), 2);
    assert_eq!(cached_plan.steps[0].step_id, "s1");
    assert_eq!(cached_plan.steps[1].step_id, "s2");
    assert_eq!(cached_plan.steps[0].description, "Fetch Q1 data");
    assert_eq!(cached_plan.steps[1].depends_on, vec!["s1"]);

    // AND a different task text produces a cache miss
    let different_key = compute_cache_key("my-agent", "1.0.0", &tools, "Completely different task");
    let miss = cache
        .lookup(&different_key)
        .expect("lookup should not error");
    assert!(
        miss.is_none(),
        "different task text should produce a cache miss"
    );

    // AND the EventBus receives PlanCacheHit when the runtime emits it
    let bus = make_event_bus();
    let mut rx = bus.subscribe();
    let _ = bus.send(RuntimeEvent::PlanCacheHit {
        task_id: "task-2".to_owned().into(),
        cache_key: cache_key.clone(),
    });
    let event = rx.try_recv().expect("should receive PlanCacheHit event");
    assert!(
        matches!(event, RuntimeEvent::PlanCacheHit { .. }),
        "event should be PlanCacheHit"
    );
}

// ─────────────────────────────────────────────
// Test 5: Weighted classifier
// ─────────────────────────────────────────────

#[tokio::test]
async fn test_weighted_classifier() {
    // GIVEN a simple agent (2 tools, 10 steps, no tags) — score < 0.40
    let simple_manifest = make_manifest(
        "simple-agent",
        vec!["tool_a".to_owned(), "tool_b".to_owned()],
        10,
        vec![],
        "auto",
    );
    let simple_task = make_task("Hello world");

    // WHEN classified
    let mode = classify(&simple_task, &simple_manifest, None);

    // THEN Direct (score = 0.0, well below 0.40)
    assert_eq!(
        mode,
        ExecutionMode::Direct,
        "simple agent with 2 tools and 10 steps should be Direct"
    );

    // GIVEN a complex agent (5+ tools, 20 steps, "multi-step" tag) — score >= 0.40
    let complex_manifest = make_manifest(
        "complex-agent",
        vec![
            "t1".to_owned(),
            "t2".to_owned(),
            "t3".to_owned(),
            "t4".to_owned(),
            "t5".to_owned(),
        ],
        20,
        vec!["multi-step".to_owned()],
        "auto",
    );
    let complex_task = make_task("Build a comprehensive analysis pipeline");

    // WHEN classified
    let mode = classify(&complex_task, &complex_manifest, None);

    // THEN Orchestrated (tag=0.40 + tools=0.20 + steps=0.30 = 0.90)
    assert_eq!(
        mode,
        ExecutionMode::Orchestrated,
        "complex agent with 5 tools, 20 steps, and multi-step tag should be Orchestrated"
    );

    // GIVEN an agent with only the "multi-step" tag — score = 0.40 (boundary)
    let tag_only_manifest = make_manifest(
        "tag-agent",
        vec!["tool_a".to_owned()],
        10,
        vec!["multi-step".to_owned()],
        "auto",
    );
    let tag_task = make_task("Simple task with multi-step tag");

    // WHEN classified
    let mode = classify(&tag_task, &tag_only_manifest, None);

    // THEN Orchestrated (score = 0.40 >= threshold 0.40)
    assert_eq!(
        mode,
        ExecutionMode::Orchestrated,
        "agent with only multi-step tag should be Orchestrated (score = 0.40 at threshold)"
    );
}

// ─────────────────────────────────────────────
// Test 6: Agent mailbox round-trip
// ─────────────────────────────────────────────

#[tokio::test]
async fn test_agent_mailbox_round_trip() {
    // GIVEN an AgentMailbox actor is running
    let bus = make_event_bus();
    let mailbox = AgentMailboxHandle::spawn(bus);

    let payload = serde_json::json!({
        "action": "review",
        "document_id": "doc-42",
        "priority": "high"
    });

    // WHEN agent-a sends a message to agent-b
    mailbox
        .send("agent-a", "agent-b", payload.clone())
        .await
        .expect("send should succeed");

    // THEN agent-b receives the message with correct metadata
    let msg = mailbox
        .receive("agent-b", Duration::from_secs(1))
        .await
        .expect("receive should return a message");

    assert_eq!(msg.from, "agent-a", "message sender should be agent-a");
    assert_eq!(
        msg.payload, payload,
        "message payload should match what was sent"
    );
    assert!(
        !msg.sent_at.is_empty(),
        "sent_at should be a non-empty timestamp"
    );

    // AND the pending count for agent-b is now 0 (message consumed)
    let pending = mailbox.pending_count("agent-b").await;
    assert_eq!(pending, 0, "pending count should be 0 after receive");

    // AND agent-a has no pending messages
    let no_msg = mailbox.receive("agent-a", Duration::from_millis(50)).await;
    assert!(no_msg.is_none(), "agent-a should have no pending messages");

    mailbox.shutdown().await;
}
