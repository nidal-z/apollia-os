use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use apollia_core::events::RuntimeEvent;
use apollia_core::model_preset::{AgentModel, ModelSource};
use apollia_core::{LlmBackendRepository, LlmRoutingConfig};
use futures::Stream;

use crate::router::LlmRouter;
use crate::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, StreamChunk,
    TokenUsage,
};
use crate::AgentModelBinding;

struct Named(&'static str);

#[async_trait::async_trait]
impl CompletionModel for Named {
    async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            engine_timings: None,
            content: self.0.to_string(),
            tool_calls: vec![],
            usage: TokenUsage::default(),
            finish_reason: FinishReason::Stop,
            latency_ms: 1,
            ttft_ms: None,
        })
    }

    async fn stream(
        &self,
        _req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        let name = self.0.to_string();
        Ok(Box::pin(futures::stream::once(async move {
            Ok(StreamChunk::Text(name))
        })))
    }

    fn is_available(&self) -> bool {
        true
    }
    fn backend_name(&self) -> &str {
        self.0
    }
    fn model_id(&self) -> &str {
        self.0
    }
}

fn router(names: &[&'static str], default: &str) -> LlmRouter {
    let backends: HashMap<String, Arc<dyn CompletionModel>> = names
        .iter()
        .map(|n| {
            (
                n.to_string(),
                Arc::new(Named(n)) as Arc<dyn CompletionModel>,
            )
        })
        .collect();
    LlmRouter::with_backends(backends, default)
}

fn assign(db: &std::path::Path, agent: &str, model: &str) {
    let repo = LlmBackendRepository::open(db).unwrap();
    repo.set_agent_model(&AgentModel {
        agent: agent.into(),
        model_source: ModelSource::Local,
        model_id: model.into(),
        preset_id: None,
    })
    .unwrap();
}

// GIVEN a router holding `default`, `declared` and `assigned`
// WHEN an agent's model is resolved with every combination of assignment and
//      manifest declaration
// THEN the assignment wins over the declaration, the declaration over the
//      default, and nothing asked for leaves the default in force (`None`)
#[test]
fn test_assignment_then_manifest_then_default() {
    // GIVEN
    let router = router(&["default", "declared", "assigned"], "default");

    // WHEN
    let both = router.resolve_agent_model("a", Some("assigned"), Some("declared"), None);
    let declared_only = router.resolve_agent_model("a", None, Some("declared"), None);
    let nothing = router.resolve_agent_model("a", None, None, None);
    let blank = router.resolve_agent_model("a", Some("  "), None, None);

    // THEN
    assert_eq!(both.as_deref(), Some("assigned"));
    assert_eq!(declared_only.as_deref(), Some("declared"));
    assert_eq!(nothing, None);
    assert_eq!(blank, None);
    assert_eq!(
        router.agent_model_fallback("a"),
        None,
        "no fallback happened"
    );
}

// GIVEN an agent assigned a model the router cannot reach, and a bus
// WHEN its model is resolved
// THEN the call runs on the default (`None`), the bus carries
//      `LlmAgentModelFallback` naming the agent, the model asked for and the one
//      used, and the router keeps it as the agent's last fallback
#[test]
fn test_missing_model_falls_back_and_says_so() {
    // GIVEN
    let router = router(&["default"], "default");
    let (bus, mut rx) = tokio::sync::broadcast::channel(8);

    // WHEN
    let resolved = router.resolve_agent_model("mail", Some("gone"), Some("declared"), Some(&bus));

    // THEN
    assert_eq!(resolved, None);
    match rx.try_recv().unwrap() {
        RuntimeEvent::LlmAgentModelFallback {
            agent,
            requested,
            used,
        } => {
            assert_eq!(agent, "mail");
            assert_eq!(requested, "gone");
            assert_eq!(used, "default");
        }
        other => panic!("unexpected event {other:?}"),
    }
    let last = router.agent_model_fallback("mail").unwrap();
    assert_eq!(
        (last.requested.as_str(), last.used.as_str()),
        ("gone", "default")
    );
    assert_eq!(router.agent_model_fallback("other"), None);
}

// GIVEN a binding over a real `system.db`, the manifest declaring `declared`
// WHEN the user assigns `assigned`, then a model that is gone, then clears the
//      assignment, resolving once after each change with no rebuild in between
// THEN each call follows the database as it is at that call
#[test]
fn test_binding_reads_the_assignment_at_each_call() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("system.db");
    let router = router(&["default", "declared", "assigned"], "default");
    let binding = AgentModelBinding::new("notes", Some("declared".into()), Some(db.clone()));
    let before = binding.resolve(&router, None);

    // WHEN
    assign(&db, "notes", "assigned");
    let after_assign = binding.resolve(&router, None);
    assign(&db, "notes", "gone");
    let after_gone = binding.resolve(&router, None);
    LlmBackendRepository::open(&db)
        .unwrap()
        .clear_agent_model("notes")
        .unwrap();
    let after_clear = binding.resolve(&router, None);

    // THEN
    assert_eq!(before.as_deref(), Some("declared"));
    assert_eq!(after_assign.as_deref(), Some("assigned"));
    assert_eq!(after_gone, None);
    assert_eq!(after_clear.as_deref(), Some("declared"));
}

// GIVEN a binding whose database does not exist
// WHEN its assignment is read
// THEN it reads as none, and the database is not created
#[test]
fn test_binding_without_database_reads_no_assignment() {
    // GIVEN
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("system.db");
    let binding = AgentModelBinding::new("notes", None, Some(db.clone()));

    // WHEN
    let assigned = binding.assigned();

    // THEN
    assert_eq!(assigned, None);
    assert!(!db.exists());
}

// GIVEN a running agent holding the boot router, a router carrying
//       `[llm.routing]`
// WHEN two reloads each build a router and supersede the one in force
// THEN the agent reaches the last one through `latest`, and the routing and
//      the agent's last fallback survive both reloads
#[test]
fn test_reload_reaches_a_holder_and_keeps_routing() {
    // GIVEN
    let routing = LlmRoutingConfig {
        precise: "default".into(),
        fast: "default".into(),
        hybrid: None,
        format_version: 1,
    };
    let boot = Arc::new(router(&["default"], "default").with_routing(routing));
    boot.resolve_agent_model("mail", Some("gone"), None, None);
    let held_by_agent = Arc::clone(&boot);

    // WHEN
    let first = Arc::new(router(&["default", "first"], "default").carry_over_from(&boot));
    boot.supersede(&first);
    let second = Arc::new(router(&["default", "second"], "default").carry_over_from(&first));
    first.supersede(&second);

    // THEN
    let seen = held_by_agent.latest();
    assert!(Arc::ptr_eq(&seen, &second));
    assert_eq!(seen.routing().map(|r| r.precise.as_str()), Some("default"));
    assert_eq!(
        seen.agent_model_fallback("mail").map(|f| f.requested),
        Some("gone".to_string())
    );
    assert!(Arc::ptr_eq(&boot.latest(), &second));
    assert!(Arc::ptr_eq(&second.latest(), &second));
}

// GIVEN a router superseded once, then superseded again through the same
//       (already replaced) router, and a clone sharing its state
// WHEN each is asked for the router in force
// THEN all reach the last one, and superseding a router by itself is a no-op
#[test]
fn test_supersede_appends_to_the_chain() {
    // GIVEN
    let boot = Arc::new(router(&["default"], "default"));
    let clone = Arc::new((*boot).clone());
    let first = Arc::new(router(&["default"], "default"));
    let second = Arc::new(router(&["default"], "default"));

    // WHEN
    boot.supersede(&first);
    boot.supersede(&second);
    second.supersede(&second);

    // THEN
    assert!(Arc::ptr_eq(&first.latest(), &second));
    assert!(Arc::ptr_eq(&boot.latest(), &second));
    assert!(Arc::ptr_eq(&clone.latest(), &second));
    assert!(Arc::ptr_eq(&second.latest(), &second));
}

// GIVEN a router rebuilt by a reload, a fallback recorded on the old one after
//       the rebuild and before the swap, and an agent still holding the old one
// WHEN the swap happens and the agent's call falls back again on the old one
// THEN the router in force sees both fallbacks, the later one last
#[test]
fn test_fallbacks_reach_the_router_in_force() {
    // GIVEN
    let boot = Arc::new(router(&["default"], "default"));
    let next = Arc::new(router(&["default"], "default").carry_over_from(&boot));
    boot.resolve_agent_model("mail", Some("gone"), None, None);
    let held_by_agent = Arc::clone(&boot);

    // WHEN
    boot.supersede(&next);
    let handed = next.agent_model_fallback("mail").map(|f| f.requested);
    held_by_agent.resolve_agent_model("mail", Some("also-gone"), None, None);

    // THEN
    assert_eq!(handed, Some("gone".to_string()));
    assert_eq!(
        next.agent_model_fallback("mail").map(|f| f.requested),
        Some("also-gone".to_string())
    );
    assert_eq!(
        held_by_agent
            .agent_model_fallback("mail")
            .map(|f| f.requested),
        Some("also-gone".to_string())
    );
}

// GIVEN a router whose `[llm.routing] precise` is `big`, holding `small`
// WHEN the reasoning backend of an orchestrated run is resolved for an agent
//      declaring `small`, one declaring a model that is gone, and one declaring
//      nothing
// THEN the first reasons on `small`, the second on the default, and the third
//      on `big` as before
#[test]
fn test_orchestrated_run_reasons_on_the_agents_model() {
    // GIVEN
    let routing = LlmRoutingConfig {
        precise: "big".into(),
        fast: "default".into(),
        hybrid: None,
        format_version: 1,
    };
    let router = router(&["default", "big", "small"], "default").with_routing(routing);
    let model_of = |declared: Option<&str>| {
        let binding = AgentModelBinding::new("a", declared.map(String::from), None);
        router
            .route_precise_for(&binding, None)
            .unwrap()
            .model_id()
            .to_string()
    };

    // WHEN
    let own = model_of(Some("small"));
    let gone = model_of(Some("gone"));
    let nothing = model_of(None);

    // THEN
    assert_eq!(own, "small");
    assert_eq!(gone, "default");
    assert_eq!(nothing, "big");
}
