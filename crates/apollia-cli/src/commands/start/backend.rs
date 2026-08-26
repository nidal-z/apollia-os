//! The production backend's execution entry point.

use std::pin::Pin;
use std::sync::Arc;

use apollia_core::{AIPResult, AIPTask};
use apollia_oria::engine::ORIAEngine;
use apollia_runtime::coordinator::ExecutionBackend;

use super::engine::{direct_path_budget, wire_engine_with_llm, AIPProductionBackend};
use super::llm_glue::OriaToolProxy;
use super::runner::BridgeRunner;

impl ExecutionBackend for AIPProductionBackend {
    fn execute(
        &self,
        task: AIPTask,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<AIPResult, String>> + Send>> {
        // Agent-declared budget capped to the runtime ceiling. The same
        // Arc<StepBudget> is shared into the runner (so the Direct-path ctx
        // counts the agent's tool/LLM calls) and into execute_direct (so the
        // engine supervises the same counters). Principle #7 holds on the Direct
        // path, not only wall-clock.
        let agent_step_budget = self.manifest.step_budget.clone().unwrap_or_default();
        let direct_budget = Arc::new(direct_path_budget(&agent_step_budget));

        let runner = BridgeRunner {
            bridge: Arc::clone(&self.bridge),
            llm_router: self.llm_router.clone(),
            event_bus: self.event_bus.clone(),
            agent_id: self.agent_id.clone(),
            manifest: self.manifest.clone(),
            allowed_tools: self.allowed_tools.clone(),
            tool_registry: self.tool_registry.clone(),
            audit_trail: self.audit_trail.clone(),
            memory_namespace: self.memory_namespace.clone(),
            memory_base_dir: self.memory_base_dir.clone(),
            a2a_invoker: self.a2a_invoker.clone(),
            tools_config: self.tools_config.clone(),
            user_memory_write: self.user_memory_write,
            datasources_declared: self.datasources_declared.clone(),
            templates_declared: self.templates_declared.clone(),
            agent_dir: self.agent_dir.clone(),
            secrets_declared: self.secrets_declared.clone(),
            secrets_data_dir: self.secrets_data_dir.clone(),
            mcp_handle: self.mcp_handle.clone(),
            budget: Arc::clone(&direct_budget),
        };

        // Build a per-task ORIAEngine wired with HITL components.
        let mut engine = ORIAEngine::new().with_event_bus(self.event_bus.clone());
        if let Some(pending) = self.pending_approvals.clone() {
            engine = engine.with_pending_approvals(pending);
        }
        if let Some(repo) = self.task_repository.clone() {
            engine = engine.with_task_repository(repo);
        }

        // Plan-mode: forward the per-run gate override. Wire the shared gate
        // registry only when the gate is explicitly requested (`--plan`), so
        // headless submissions (A2A, triggers) never pause for a decision.
        engine = engine.with_plan_gate_override(task.run_options.plan_gate);
        if task.run_options.plan_gate == Some(true) {
            if let Some(gates) = self.plan_gates.clone() {
                engine = engine.with_pending_plan_gates(gates);
            }
        }
        if let Some(cache) = self.plan_cache.clone() {
            engine = engine.with_shared_plan_cache(cache);
        }
        // CLI `--autonomy` override feeds the engine tier (drives the gate
        // policy when no explicit `--plan` override is set).
        if let Some(tier) = task.run_options.autonomy_level {
            engine = engine.with_oria_config(apollia_core::ORIAConfig {
                autonomy_level: Some(tier),
                ..apollia_core::ORIAConfig::default()
            });
        }

        let execution_mode = self.manifest.execution_mode.clone();
        let step_budget_max = self
            .manifest
            .step_budget
            .as_ref()
            .map(|b| b.max_steps)
            .unwrap_or(20);

        // Wire the Reasoner + LlmRouter so ORIA's orchestrated path can plan.
        // Extracted into `wire_engine_with_llm` for unit testing, see the
        // regression guard in the test module at the bottom of this file.
        engine = wire_engine_with_llm(
            engine,
            self.llm_router.clone(),
            &self.agent_id,
            step_budget_max,
        );

        Box::pin(async move {
            // Orchestrated agents (declared via `@orchestrated`) flow through
            // ORIA's planning + ActorLoop. Everything else uses the direct
            // path which invokes `__apollia_dispatch__` (skills, @on_message).
            if execution_mode == "orchestrated" {
                // Wire the governed ToolProxy so orchestrated plan steps execute
                // real tools (under permission + audit + resilience + budget)
                // instead of the engine's NoopToolProxy. Without it, an
                // orchestrated agent could only run LLM steps.
                let engine = match runner.build_tool_proxy(&task).await {
                    Some(proxy) => engine.with_tool_proxy(Arc::new(OriaToolProxy { proxy })),
                    None => engine,
                };
                Ok(engine.execute(task, &runner).await)
            } else {
                // Bound the direct path by the same budget shared into the
                // runner's ctx, so the engine supervisor and the agent's
                // tool/LLM chokepoints enforce one set of counters.
                engine
                    .execute_direct(task, &runner, direct_budget)
                    .await
                    .map_err(|e| e.to_string())
            }
        })
    }
}
