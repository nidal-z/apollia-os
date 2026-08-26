#![allow(clippy::unwrap_used, clippy::expect_used)]
use apollia_core::{AgentManifest, ProcessState, RuntimeEvent};
use apollia_runtime::{AgentRegistry, EventBus};
use tokio::time::{timeout, Duration};

fn make_manifest(name: &str) -> AgentManifest {
    AgentManifest {
        format_version: 1,
        name: name.to_string(),
        version: "0.1.0".to_string(),
        description: String::new(),
        tools_required: vec![],
        tools_optional: vec![],
        supports_streaming: false,
        supports_a2a: false,
        memory_namespace: None,
        shared_memory_namespaces: vec![],
        max_concurrent_tasks: 1,
        step_budget: None,
        network_allowlist: None,
        dangerous_tools_allowed: false,
        tags: vec![],
        skills: vec![],
        execution_mode: "auto".to_string(),
        supports_mailbox: false,
        mailbox_allowlist: None,
        system_prompt: None,
        tools_requiring_approval: vec![],
        llm_backend: None,
        packages: vec![],
        memory_config: None,
        agent_type: None,
        examples: vec![],
        limitations: vec![],
        setup_notes: None,
        agent_class: None,
        user_memory_write: false,
        datasources: vec![],
        templates: vec![],
        secrets: vec![],
        check_commands: vec![],
    }
}

/// Collects `count` events from the receiver, with a timeout.
///
/// Returns fewer than `count` when the timeout expires first.
async fn collect_events(
    rx: &mut tokio::sync::broadcast::Receiver<RuntimeEvent>,
    count: usize,
    timeout_ms: u64,
) -> Vec<RuntimeEvent> {
    let mut events = Vec::with_capacity(count);
    for _ in 0..count {
        match timeout(Duration::from_millis(timeout_ms), rx.recv()).await {
            Ok(Ok(event)) => events.push(event),
            _ => break,
        }
    }
    events
}

/// Full life cycle of one agent.
///
/// Transitions : register → Active → Degraded → Active → Stopping → Stopped → unregister
/// Expected events (7):
///   AgentRegistered → AgentReady → AgentDegraded → AgentReady → AgentStopping → AgentStopped → AgentStopped
#[tokio::test]
async fn test_cycle_de_vie_complet() {
    // GIVEN
    let (bus_tx, mut bus_rx) = EventBus::new();
    let registry = AgentRegistry::spawn(bus_tx);

    // WHEN - cycle complet
    let id = registry
        .register(make_manifest("agent-lifecycle"))
        .await
        .unwrap();
    registry
        .update_state(id.as_str(), ProcessState::Active)
        .await
        .unwrap();
    registry
        .update_state(id.as_str(), ProcessState::Degraded)
        .await
        .unwrap();
    registry
        .update_state(id.as_str(), ProcessState::Active)
        .await
        .unwrap();
    registry
        .update_state(id.as_str(), ProcessState::Stopping)
        .await
        .unwrap();
    registry
        .update_state(id.as_str(), ProcessState::Stopped)
        .await
        .unwrap();
    registry.unregister(id.as_str()).await.unwrap();

    // THEN - 7 events, in that exact order
    let events = collect_events(&mut bus_rx, 7, 200).await;
    assert_eq!(
        events.len(),
        7,
        "expected 7 events, received {}",
        events.len()
    );

    assert!(matches!(&events[0], RuntimeEvent::AgentRegistered(eid) if eid == &id));
    assert!(matches!(&events[1], RuntimeEvent::AgentReady(eid) if eid == &id));
    assert!(matches!(&events[2], RuntimeEvent::AgentDegraded { agent_id, .. } if agent_id == &id));
    assert!(matches!(&events[3], RuntimeEvent::AgentReady(eid) if eid == &id));
    assert!(matches!(&events[4], RuntimeEvent::AgentStopping(eid) if eid == &id));
    assert!(matches!(&events[5], RuntimeEvent::AgentStopped(eid) if eid == &id));
    assert!(matches!(&events[6], RuntimeEvent::AgentStopped(eid) if eid == &id));
}

/// Several agents at once.
#[tokio::test]
async fn test_agents_simultanes() {
    // GIVEN
    let (bus_tx, mut bus_rx) = EventBus::new();
    let registry = AgentRegistry::spawn(bus_tx);
    let r2 = registry.clone();
    let r3 = registry.clone();

    // WHEN - 3 registrations concurrentes
    let (id1, id2, id3) = tokio::join!(
        registry.register(make_manifest("agent-a")),
        r2.register(make_manifest("agent-b")),
        r3.register(make_manifest("agent-c")),
    );

    assert!(id1.is_ok() && id2.is_ok() && id3.is_ok());

    // THEN - list_agents returns exactly 3 entries
    let agents = registry.list_agents().await.unwrap();
    assert_eq!(agents.len(), 3);

    // AND - the bus received 3 AgentRegistered events
    let events = collect_events(&mut bus_rx, 3, 200).await;
    assert_eq!(events.len(), 3);
    assert!(events
        .iter()
        .all(|e| matches!(e, RuntimeEvent::AgentRegistered(_))));
}

/// An invalid transition does not alter the state.
#[tokio::test]
async fn test_transition_invalide_preserve_etat() {
    // GIVEN
    let (bus_tx, mut bus_rx) = EventBus::new();
    let registry = AgentRegistry::spawn(bus_tx);
    let id = registry
        .register(make_manifest("agent-stable"))
        .await
        .unwrap();
    registry
        .update_state(id.as_str(), ProcessState::Active)
        .await
        .unwrap();

    // Drain the 2 events already published (AgentRegistered + AgentReady)
    collect_events(&mut bus_rx, 2, 100).await;

    // WHEN - transition invalide Active → Initializing
    let result = registry
        .update_state(id.as_str(), ProcessState::Initializing)
        .await;

    // THEN - an InvalidTransition error is returned
    assert!(matches!(
        result.unwrap_err(),
        apollia_runtime::AgentRegistryError::InvalidTransition { .. }
    ));

    // AND - the state is still Active
    let entry = registry.get_agent(id.as_str()).await.unwrap().unwrap();
    assert!(matches!(entry.process_state, ProcessState::Active));

    // AND - no extra event is published
    let extra_events = collect_events(&mut bus_rx, 1, 50).await;
    assert!(extra_events.is_empty());
}

/// Unregister of an unknown agent.
#[tokio::test]
async fn test_unregister_agent_inconnu() {
    // GIVEN
    let (bus_tx, _) = EventBus::new();
    let registry = AgentRegistry::spawn(bus_tx);

    // WHEN
    let result = registry.unregister("ghost-id").await;

    // THEN
    assert!(matches!(
        result.unwrap_err(),
        apollia_runtime::AgentRegistryError::NotFound(id) if id == "ghost-id"
    ));
}
