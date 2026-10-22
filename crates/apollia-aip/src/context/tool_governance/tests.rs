//! A tool called from Python is decided by governance like a call of the
//! Chat, of a plan or of a batch item: refusals come back with their reason,
//! a sensitive call follows the consent rule of the agent, and a connection
//! in pause or forbidden by the policy refuses every one of its tools.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use apollia_core::events::RuntimeEvent;
use apollia_core::SandboxProfile;
use apollia_permissions::{ConsentFamily, PrefixRule, RuleAction};
use apollia_runtime::governance::{
    AccessCall, AccessDecision, DeclaredAccess, EffectiveAccess, GovernanceContract,
    GovernanceError, GovernanceEvent, GovernanceHandle, GovernanceState, InstallSubject,
    PolicyRefusals,
};
use apollia_tools::{ActionFamily, AuditTrailHandle, ToolDescriptor, ToolKind, ToolRegistryHandle};
use pyo3::prelude::*;

use crate::context::tool_proxy::ToolProxyError;
use crate::context::{ToolExecutor, ToolGovernance, ToolProxy, ToolProxyConfig};

/// The agent whose workflow calls the tools.
const AGENT: &str = "quotes";
/// A tool that reads.
const LOOKUP: &str = "mcp:crm/lookup";
/// A sensitive tool: it sends outside the machine.
const SEND: &str = "mcp:crm/send_quote";

/// Counts the calls that actually ran.
struct Counting(Arc<Mutex<Vec<String>>>);

impl ToolExecutor for Counting {
    fn execute(
        &self,
        tool_name: &str,
        _input: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        if let Ok(mut ran) = self.0.lock() {
            ran.push(tool_name.to_string());
        }
        Ok(serde_json::json!({ "ran": tool_name }))
    }
}

/// Governance over a fixed state, keeping what it is asked to record.
struct Recording {
    inner: GovernanceHandle,
    recorded: Mutex<Vec<GovernanceEvent>>,
}

impl GovernanceContract for Recording {
    fn effective_accesses(
        &self,
        agent_id: &str,
        declared: &[DeclaredAccess],
    ) -> Vec<EffectiveAccess> {
        self.inner.effective_accesses(agent_id, declared)
    }
    fn decide(&self, call: &AccessCall<'_>) -> AccessDecision {
        self.inner.decide(call)
    }
    fn signature_required(&self, subject: &InstallSubject<'_>) -> bool {
        self.inner.signature_required(subject)
    }
    fn record(&self, event: &GovernanceEvent) -> Result<(), GovernanceError> {
        if let Ok(mut recorded) = self.recorded.lock() {
            recorded.push(event.clone());
        }
        Ok(())
    }
}

fn descriptor(name: &str, family: ActionFamily) -> ToolDescriptor {
    ToolDescriptor {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        description: format!("tool {name}"),
        kind: ToolKind::Native,
        input_schema: serde_json::json!({ "type": "object" }),
        output_schema: None,
        sandbox_profile: SandboxProfile::FileSystem,
        tags: vec![],
        family,
        family_source: apollia_tools::FamilySource::Unsourced,
        target_arg: None,
        is_read_only: family == ActionFamily::Read,
        risk_score: 1,
        approval_risk_level: None,
        impact_description: None,
        reject_reason_required: false,
    }
}

/// What a test reads back after the calls.
struct Fixture {
    proxy: ToolProxy,
    registry: ToolRegistryHandle,
    audit: AuditTrailHandle,
    ran: Arc<Mutex<Vec<String>>>,
    governance: Arc<Recording>,
}

impl Fixture {
    fn ran(&self) -> Vec<String> {
        self.ran.lock().map(|r| r.clone()).unwrap_or_default()
    }
    fn recorded(&self) -> Vec<GovernanceEvent> {
        self.governance
            .recorded
            .lock()
            .map(|r| r.clone())
            .unwrap_or_default()
    }
    async fn shutdown(self) {
        self.registry.shutdown().await;
        self.audit.shutdown().await;
    }
}

/// A proxy of [`AGENT`] granted [`LOOKUP`] and [`SEND`], deciding its calls
/// through governance over `state`, the run holding `run_grants`.
async fn fixture(state: GovernanceState, run_grants: &[&str]) -> Fixture {
    let registry = ToolRegistryHandle::start();
    registry
        .register(descriptor(LOOKUP, ActionFamily::Read))
        .await
        .unwrap();
    registry
        .register(descriptor(SEND, ActionFamily::SendExternal))
        .await
        .unwrap();
    let audit = AuditTrailHandle::open(&tempfile::tempdir().unwrap().keep().join("audit.db"))
        .await
        .unwrap();
    let ran = Arc::new(Mutex::new(Vec::new()));
    let governance = Arc::new(Recording {
        inner: GovernanceHandle::detached(state, None),
        recorded: Mutex::new(Vec::new()),
    });
    let contract: Arc<dyn GovernanceContract> = governance.clone();
    let proxy = ToolProxy::new(ToolProxyConfig {
        registry: registry.clone(),
        audit: audit.clone(),
        executor: Arc::new(Counting(Arc::clone(&ran))),
        allowed_tools: vec![LOOKUP.to_string(), SEND.to_string()],
        agent_id: AGENT.to_string(),
        task_id: "task-workflow".to_string(),
        run_id: None,
    })
    .with_governance(
        ToolGovernance::new(contract).with_run_grants(run_grants.iter().map(|g| g.to_string())),
    );
    Fixture {
        proxy,
        registry,
        audit,
        ran,
        governance,
    }
}

// GIVEN a workflow of an agent whose tools are both in its access list, and a
// policy that refuses the sensitive one by name
// WHEN the workflow calls each of them
// THEN the read runs, the refused tool does not run, its refusal carries the
// reason of the policy, and the refusal is journaled and audited
#[tokio::test]
async fn test_a_tool_the_policy_refuses_is_refused_to_the_workflow_with_its_reason() {
    let state = GovernanceState {
        policy: PolicyRefusals {
            denied: HashSet::from([SEND.to_string()]),
            ..Default::default()
        },
        ..Default::default()
    };
    let fx = fixture(state, &[]).await;

    let read = fx.proxy.call_inner(LOOKUP, serde_json::json!({})).await;
    let sent = fx.proxy.call_inner(SEND, serde_json::json!({})).await;
    let records = fx.audit.query_last(2).await;

    assert_eq!(read.unwrap(), serde_json::json!({ "ran": LOOKUP }));
    let message = sent.as_ref().map_err(|e| e.to_string()).unwrap_err();
    assert!(
        matches!(&sent, Err(ToolProxyError::Refused { tool, .. }) if tool == SEND),
        "{sent:?}"
    );
    assert!(
        message.contains("refused by the organisation policy"),
        "{message}"
    );
    assert_eq!(fx.ran(), vec![LOOKUP.to_string()]);
    assert!(matches!(
        fx.recorded().as_slice(),
        [GovernanceEvent::PolicyRefused { agent_id, access_id, .. }]
            if agent_id == AGENT && access_id == SEND
    ));
    let refused = records.iter().find(|r| r.tool_name == SEND).unwrap();
    assert!(!refused.success);
    fx.shutdown().await;
}

// GIVEN a workflow of an agent holding no consent for sending outside
// WHEN it calls the sensitive tool
// THEN the call does not run and asks for an approval of that tool, as a
// batch item does
#[tokio::test]
async fn test_a_sensitive_tool_without_consent_asks_for_an_approval() {
    let fx = fixture(GovernanceState::default(), &[]).await;

    let sent = fx.proxy.call_inner(SEND, serde_json::json!({})).await;

    match &sent {
        Err(ToolProxyError::ApprovalRequired { payload, .. }) => {
            assert_eq!(payload["genre"], "approbation");
            assert_eq!(payload["geste"], SEND);
        }
        other => panic!("expected an approval, got {other:?}"),
    }
    assert!(fx.ran().is_empty());
    fx.shutdown().await;
}

// GIVEN a workflow of an agent with a standing consent for sending outside,
// and another whose run was granted the sensitive tool, as a batch grants
// the accesses of its agent
// WHEN each calls the sensitive tool
// THEN it runs in both
#[tokio::test]
async fn test_a_sensitive_tool_runs_on_the_consent_of_its_agent_or_a_grant_of_the_run() {
    let consented = GovernanceState {
        consents: HashSet::from([(AGENT.to_string(), ConsentFamily::SendExternal)]),
        ..Default::default()
    };
    let by_consent = fixture(consented, &[]).await;
    let by_grant = fixture(GovernanceState::default(), &[SEND]).await;

    let first = by_consent
        .proxy
        .call_inner(SEND, serde_json::json!({}))
        .await;
    let second = by_grant.proxy.call_inner(SEND, serde_json::json!({})).await;

    assert_eq!(first.unwrap(), serde_json::json!({ "ran": SEND }));
    assert_eq!(second.unwrap(), serde_json::json!({ "ran": SEND }));
    by_consent.shutdown().await;
    by_grant.shutdown().await;
}

// GIVEN a workflow of an agent with a standing consent for sending outside,
// whose connection `mcp:crm` is in pause
// WHEN it calls a tool of that connection, the read and the sensitive one
// THEN neither runs, each refusal names the connection in pause, and each is
// journaled as refused by the pause
#[tokio::test]
async fn test_every_tool_of_a_paused_connection_is_refused_to_the_workflow() {
    let state = GovernanceState {
        consents: HashSet::from([(AGENT.to_string(), ConsentFamily::SendExternal)]),
        paused: HashSet::from(["mcp:crm".to_string()]),
        ..Default::default()
    };
    let fx = fixture(state, &[SEND]).await;

    let read = fx.proxy.call_inner(LOOKUP, serde_json::json!({})).await;
    let sent = fx.proxy.call_inner(SEND, serde_json::json!({})).await;

    for result in [&read, &sent] {
        match result {
            Err(error @ ToolProxyError::Refused { .. }) => {
                assert!(error.to_string().contains("mcp:crm is paused"), "{error}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }
    assert!(fx.ran().is_empty());
    let recorded = fx.recorded();
    assert_eq!(recorded.len(), 2);
    assert!(recorded.iter().all(|e| matches!(
        e,
        GovernanceEvent::PausedRefused { connection_id, .. } if connection_id == "mcp:crm"
    )));
    fx.shutdown().await;
}

// GIVEN a workflow whose run was granted the sensitive tool, and a policy
// that forbade the connection `mcp:crm` at boot
// WHEN it calls a tool of that connection
// THEN the call is refused by the policy, the grant of the run
// notwithstanding
#[tokio::test]
async fn test_a_tool_of_a_forbidden_connection_is_refused_to_the_workflow() {
    let state = GovernanceState {
        policy: PolicyRefusals {
            forbidden_connections: HashSet::from(["mcp:crm".to_string()]),
            ..Default::default()
        },
        ..Default::default()
    };
    let fx = fixture(state, &[SEND]).await;

    let sent = fx.proxy.call_inner(SEND, serde_json::json!({})).await;

    let message = sent.as_ref().map_err(|e| e.to_string()).unwrap_err();
    assert!(
        message.contains("this connection is not allowed"),
        "{message}"
    );
    assert!(fx.ran().is_empty());
    fx.shutdown().await;
}

// GIVEN a workflow of an agent and a fine rule refusing the read tool to
// every agent
// WHEN the workflow calls that tool
// THEN the call does not run, its refusal names the rule, and it is
// journaled as refused by that rule
#[tokio::test]
async fn test_a_tool_a_fine_rule_refuses_is_refused_to_the_workflow_and_journaled() {
    let state = GovernanceState {
        rules: vec![PrefixRule {
            id: 7,
            tool_name: LOOKUP.to_string(),
            action: RuleAction::Deny,
            ..PrefixRule::default()
        }],
        ..Default::default()
    };
    let fx = fixture(state, &[]).await;

    let read = fx.proxy.call_inner(LOOKUP, serde_json::json!({})).await;

    let message = read.as_ref().map_err(|e| e.to_string()).unwrap_err();
    assert!(
        message.contains("refused by permission rule 7"),
        "{message}"
    );
    assert!(fx.ran().is_empty());
    assert!(
        matches!(
            fx.recorded().as_slice(),
            [GovernanceEvent::RuleRefused { agent_id, access_id, rule_id: 7, .. }]
                if agent_id == AGENT && access_id == LOOKUP
        ),
        "{:?}",
        fx.recorded()
    );
    fx.shutdown().await;
}

/// Awaits `proxy.call(tool, input)` from Python, under `asyncio.run` as a
/// workflow does, and returns the message of the `RuntimeError` it raised,
/// `None` when it raised nothing.
fn call_from_python(proxy: ToolProxy, tool: &str, input: &str) -> Option<String> {
    pyo3::prepare_freethreaded_python();
    Python::with_gil(|py| {
        let scope = pyo3::types::PyDict::new(py);
        scope
            .set_item("proxy", Py::new(py, proxy).unwrap())
            .unwrap();
        scope.set_item("tool", tool).unwrap();
        scope.set_item("raw", input).unwrap();
        py.run(
            c"import asyncio, json
async def main():
    try:
        await proxy.call(tool, json.loads(raw))
    except RuntimeError as error:
        return str(error)
    return None
outcome = asyncio.run(main())",
            Some(&scope),
            None,
        )
        .unwrap();
        scope
            .get_item("outcome")
            .unwrap()
            .and_then(|o| o.extract::<Option<String>>().ok())
            .flatten()
    })
}

// GIVEN a workflow of an agent whose policy refuses the sensitive tool, the
// proxy wired to the event bus of the runtime
// WHEN the code of the workflow awaits that tool from Python, with a
// recipient as its argument
// THEN the tool does not run, the code catches a RuntimeError that names the
// tool and the reason of the policy and leaves the argument out, and the bus
// carries the refusal as a `ToolCallDenied` with the same reason
#[tokio::test(flavor = "multi_thread")]
async fn test_a_refusal_reaches_python_as_a_readable_error_and_the_bus_as_a_denial() {
    let state = GovernanceState {
        policy: PolicyRefusals {
            denied: HashSet::from([SEND.to_string()]),
            ..Default::default()
        },
        ..Default::default()
    };
    let Fixture {
        proxy,
        registry,
        audit,
        ran,
        governance,
    } = fixture(state, &[]).await;
    let (bus, mut events) = tokio::sync::broadcast::channel(8);
    let proxy = proxy.with_event_bus(bus);

    let raised = tokio::task::spawn_blocking(move || {
        call_from_python(proxy, SEND, r#"{"to": "ada@example.org"}"#)
    })
    .await;
    let journaled = governance.recorded.lock().map(|r| r.len()).unwrap_or(0);
    let ran = ran.lock().map(|r| r.clone()).unwrap_or_default();
    registry.shutdown().await;
    audit.shutdown().await;

    let message = raised
        .unwrap()
        .expect("the refused call raises a RuntimeError");
    assert!(message.contains(SEND), "{message}");
    assert!(
        message.contains("refused by the organisation policy"),
        "{message}"
    );
    assert!(!message.contains("ada@example.org"), "{message}");
    assert!(ran.is_empty());
    let mut denial = None;
    while let Ok(event) = events.try_recv() {
        if let RuntimeEvent::ToolCallDenied {
            tool_name,
            reason,
            detail,
            ..
        } = event
        {
            denial = Some((tool_name, reason, detail));
        }
    }
    let (tool_name, reason, detail) = denial.expect("a ToolCallDenied on the bus");
    assert_eq!(tool_name, SEND);
    assert_eq!(reason, "permission_denied");
    let detail = detail.unwrap_or_default();
    assert!(
        detail.contains("refused by the organisation policy"),
        "{detail}"
    );
    assert!(!detail.contains("ada@example.org"), "{detail}");
    assert_eq!(journaled, 1);
}
