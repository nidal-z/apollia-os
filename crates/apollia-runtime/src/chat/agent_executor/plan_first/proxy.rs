//! How a step of a plan calls a tool: as the loop calls one.
//!
//! A tool the agent is not granted is refused. A granted tool the run has not
//! pre-authorized waits for the human, through the same event, registry and
//! answer as a call of the loop; an approval holds for the same call when the
//! step is retried. The output reaches the plan as the loop would give it to
//! the model: whole under the cap, otherwise kept on disk.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use apollia_core::RuntimeEvent;
use apollia_llm::ToolInvoker;
use apollia_oria::actor::ToolProxyTrait;

use super::{granted_set, PlanFirstDeps};
use crate::chat::agent_executor::AgentRunSpec;
use crate::chat::builtin_agent::context_tool_output;
use crate::chat::types::{PendingChatApprovals, ToolCallRecord, ToolCallStatus, ToolDecision};
use crate::eventbus::EventBusSender;

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
    read_only: bool,
    schema: serde_json::Value,
}

/// The tools of one plan-first run.
pub(in crate::chat::agent_executor) struct GrantedToolProxy {
    invoker: std::sync::Arc<dyn ToolInvoker>,
    event_bus: EventBusSender,
    session_id: String,
    message_id: String,
    granted: HashSet<String>,
    described: HashMap<String, Described>,
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
                        read_only: descriptor.is_read_only,
                        schema: descriptor.input_schema,
                    },
                );
            }
        }
        Self {
            invoker: std::sync::Arc::clone(&wiring.deps.tool_invoker),
            event_bus: wiring.deps.event_bus.clone(),
            session_id: wiring.session_id.to_string(),
            message_id: wiring.message_id.to_string(),
            granted,
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

    /// Wait for the human on a call the run has not pre-authorized. `Ok` when
    /// the call may run.
    async fn approve(&self, tool_name: &str, input: &serde_json::Value) -> Result<(), String> {
        if locked(&self.authorized).contains(tool_name) {
            return Ok(());
        }
        let call_key = format!("{tool_name}\u{1f}{input}");
        if locked(&self.approved_calls).contains(&call_key) {
            return Ok(());
        }
        let call_id = format!(
            "plan-call-{}",
            self.next_call.fetch_add(1, Ordering::SeqCst)
        );
        let _ = self.event_bus.send(RuntimeEvent::ChatApprovalRequired {
            session_id: self.session_id.clone(),
            message_id: self.message_id.clone(),
            tool_call_id: call_id.clone(),
            tool_name: tool_name.to_string(),
            prompt: format!(
                "Tool '{tool_name}' asks to run with: {}",
                preview(&input.to_string())
            ),
        });
        let rx = self.pending_approvals.register(format!(
            "{}::{}::{call_id}",
            self.session_id, self.message_id
        ));
        match rx.await.unwrap_or(ToolDecision::refuse()) {
            ToolDecision::Accept => {
                locked(&self.approved_calls).insert(call_key);
                Ok(())
            }
            ToolDecision::AlwaysAccept { .. } => {
                if apollia_permissions::is_code_executor(tool_name) {
                    locked(&self.approved_calls).insert(call_key);
                } else {
                    locked(&self.authorized).insert(tool_name.to_string());
                }
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
}

#[async_trait::async_trait]
impl ToolProxyTrait for GrantedToolProxy {
    async fn invoke(&self, tool_name: &str, input: &serde_json::Value) -> Result<String, String> {
        if !self.granted.contains(tool_name) {
            let refusal = format!("unauthorized: tool '{tool_name}' is not granted to this agent");
            self.record(tool_name, input, &refusal, ToolCallStatus::Refused);
            return Err(refusal);
        }
        if let Err(refusal) = self.approve(tool_name, input).await {
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
