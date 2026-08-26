//! The orchestrated path: run a plan, verify it, finalize.
//!
//! Split out of `engine.rs`: the engine's state stays in the parent, the
//! plan-driven execution and its verification loop live here.

use std::sync::Arc;
use std::time::Instant;

use apollia_core::{
    AIPResult, AIPTask, AgentManifest, AutonomyLevel, AutonomyLevelConfig, RuntimeEvent, TaskStatus,
};

use crate::actor::{ActorLoop, StepDeps, ToolProxyTrait};
use crate::budget::StepBudget;
use crate::engine::{
    concat_outputs, extract_step_outputs, extract_task_text, result_text, AIPAgent, NoopToolProxy,
    ORIAEngine, ORIAError,
};
use crate::observer::{ContextBundle, ExecutionMode};
use crate::plan::ExecutionPlan;
use crate::plan_cache::compute_cache_key;
use crate::plan_gate::PlanGateDecision;
use crate::verification::{
    run_post_run_verification, verdict_feedback, CriticPass, VerificationLoop,
};

impl ORIAEngine {
    pub(super) async fn execute_orchestrated_plan(
        &self,
        task: AIPTask,
        agent: &(dyn AIPAgent + Send + Sync),
        manifest: AgentManifest,
    ) -> AIPResult {
        // validate system_prompt
        if manifest.system_prompt.is_none() {
            return AIPResult::failed(
                "MISSING_SYSTEM_PROMPT",
                "execution_mode=orchestrated requires system_prompt in the agent manifest",
            );
        }

        // Collect workspace context and enrich system prompt
        let workspace_block = self.build_system_prompt().await;
        let enriched_system_prompt = if workspace_block.is_empty() {
            manifest.system_prompt.clone()
        } else {
            manifest
                .system_prompt
                .as_ref()
                .map(|sp| format!("{}\n\n{}", sp, workspace_block))
        };

        // Build ContextBundle
        let available_tools: Vec<String> = manifest
            .tools_required
            .iter()
            .chain(manifest.tools_optional.iter())
            .cloned()
            .collect();

        let ctx = ContextBundle {
            task: task.clone(),
            memory_snapshot: None,
            execution_mode: ExecutionMode::Orchestrated,
            available_tools,
            manifest_system_prompt: enriched_system_prompt,
            llm_backend_names: vec![],
        };

        // get reasoner or fail
        let reasoner = match self.reasoner.as_ref() {
            Some(r) => r,
            None => {
                return AIPResult::failed(
                    "NO_LLM",
                    "Orchestrated mode requires a configured LLM (use with_reasoner())",
                )
            }
        };

        // Plan cache lookup
        let task_text = extract_task_text(&task);
        let cache_key = compute_cache_key(
            &manifest.name,
            &manifest.version,
            &ctx.available_tools,
            &task_text,
        );

        if let Some(plan) = self.lookup_cached_plan(&cache_key, &task.task_id) {
            let _ = self.event_bus.send(RuntimeEvent::PlanCacheHit {
                task_id: task.task_id.clone().into(),
                cache_key: cache_key.clone(),
            });

            return self
                .execute_cached_plan(plan, task, agent, manifest, &ctx, &cache_key)
                .await;
        }

        // Generate plan (Reasoner handles retries internally)
        let mut plan = match reasoner.plan(&ctx).await {
            Ok(p) => p,
            Err(e) => return AIPResult::failed("PLAN_FAILED", &e.to_string()),
        };

        // Resolve each tool step's structured arguments so the persisted plan is
        // fully specified before it is cached, audited and executed. Best-effort:
        // unresolved steps are handled just in time.
        self.enrich_plan_with_args(&mut plan).await;

        self.store_plan_in_cache(&cache_key, &plan, &manifest);

        let task_id_str = task.task_id.clone();
        let db_path = self.db_path.as_deref().unwrap_or(":memory:");

        // emit PlanGenerated for the initial plan
        let _ = self.event_bus.send(RuntimeEvent::PlanGenerated {
            task_id: task_id_str.clone().into(),
            agent_name: manifest.name.clone(),
            plan_id: plan.plan_id.clone(),
            step_count: plan.steps.len(),
            // The orchestrated engine path correlates via task_id, not a chat run.
            run_id: None,
        });

        // Plan gate: when active, pause and await an operator decision before any
        // budget is created or step executed. The wait holds no budget (principle
        // #7). On rejection the engine replans with the feedback and re-opens the
        // gate, bounded by plan_gate_max_replans; a timeout or closed channel ends
        // the run cleanly.
        if self.plan_gate_active() {
            let max_replans = self.oria_config.plan_gate_max_replans;
            let mut replans_count: u32 = 0;
            loop {
                let plan_id = plan.plan_id.clone();
                match self.await_plan_gate(&task_id_str, &plan).await {
                    Ok(PlanGateDecision::Approved) => {
                        let _ = self.event_bus.send(RuntimeEvent::PlanApproved {
                            run_id: task_id_str.clone(),
                            plan_id,
                            task_id: task_id_str.clone(),
                        });
                        break;
                    }
                    Ok(PlanGateDecision::Rejected { feedback }) => {
                        // Enforce the ceiling before any further LLM call (principle #7).
                        if replans_count >= max_replans {
                            tracing::warn!(
                                run_id = %task_id_str,
                                replans_count,
                                "plan.gate.replan_limit"
                            );
                            let _ = self.event_bus.send(RuntimeEvent::PlanAbandoned {
                                run_id: task_id_str.clone(),
                                task_id: task_id_str.clone(),
                                reason: "replan_limit".into(),
                            });
                            return AIPResult::failed(
                                "PLAN_REPLAN_LIMIT_EXCEEDED",
                                "plan rejected more times than plan_gate_max_replans allows",
                            );
                        }
                        let _ = self.event_bus.send(RuntimeEvent::PlanRejected {
                            run_id: task_id_str.clone(),
                            plan_id: plan_id.clone(),
                            task_id: task_id_str.clone(),
                            feedback: feedback.clone(),
                            replans_so_far: replans_count,
                        });
                        replans_count += 1;
                        match reasoner
                            .plan_with_feedback(&ctx, &plan_id, feedback.as_deref())
                            .await
                        {
                            Ok(new_plan) => {
                                plan = new_plan;
                                self.store_plan_in_cache(&cache_key, &plan, &manifest);
                                let _ = self.event_bus.send(RuntimeEvent::PlanGenerated {
                                    task_id: task_id_str.clone().into(),
                                    agent_name: manifest.name.clone(),
                                    plan_id: plan.plan_id.clone(),
                                    step_count: plan.steps.len(),
                                    run_id: None,
                                });
                                // Loop re-opens the gate for the new plan.
                            }
                            Err(e) => {
                                tracing::error!(run_id = %task_id_str, error = %e, "plan.gate.replan_failed");
                                let _ = self.event_bus.send(RuntimeEvent::PlanAbandoned {
                                    run_id: task_id_str.clone(),
                                    task_id: task_id_str.clone(),
                                    reason: "replan_failed".into(),
                                });
                                return AIPResult::failed("REPLAN_FAILED", &e.to_string());
                            }
                        }
                    }
                    Ok(PlanGateDecision::Edited { revised_steps }) => {
                        // Execute the operator's revised plan directly. Re-validate
                        // the edited steps (unique ids, resolvable deps, no cycle)
                        // before committing: a malformed edit ends the run cleanly.
                        let steps = revised_steps;

                        if let Err(e) = crate::reasoner::validate_steps(&steps) {
                            tracing::warn!(
                                run_id = %task_id_str,
                                error = %e,
                                "plan.gate.edit_invalid"
                            );
                            let _ = self.event_bus.send(RuntimeEvent::PlanAbandoned {
                                run_id: task_id_str.clone(),
                                task_id: task_id_str.clone(),
                                reason: "edit_invalid".into(),
                            });
                            return AIPResult::failed("PLAN_EDIT_INVALID", &e.to_string());
                        }

                        plan = ExecutionPlan {
                            plan_id: plan_id.clone(),
                            task_id: task_id_str.clone(),
                            steps,
                        };
                        self.store_plan_in_cache(&cache_key, &plan, &manifest);
                        let _ = self.event_bus.send(RuntimeEvent::PlanApproved {
                            run_id: task_id_str.clone(),
                            plan_id: plan.plan_id.clone(),
                            task_id: task_id_str.clone(),
                        });
                        break;
                    }
                    Err(ORIAError::PlanGateTimeout {
                        run_id, ttl_secs, ..
                    }) => {
                        tracing::warn!(run_id = %run_id, ttl_secs, "plan.gate.timeout");
                        return AIPResult::failed(
                            "PLAN_GATE_TIMEOUT",
                            "plan gate timed out before a decision was received",
                        );
                    }
                    Err(ORIAError::PlanGateChannelClosed { run_id }) => {
                        tracing::warn!(run_id = %run_id, "plan.gate.channel_closed");
                        return AIPResult::failed(
                            "PLAN_GATE_CHANNEL_CLOSED",
                            "plan gate channel closed before a decision",
                        );
                    }
                    Err(e) => return AIPResult::failed("PLAN_GATE_ERROR", &e.to_string()),
                }
            }
        }

        // Execute the plan, verify the result, and replan on a failing verdict.
        self.run_plan_with_verification(plan, &task, agent, &manifest, &ctx, &cache_key, db_path)
            .await
    }
    /// Execute a plan via the `ActorLoop`, then run the post-run verification and,
    /// on a failing verdict, replan and re-execute up to
    /// `oria_config.verification_max_replans` times.
    ///
    /// The `StepBudget` is created once and shared across every replan iteration,
    /// so it remains the non-bypassable ceiling for the whole run (principle #7).
    /// The critic call is off-budget by construction (it routes directly); the
    /// replan re-execution is on-budget (the `ActorLoop` increments), and the loop
    /// stops once the budget is exhausted.
    ///
    /// Verification is gated by the autonomy tier, mirroring the chat path: the
    /// tier's `run_verification` flag decides whether the pass runs at all. When it
    /// does not, the completed result is returned unverified after a `PlanCompleted`.
    ///
    /// The final verdict is emitted as [`RuntimeEvent::VerificationCompleted`] so it
    /// lands in the signed audit journal.
    // REASON: threads the engine's borrowed state through the verification run; a struct would borrow the same fields.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn run_plan_with_verification(
        &self,
        mut plan: ExecutionPlan,
        task: &AIPTask,
        agent: &(dyn AIPAgent + Send + Sync),
        manifest: &AgentManifest,
        ctx: &ContextBundle,
        cache_key: &str,
        db_path: &str,
    ) -> AIPResult {
        let reasoner = match self.reasoner.as_ref() {
            Some(r) => r,
            None => {
                return AIPResult::failed(
                    "NO_LLM",
                    "Orchestrated mode requires a configured LLM (use with_reasoner())",
                )
            }
        };
        let task_id_str = task.task_id.clone();

        // StepBudget created once, shared across every verification replan.
        let agent_budget = manifest.step_budget.clone().unwrap_or_default();
        let budget = StepBudget::from_capped(&agent_budget, &self.runtime_config);

        // Resolve the verification gate from the autonomy tier (chat parity: the
        // chat reads `AutonomyConfig::default().level_config(level).run_verification`).
        let tier = self
            .oria_config
            .autonomy_level
            .unwrap_or(AutonomyLevel::Assisted);
        let run_verification = AutonomyLevelConfig::default_for(tier).run_verification;
        let verifier = if run_verification {
            Some((
                VerificationLoop::new(manifest.check_commands.clone(), Vec::new()),
                CriticPass::new(Arc::new(self.llm_router.clone())),
            ))
        } else {
            None
        };

        let objective = extract_task_text(task);
        let max_replans = self.oria_config.verification_max_replans;
        let mut replans: u32 = 0;

        loop {
            let repo = match self.open_repo_with_plan(db_path, &plan, &manifest.name) {
                Ok(repo) => repo,
                Err(e) => {
                    return AIPResult::failed(
                        "PLAN_REPOSITORY",
                        &format!("plan repository unavailable: {e}"),
                    )
                }
            };
            let plan_id = plan.plan_id.clone();
            let step_count = plan.steps.len();

            // Reset per-task token budget before each execution.
            self.llm_router.reset_session_budget();

            let noop_proxy = NoopToolProxy;
            let tool_proxy: &dyn ToolProxyTrait = match &self.tool_proxy {
                Some(p) => p.as_ref(),
                None => &noop_proxy,
            };

            let plan_start = Instant::now();
            let mut actor = ActorLoop::new(
                plan,
                self.oria_config.max_replans,
                repo,
                self.event_bus.clone(),
                manifest.clone(),
            )
            .with_pending_approvals(self.pending_approvals.clone())
            .with_memory_manager(self.memory_manager.clone())
            .with_step_memory_max_chars(self.oria_config.step_memory_max_chars)
            .with_context_manager(self.context_manager.clone());
            let step_result = actor
                .execute(
                    StepDeps {
                        tool_proxy,
                        llm_router: &self.llm_router,
                        budget: &budget,
                        reasoner,
                    },
                    &self.resilience,
                )
                .await;
            let duration_ms = plan_start.elapsed().as_millis() as u64;

            // Emit final session budget snapshot for this execution.
            let token_budget = self.llm_router.session_budget();
            let _ = self.event_bus.send(RuntimeEvent::TokenBudgetUpdated {
                session_cost_usd: token_budget.cost_usd,
                total_input_tokens: token_budget.input_tokens,
                total_output_tokens: token_budget.output_tokens,
                total_cache_read_tokens: token_budget.cache_read_tokens,
                threshold_usd: f64::MAX,
                threshold_exceeded: false,
            });

            // A non-completed run (budget exceeded, plan failure) carries its own
            // failure and is not verified.
            if step_result.status != TaskStatus::Completed {
                return step_result;
            }

            let final_result = self.finalize_completed(agent, step_result).await;

            // Verification disabled for this tier: return the completed result.
            let Some((verification, critic)) = verifier.as_ref() else {
                let _ = self.event_bus.send(RuntimeEvent::PlanCompleted {
                    task_id: task_id_str.clone().into(),
                    plan_id,
                    step_count,
                    duration_ms,
                });
                return final_result;
            };

            // Run the post-run verification. The critic is off-budget by design.
            let output_text = result_text(&final_result);
            let verdict =
                run_post_run_verification(verification, critic, &objective, &output_text).await;
            let _ = self.event_bus.send(RuntimeEvent::VerificationCompleted {
                task_id: task_id_str.clone().into(),
                passed: verdict.passed,
                check_failures: verdict.check_failures.len() as u32,
                corrections: verdict.corrections.len() as u32,
                skipped: verdict.skipped,
                replans,
            });

            // Accept the result when the verdict passes, the replan ceiling is
            // reached, or the shared budget is spent.
            if verdict.passed || replans >= max_replans || budget.is_exhausted() {
                let _ = self.event_bus.send(RuntimeEvent::PlanCompleted {
                    task_id: task_id_str.clone().into(),
                    plan_id,
                    step_count,
                    duration_ms,
                });
                return final_result;
            }

            // Failing verdict with replan budget remaining: replan with feedback.
            let feedback = verdict_feedback(&verdict);
            match reasoner
                .plan_with_feedback(ctx, &plan_id, Some(&feedback))
                .await
            {
                Ok(mut new_plan) => {
                    self.enrich_plan_with_args(&mut new_plan).await;
                    self.store_plan_in_cache(cache_key, &new_plan, manifest);
                    let _ = self.event_bus.send(RuntimeEvent::PlanGenerated {
                        task_id: task_id_str.clone().into(),
                        agent_name: manifest.name.clone(),
                        plan_id: new_plan.plan_id.clone(),
                        step_count: new_plan.steps.len(),
                        run_id: None,
                    });
                    plan = new_plan;
                    replans += 1;
                }
                Err(e) => {
                    tracing::event!(
                        tracing::Level::WARN,
                        task_id = %task_id_str,
                        error = %e,
                        "oria.verification.replan_failed"
                    );
                    // The run has a valid result; only the hardening step failed.
                    let _ = self.event_bus.send(RuntimeEvent::PlanCompleted {
                        task_id: task_id_str.clone().into(),
                        plan_id,
                        step_count,
                        duration_ms,
                    });
                    return final_result;
                }
            }
        }
    }
    /// Assemble a completed orchestrated run into its user-facing result.
    ///
    /// Calls the agent's `on_plan_complete()` hook when present, otherwise
    /// concatenates the per-step outputs.
    pub(super) async fn finalize_completed(
        &self,
        agent: &(dyn AIPAgent + Send + Sync),
        step_result: AIPResult,
    ) -> AIPResult {
        let outputs = extract_step_outputs(&step_result);
        if agent.has_on_plan_complete() {
            agent.call_on_plan_complete(outputs).await
        } else {
            concat_outputs(&outputs)
        }
    }
}
