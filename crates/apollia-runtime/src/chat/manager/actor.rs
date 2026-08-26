use super::*;

impl ChatSessionManager {
    /// Process incoming commands until Shutdown or channel close.
    pub(in crate::chat::manager) async fn run(mut self, mut rx: mpsc::Receiver<ChatCommand>) {
        while let Some(cmd) = rx.recv().await {
            match cmd {
                ChatCommand::CreateSession {
                    mode,
                    agent_name,
                    system_prompt,
                    tools,
                    project_id,
                    reply,
                } => {
                    let result = self
                        .handle_create_session(CreateSessionParams {
                            mode,
                            agent_name,
                            system_prompt,
                            tools,
                            project_id,
                        })
                        .await;
                    let _ = reply.send(result);
                }
                ChatCommand::SendMessage {
                    session_id,
                    content,
                    reply,
                } => {
                    let result = self.handle_send_message(&session_id, &content);
                    let _ = reply.send(result);
                }
                ChatCommand::ResolveTool {
                    session_id,
                    message_id,
                    tool_call_id,
                    tool_name,
                    decision,
                    reply,
                } => {
                    let result = self.handle_resolve_tool(
                        &session_id,
                        &message_id,
                        &tool_call_id,
                        &tool_name,
                        decision,
                    );
                    let _ = reply.send(result);
                }
                ChatCommand::ListSessions {
                    status_filter,
                    reply,
                } => {
                    let result = self.handle_list_sessions(status_filter.as_ref());
                    let _ = reply.send(result);
                }
                ChatCommand::GetSession { session_id, reply } => {
                    let result = self.handle_get_session(&session_id);
                    let _ = reply.send(result);
                }
                ChatCommand::ResolveSessionWorkspace { session_id, reply } => {
                    let result = self.handle_resolve_session_workspace(&session_id);
                    let _ = reply.send(result);
                }
                ChatCommand::GetSessionTodo { session_id, reply } => {
                    // Resolve synchronously so no SQLite borrow is held across
                    // the await, then read through the owned handle clone.
                    let result = match self.resolve_todo_handle(&session_id) {
                        Ok(Some(todo)) => todo.get_items(&session_id).await.map_err(|e| {
                            ChatError::InternalError(format!("todo read failed: {e}"))
                        }),
                        Ok(None) => Ok(Vec::new()),
                        Err(e) => Err(e),
                    };
                    let _ = reply.send(result);
                }
                ChatCommand::CloseSession { session_id, reply } => {
                    let result = self.handle_close_session(&session_id);
                    let _ = reply.send(result);
                }
                ChatCommand::DeleteSession { session_id, reply } => {
                    let result = self.handle_delete_session(&session_id);
                    let _ = reply.send(result);
                }
                ChatCommand::RenameSession {
                    session_id,
                    title,
                    reply,
                } => {
                    let result = self.handle_rename_session(&session_id, &title);
                    let _ = reply.send(result);
                }
                ChatCommand::SetPlanMode {
                    session_id,
                    enabled,
                    reply,
                } => {
                    let result = self.handle_set_plan_mode(&session_id, enabled);
                    let _ = reply.send(result);
                }
                ChatCommand::ApprovePlan { session_id, reply } => {
                    let result = self.handle_approve_plan(&session_id).await;
                    let _ = reply.send(result);
                }
                ChatCommand::RejectPlan {
                    session_id,
                    reason,
                    reply,
                } => {
                    let result = self.handle_reject_plan(&session_id, reason).await;
                    let _ = reply.send(result);
                }
                ChatCommand::ReadPlanMutations { session_id, reply } => {
                    // Resolve the handle synchronously so no SQLite borrow is held
                    // across the await, then read through the owned handle clone.
                    let result = match self.resolve_plan_handle(&session_id) {
                        Ok(Some(plan)) => plan.read_mutations(&session_id).await.map_err(|e| {
                            ChatError::InternalError(format!("plan history read failed: {e}"))
                        }),
                        Ok(None) => Ok(Vec::new()),
                        Err(e) => Err(e),
                    };
                    let _ = reply.send(result);
                }
                ChatCommand::GetPlan { session_id, reply } => {
                    // Phase is the authoritative gate state, resolved synchronously
                    // (in-memory session first, then persisted row) so no SQLite
                    // borrow is held across the await below.
                    let phase = self
                        .sessions
                        .get(&session_id)
                        .map(|s| s.plan_phase.as_sql().to_string())
                        .or_else(|| {
                            self.repository
                                .get_session(&session_id)
                                .ok()
                                .flatten()
                                .map(|r| r.plan_phase)
                        })
                        .unwrap_or_else(|| PlanPhase::Done.as_sql().to_string());
                    let result = match self.resolve_plan_handle(&session_id) {
                        Ok(Some(plan)) => plan
                            .get_plan(&session_id)
                            .await
                            .map(|plan| ChatPlanSnapshot {
                                plan,
                                phase: phase.clone(),
                            })
                            .map_err(|e| {
                                ChatError::InternalError(format!("plan snapshot read failed: {e}"))
                            }),
                        Ok(None) => Ok(ChatPlanSnapshot { plan: None, phase }),
                        Err(e) => Err(e),
                    };
                    let _ = reply.send(result);
                }
                ChatCommand::PauseSession { session_id, reply } => {
                    let result = self.handle_pause_session(&session_id);
                    let _ = reply.send(result);
                }
                ChatCommand::ResumePausedSession { session_id, reply } => {
                    let result = self.handle_resume_paused_session(&session_id);
                    let _ = reply.send(result);
                }
                ChatCommand::InjectInstruction {
                    session_id,
                    text,
                    reply,
                } => {
                    let result = self.handle_inject_instruction(&session_id, &text);
                    let _ = reply.send(result);
                }
                ChatCommand::GetPauseState { session_id, reply } => {
                    let _ = reply.send(self.pause_state(&session_id));
                }
                ChatCommand::UpdateSession {
                    session_id,
                    system_prompt,
                    available_tools,
                    llm_backend,
                    reply,
                } => {
                    let result = self.handle_update_session(
                        &session_id,
                        system_prompt.as_deref(),
                        available_tools.as_deref(),
                        llm_backend.as_ref(),
                    );
                    let _ = reply.send(result);
                }
                ChatCommand::RegenerateResponse {
                    session_id,
                    message_id,
                    reply,
                } => {
                    let result = self.handle_regenerate_response(&session_id, &message_id);
                    let _ = reply.send(result);
                }
                ChatCommand::EditAndResend {
                    session_id,
                    message_id,
                    content,
                    reply,
                } => {
                    let result = self.handle_edit_and_resend(&session_id, &message_id, &content);
                    let _ = reply.send(result);
                }
                ChatCommand::ExchangeComplete {
                    session_id,
                    message_id,
                    response,
                } => {
                    self.handle_exchange_complete(&session_id, &message_id, response);
                }
                ChatCommand::ExchangeError {
                    session_id,
                    message_id,
                    error,
                } => {
                    self.handle_exchange_error(&session_id, &message_id, &error);
                }
                ChatCommand::PersistSummary {
                    session_id,
                    summary,
                } => {
                    if let Err(e) = self.repository.update_summary(&session_id, &summary) {
                        warn!(session_id = %session_id, error = %e, "chat.summary.persist.failed");
                    }
                }
                ChatCommand::GetRecentSummaries { limit, reply } => {
                    let result = self.handle_get_recent_summaries(limit);
                    let _ = reply.send(result);
                }
                ChatCommand::ResumeSession { session_id, reply } => {
                    let result = self.handle_resume_session(&session_id);
                    let _ = reply.send(result);
                }
                ChatCommand::ForkSession {
                    session_id,
                    up_to_index,
                    reply,
                } => {
                    let result = self.handle_fork_session(&session_id, up_to_index);
                    let _ = reply.send(result);
                }
                ChatCommand::ListChildren { session_id, reply } => {
                    let result = self.handle_list_children(&session_id);
                    let _ = reply.send(result);
                }
                ChatCommand::LinkSessionToProject {
                    session_id,
                    project_id,
                    reply,
                } => {
                    let result =
                        self.handle_link_session_to_project(&session_id, project_id.as_deref());
                    let _ = reply.send(result);
                }
                ChatCommand::ListSessionsByProject { project_id, reply } => {
                    let result = self.handle_list_sessions_by_project(&project_id);
                    let _ = reply.send(result);
                }
                ChatCommand::OrphanProjectSessions { project_id } => {
                    self.handle_orphan_project_sessions(&project_id);
                }
                ChatCommand::ListSessionAuthorizations { reply } => {
                    let _ = reply.send(self.handle_list_session_authorizations());
                }
                ChatCommand::RevokeSessionAuthorization {
                    session_id,
                    tool_name,
                    reply,
                } => {
                    let _ = reply
                        .send(self.handle_revoke_session_authorization(&session_id, &tool_name));
                }
                ChatCommand::ListA2ASkills { reply } => {
                    self.handle_list_a2a_skills(reply);
                }
                ChatCommand::ListA2ASkillTelemetry { reply } => {
                    self.handle_list_a2a_skill_telemetry(reply);
                }
                ChatCommand::ListA2AStepProvenance { skill_id, reply } => {
                    self.handle_list_a2a_step_provenance(skill_id, reply);
                }
                ChatCommand::CheckA2ACompatibility {
                    skill_id,
                    required_version,
                    reply,
                } => {
                    self.handle_check_a2a_compatibility(skill_id, required_version, reply);
                }
                ChatCommand::ReloadLlm { router } => {
                    info!("chat.router.reloaded");
                    self.llm_router = router;
                }
                ChatCommand::ResolveFsHitl {
                    request_id,
                    decision,
                    reply,
                } => {
                    let _ = reply.send(self.handle_resolve_fs_hitl(&request_id, decision));
                }
                ChatCommand::ListApprovalHistory { limit, days, reply } => {
                    let result = self.repository.list_tool_approval_history(limit, days);
                    let _ = reply.send(result);
                }
                ChatCommand::RegisterUserInputReply {
                    request_id,
                    session_id,
                    questions_json,
                    context,
                    reply_tx,
                } => {
                    self.handle_register_user_input_reply(RegisterUserInputReplyParams {
                        request_id,
                        session_id,
                        questions_json,
                        context,
                        reply_tx,
                    });
                }
                ChatCommand::ResolveUserInput {
                    request_id,
                    answers,
                    reply,
                } => {
                    let result = self.resolve_user_input_internal(&request_id, answers);
                    let _ = reply.send(result);
                }
                ChatCommand::RejectUserInput {
                    request_id,
                    reason,
                    reply,
                } => {
                    let result = self.reject_user_input_internal(&request_id, reason);
                    let _ = reply.send(result);
                }
                ChatCommand::ListPendingUserInputs { reply } => {
                    let views = self
                        .pending_user_replies
                        .iter()
                        .map(|(req_id, (meta, _))| PendingUserInputView {
                            request_id: req_id.clone(),
                            session_id: meta.session_id.clone(),
                            questions_json: meta.questions_json.clone(),
                            context: meta.context.clone(),
                            created_at: meta.created_at.clone(),
                        })
                        .collect();
                    let _ = reply.send(views);
                }
                ChatCommand::GetSessionMetrics { session_id, reply } => {
                    let result = self.metrics.get(&session_id).cloned();
                    let _ = reply.send(result);
                }
                ChatCommand::Shutdown => {
                    info!("chat.manager.stopping");
                    break;
                }
            }
        }
        info!("chat.manager.stopped");
    }
}
