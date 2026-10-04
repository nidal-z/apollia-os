use super::*;

/// The built-in prompt of a tier, used when the agent has no mission: the
/// assisted tier uses [`DEFAULT_SYSTEM_PROMPT`], every autonomous tier
/// [`PERSEVERANCE_SYSTEM_PROMPT`].
pub fn built_in_prompt(level: AutonomyLevel) -> &'static str {
    match level {
        AutonomyLevel::Assisted => DEFAULT_SYSTEM_PROMPT,
        AutonomyLevel::Supervised
        | AutonomyLevel::BoundedAutonomous
        | AutonomyLevel::LongAutonomous => PERSEVERANCE_SYSTEM_PROMPT,
    }
}

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
    /// truth, not as one fact among its priors. The profile block of the
    /// conversation is appended whenever the executor handed a profile store:
    /// at every tier, for the Apollia agent alone, while the profile switch of
    /// the conversation is on.
    pub fn build_system_prompt(&self, custom_prompt: Option<&str>, level: AutonomyLevel) -> String {
        // Base: a non-empty custom prompt (already enriched upstream with the
        // project / cross-session context) wins; otherwise the tier picks a
        // built-in profile.
        let base_prompt = match custom_prompt {
            Some(custom) if !custom.is_empty() => custom,
            _ => built_in_prompt(level),
        };

        // Dynamic blocks owned by the caller; apollia-prompts only composes.
        // The host-environment block rides in the temporal slot: both are
        // authoritative ground truth about "here and now" and belong at the top.
        let temporal = format!(
            "{}\n{}",
            apollia_core::temporal_context::temporal_context_block(),
            host_environment_block()
        );

        // The working directory is the first folder granted to the agent, the
        // anchor of relative paths; the file tools reach the granted folders
        // alone. The note says so in both cases, including the one where no
        // folder is granted.
        let workspace_display = self
            .workspace_path
            .as_deref()
            .map(|ws| ws.display().to_string());
        let working_dir_note = Some(apollia_prompts::blocks::working_directory_note(
            workspace_display.as_deref(),
        ));

        // The profile of the conversation. The store is present only for the
        // Apollia agent of a conversation whose profile switch is on.
        let persona = crate::chat::profile_injection::profile_block(
            self.user_memory.as_deref(),
            self.conversation_profile.as_deref(),
        );

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

/// Render the host-environment block from freshly gathered host facts.
///
/// Gathering lives in `apollia-tools` (shell discovery, OS version probes),
/// rendering in `apollia-prompts` (zero-I/O crate); this helper is the glue.
pub(crate) fn host_environment_block() -> String {
    let env = apollia_tools::host_env::gather_host_environment(None);
    let shell = env.posix_shell.as_ref().map(|p| p.display().to_string());
    // No confinement to declare: the chat invoker runs with `/` as its root and
    // the HITL layer, not a path check, is what stops an operation.
    apollia_prompts::blocks::environment_block(
        env.os,
        env.os_version.as_deref(),
        env.arch,
        shell.as_deref(),
        None,
    )
}
