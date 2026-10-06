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
    /// The base is the mission of the agent when it has one, the built-in
    /// prompt of its tier otherwise: the assisted tier uses
    /// [`DEFAULT_SYSTEM_PROMPT`], every autonomous tier uses
    /// [`PERSEVERANCE_SYSTEM_PROMPT`]. What a conversation asks for itself is
    /// never the base: its instructions are added after it, under their own
    /// heading, and the mission stays whole.
    ///
    /// Then prepends the authoritative temporal/environment block at the **top**
    /// of the prompt so the LLM treats current date + time + timezone as ground
    /// truth, not as one fact among its priors. The profile block of the
    /// conversation is appended whenever the executor handed a profile store:
    /// at every tier, for the Apollia agent alone, while the profile switch of
    /// the conversation is on.
    pub fn build_system_prompt(&self, mission: Option<&str>, level: AutonomyLevel) -> String {
        let base_prompt = match mission {
            Some(mission) if !mission.is_empty() => mission,
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

        // The instructions of the conversation were checked when they were
        // stored; a text the builder still refuses is left out, and said.
        let builder = apollia_prompts::ChatPromptBuilder::new(base_prompt);
        let builder = match builder
            .clone()
            .session_instructions(self.session.instructions.as_deref())
        {
            Ok(with_instructions) => with_instructions,
            Err(error) => {
                tracing::event!(
                    tracing::Level::WARN,
                    error = %error,
                    detail = "the turn runs without the instructions of the conversation",
                    "chat.session.instructions.refused"
                );
                builder
            }
        };
        builder
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
