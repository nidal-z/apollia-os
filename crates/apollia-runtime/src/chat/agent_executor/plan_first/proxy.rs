//! How a step of a plan calls a tool: as the loop calls one.
//!
//! A tool the agent is not granted is refused. A granted tool is decided by
//! governance at the moment of the call, as a call of the loop is: it runs,
//! it is refused, or it waits for the human through the same event, registry
//! and answers; a step holds no approval of its own. An answer holds for the
//! same call when the step is retried. The output reaches the plan as the
//! loop would give it to the model: whole under the cap, otherwise kept on
//! disk.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use apollia_core::RuntimeEvent;
use apollia_llm::types::ToolCall;
use apollia_llm::ToolInvoker;
use apollia_oria::actor::ToolProxyTrait;
use apollia_tools::ActionFamily;

use super::{granted_set, PlanFirstDeps};
use crate::chat::agent_executor::AgentRunSpec;
use crate::chat::builtin_agent::{approval_request, context_tool_output, refusal_text};
use crate::chat::types::{PendingChatApprovals, ToolCallRecord, ToolCallStatus, ToolDecision};
use crate::eventbus::EventBusSender;
use crate::governance::{
    answered_once, once_grant, AccessCall, AccessDecision, ConsentAnswer, DecisionReason,
    GovernanceContract, GovernanceEvent, GovernanceHandle,
};

/// Characters of an input or output shown in an event.
const PREVIEW_CHARS: usize = 200;

/// What the proxy of one run is built from.
pub(in crate::chat::agent_executor) struct ProxyWiring<'a> {
    pub spec: &'a AgentRunSpec,
    pub deps: &'a PlanFirstDeps,
    pub session_id: &'a str,
    pub message_id: &'a str,
    pub authorized: &'a HashSet<String>,
    pub pending_approvals: &'a PendingChatApprovals,
}

/// What the registry says of a granted tool.
struct Described {
    family: ActionFamily,
    read_only: bool,
    schema: serde_json::Value,
}

/// The tools of one plan-first run.
pub(in crate::chat::agent_executor) struct GrantedToolProxy {
    invoker: std::sync::Arc<dyn ToolInvoker>,
    event_bus: EventBusSender,
    governance: GovernanceHandle,
    agent_id: String,
    unattended: bool,
    session_id: String,
    message_id: String,
    granted: HashSet<String>,
    described: HashMap<String, Described>,
    /// The entries of the deferred MCP tool index for the granted tools the
    /// registry does not describe.
    mcp_index: Vec<apollia_mcp::tool_search::ToolIndexSnapshot>,
    authorized: Mutex<HashSet<String>>,
    approved_calls: Mutex<HashSet<String>>,
    pending_approvals: PendingChatApprovals,
    tool_output_dir: Option<PathBuf>,
    records: Mutex<Vec<ToolCallRecord>>,
    next_call: AtomicU64,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn preview(text: &str) -> String {
    text.chars().take(PREVIEW_CHARS).collect()
}

impl GrantedToolProxy {
    /// The proxy of one run, the registry read once for its granted tools.
    pub(in crate::chat::agent_executor) async fn new(wiring: ProxyWiring<'_>) -> Self {
        let granted = granted_set(wiring.spec);
        let mut described = HashMap::new();
        for name in &granted {
            if let Some(descriptor) = wiring.deps.tool_registry.describe(name).await {
                described.insert(
                    name.clone(),
                    Described {
                        family: descriptor.family,
                        read_only: descriptor.is_read_only,
                        schema: descriptor.input_schema,
                    },
                );
            }
        }
        let mcp_index = wiring
            .deps
            .mcp_index
            .iter()
            .filter(|entry| {
                let name = entry.full_name();
                granted.contains(&name) && !described.contains_key(&name)
            })
            .cloned()
            .collect();
        Self {
            invoker: std::sync::Arc::clone(&wiring.deps.tool_invoker),
            event_bus: wiring.deps.event_bus.clone(),
            governance: wiring.deps.governance.clone(),
            agent_id: wiring.spec.agent_id.clone(),
            unattended: wiring.deps.unattended,
            session_id: wiring.session_id.to_string(),
            message_id: wiring.message_id.to_string(),
            granted,
            mcp_index,
            described,
            authorized: Mutex::new(wiring.authorized.clone()),
            approved_calls: Mutex::new(HashSet::new()),
            pending_approvals: wiring.pending_approvals.clone(),
            tool_output_dir: wiring.deps.tool_output_dir.clone(),
            records: Mutex::new(Vec::new()),
            next_call: AtomicU64::new(0),
        }
    }

    /// The calls made so far, in order.
    pub(in crate::chat::agent_executor) fn records(&self) -> Vec<ToolCallRecord> {
        locked(&self.records).clone()
    }

    fn record(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
        output: &str,
        status: ToolCallStatus,
    ) {
        locked(&self.records).push(ToolCallRecord {
            tool_name: tool_name.to_string(),
            input: input.clone(),
            output: Some(output.to_string()),
            status,
            rationale: None,
            retry_attempts: Vec::new(),
        });
    }

    /// The family of `tool_name` as its descriptor declares it, else as its
    /// entry in the deferred MCP tool index carries it; a tool neither
    /// describes is unclassified.
    fn family_of(&self, tool_name: &str) -> ActionFamily {
        match self.described.get(tool_name) {
            Some(described) => described.family,
            None => apollia_mcp::tool_search::tool_family(tool_name, None, &self.mcp_index),
        }
    }

    /// The governance decision on one call of a step. A grant of the run
    /// counts as a consent for the run; an answer already given to this very
    /// call, or a grant of one call, lets a call that would ask go through.
    fn decide(&self, tool_name: &str, input: &serde_json::Value, call_key: &str) -> AccessDecision {
        let argument = apollia_permissions::extract_first_arg(input);
        let decision = self.governance.decide(&AccessCall {
            agent_id: &self.agent_id,
            access_id: tool_name,
            argument: argument.as_deref(),
            family: self.family_of(tool_name),
            unattended: self.unattended,
            granted_for_run: locked(&self.authorized).contains(tool_name),
        });
        let answered = locked(&self.approved_calls).contains(call_key)
            || locked(&self.authorized).contains(&once_grant(tool_name));
        if answered {
            answered_once(decision)
        } else {
            decision
        }
    }

    /// Decide one call of a step. `Ok` when the call may run: governance
    /// allowed it, or the human answered yes. A refusal, of governance or of
    /// the human, is the error the step fails with.
    async fn step_call_consent(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Result<(), String> {
        let call_key = format!("{tool_name}\u{1f}{input}");
        let decision = self.decide(tool_name, input, &call_key);
        let family = self.family_of(tool_name);
        match &decision {
            AccessDecision::Denied { reason } => {
                tracing::info!(tool = %tool_name, reason = ?reason, "plan.tool.governance_denied");
                self.journal_refusal(tool_name, family, reason);
                return Err(format!("unauthorized: {}", refusal_text(reason)));
            }
            AccessDecision::Allowed { reason } => {
                tracing::info!(tool = %tool_name, reason = ?reason, "plan.tool.governance_allowed");
                // A grant of one call is spent by the call it let through.
                locked(&self.authorized).remove(&once_grant(tool_name));
                return Ok(());
            }
            AccessDecision::AskUser { .. } | AccessDecision::Pending { .. } => {
                tracing::info!(
                    tool = %tool_name,
                    pending = matches!(decision, AccessDecision::Pending { .. }),
                    "plan.tool.governance_asks"
                );
            }
        }
        let always_offered = decision.answers().contains(&ConsentAnswer::Always);
        let call_id = format!(
            "plan-call-{}",
            self.next_call.fetch_add(1, Ordering::SeqCst)
        );
        let request = approval_request(
            &ToolCall {
                id: call_id.clone(),
                name: tool_name.to_string(),
                arguments: input.clone(),
            },
            decision.answers(),
        );
        let _ = self.event_bus.send(RuntimeEvent::ChatApprovalRequired {
            session_id: self.session_id.clone(),
            message_id: self.message_id.clone(),
            tool_call_id: call_id.clone(),
            tool_name: tool_name.to_string(),
            prompt: request.prompt,
            answers: request.answers,
            detail: request.detail,
            code: request.code,
            working_dir: request.working_dir,
        });
        let rx = self.pending_approvals.register(format!(
            "{}::{}::{call_id}",
            self.session_id, self.message_id
        ));
        let answer = rx.await.unwrap_or(ToolDecision::refuse());
        let taken = match &answer {
            ToolDecision::Accept => ConsentAnswer::Once,
            ToolDecision::AlwaysAccept if always_offered => ConsentAnswer::Always,
            ToolDecision::AlwaysAccept => ConsentAnswer::Once,
            ToolDecision::Refuse { .. } => ConsentAnswer::Refuse,
        };
        self.journal(&GovernanceEvent::ConsentAnswered {
            agent_id: self.agent_id.clone(),
            access_id: tool_name.to_string(),
            family,
            answer: taken,
        });
        match answer {
            ToolDecision::Accept => {
                locked(&self.approved_calls).insert(call_key);
                Ok(())
            }
            ToolDecision::AlwaysAccept => {
                if always_offered {
                    self.keep_always(tool_name, input, family).await;
                }
                locked(&self.approved_calls).insert(call_key);
                Ok(())
            }
            ToolDecision::Refuse { reason } => Err(match reason {
                Some(reason) => {
                    format!("unauthorized: the operator refused '{tool_name}'. Reason: {reason}")
                }
                None => format!("unauthorized: the operator refused '{tool_name}'"),
            }),
        }
    }

    /// Writes what an "always" governance offered stands for: a fine allow
    /// rule of this agent on the command for a tool that runs code, a
    /// standing consent of this agent for the family otherwise. Never a grant
    /// by tool name. A write that fails leaves the answer worth this call.
    async fn keep_always(&self, tool_name: &str, input: &serde_json::Value, family: ActionFamily) {
        let written = if family == ActionFamily::ExecuteCode
            || apollia_permissions::is_code_executor(tool_name)
        {
            match apollia_permissions::extract_first_arg(input) {
                Some(command) => self
                    .governance
                    .allow_command(&self.agent_id, tool_name, &command)
                    .await
                    .map(|_| ()),
                None => {
                    tracing::warn!(tool = %tool_name, "plan.tool.always_accept.downgraded");
                    return;
                }
            }
        } else {
            self.governance
                .grant_always(&self.agent_id, family)
                .await
                .map(|_| ())
        };
        match written {
            Ok(()) => tracing::info!(tool = %tool_name, "plan.tool.always_accept.granted"),
            Err(error) => tracing::warn!(
                tool = %tool_name,
                error = %error,
                "plan.tool.always_accept.not_recorded"
            ),
        }
    }

    /// Records in the journal a call the policy or a rule refused. Neither
    /// the call's argument nor its result is recorded.
    fn journal_refusal(&self, tool_name: &str, family: ActionFamily, reason: &DecisionReason) {
        let event = match reason {
            DecisionReason::Policy { detail } => GovernanceEvent::PolicyRefused {
                agent_id: self.agent_id.clone(),
                access_id: tool_name.to_string(),
                family,
                detail: detail.clone(),
            },
            DecisionReason::Rule { rule_id } => GovernanceEvent::RuleRefused {
                agent_id: self.agent_id.clone(),
                access_id: tool_name.to_string(),
                family,
                rule_id: *rule_id,
            },
            _ => return,
        };
        self.journal(&event);
    }

    fn journal(&self, event: &GovernanceEvent) {
        if !self.governance.journaled() {
            return;
        }
        if let Err(error) = self.governance.record(event) {
            tracing::warn!(error = %error, "plan.tool.governance_unjournaled");
        }
    }
}

#[async_trait::async_trait]
impl ToolProxyTrait for GrantedToolProxy {
    async fn invoke(&self, tool_name: &str, input: &serde_json::Value) -> Result<String, String> {
        if !self.granted.contains(tool_name) {
            let refusal = format!("unauthorized: tool '{tool_name}' is not granted to this agent");
            self.record(tool_name, input, &refusal, ToolCallStatus::Refused);
            return Err(refusal);
        }
        if let Err(refusal) = self.step_call_consent(tool_name, input).await {
            self.record(tool_name, input, &refusal, ToolCallStatus::Refused);
            return Err(refusal);
        }
        let _ = self.event_bus.send(RuntimeEvent::ChatToolCallStarted {
            session_id: self.session_id.clone(),
            message_id: self.message_id.clone(),
            tool_name: tool_name.to_string(),
            input_preview: preview(&input.to_string()),
            rationale: None,
        });
        let result = self.invoker.invoke(tool_name, input).await;
        let (output, success) = match &result {
            Ok(output) => (output.clone(), true),
            Err(e) => (format!("tool error: {e}"), false),
        };
        let _ = self.event_bus.send(RuntimeEvent::ChatToolCallCompleted {
            session_id: self.session_id.clone(),
            message_id: self.message_id.clone(),
            tool_name: tool_name.to_string(),
            success,
            output_preview: Some(preview(&output)),
            analysis: None,
        });
        self.record(
            tool_name,
            input,
            &output,
            ToolCallStatus::from_success(success),
        );
        result.map(|output| context_tool_output(&output, self.tool_output_dir.as_deref()))
    }

    fn is_tool_read_only(&self, tool_name: &str) -> bool {
        self.described.get(tool_name).is_some_and(|d| d.read_only)
    }

    async fn tool_schema(&self, tool_name: &str) -> Option<serde_json::Value> {
        self.described.get(tool_name).map(|d| d.schema.clone())
    }
}
