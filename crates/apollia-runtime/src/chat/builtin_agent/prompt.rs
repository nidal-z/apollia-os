use super::*;

impl BuiltInChatAgent {
    /// Build the effective system prompt for the given autonomy level.
    ///
    /// When `custom_prompt` is a non-empty string it is used as the base
    /// (preserving the behavior for agents with a personalized prompt).
    /// Otherwise the base is selected by tier: the assisted tier uses
    /// [`DEFAULT_SYSTEM_PROMPT`], every autonomous tier uses
    /// [`PERSEVERANCE_SYSTEM_PROMPT`].
    ///
    /// Then prepends the authoritative temporal/environment block at the **top**
    /// of the prompt so the LLM treats current date + time + timezone as ground
    /// truth, not as one fact among its priors. The user persona block is
    /// appended only when `inject_memory` is true and a memory repository is
    /// configured (memory stays at the agent's initiative, gated by the tier).
    pub fn build_system_prompt(
        &self,
        custom_prompt: Option<&str>,
        level: AutonomyLevel,
        inject_memory: bool,
    ) -> String {
        // Base: a non-empty custom prompt (already enriched upstream with the
        // project / cross-session context) wins; otherwise the tier picks a
        // built-in profile.
        let base_prompt = match custom_prompt {
            Some(custom) if !custom.is_empty() => custom,
            _ => match level {
                AutonomyLevel::Assisted => DEFAULT_SYSTEM_PROMPT,
                AutonomyLevel::Supervised
                | AutonomyLevel::BoundedAutonomous
                | AutonomyLevel::LongAutonomous => PERSEVERANCE_SYSTEM_PROMPT,
            },
        };

        // Dynamic blocks owned by the caller; apollia-prompts only composes.
        let temporal = apollia_core::temporal_context::temporal_context_block();

        let working_dir_note = self
            .workspace_path
            .as_deref()
            .map(|ws| apollia_prompts::blocks::working_directory_note(&ws.display().to_string()));

        // User-persona brief, memory at the agent's initiative, gated by tier.
        let persona = if inject_memory {
            self.user_memory
                .as_ref()
                .and_then(|repo_mutex| match repo_mutex.lock() {
                    Ok(repo) => match repo.recall_persona_brief(30) {
                        Ok(brief) if !brief.is_empty() => {
                            Some(apollia_prompts::blocks::persona_block(&brief))
                        }
                        Ok(_) => None,
                        Err(e) => {
                            warn!(error = %e, "Failed to read user memory for injection, skipping");
                            None
                        }
                    },
                    Err(e) => {
                        warn!(error = %e, "User memory mutex poisoned, skipping injection");
                        None
                    }
                })
        } else {
            None
        };

        // Plan block is phase-aware: plan-first while preparing, execution-first
        // once approved, so the two never contradict each other.
        let plan = if self.plan_mode_active() {
            Some(if self.session_plan_phase == PlanPhase::Executing {
                crate::chat::plan_prompt::plan_execute_block()
            } else {
                crate::chat::plan_prompt::plan_mode_block()
            })
        } else {
            None
        };

        apollia_prompts::ChatPromptBuilder::new(base_prompt)
            .temporal(&temporal)
            .working_dir_note(working_dir_note.as_deref())
            .persona(persona.as_deref())
            .plan(plan)
            .build()
    }
}
