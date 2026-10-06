//! The questions running workflows wait on.
//!
//! A step asks the user a question (`ctx.user.ask`) and its run waits there:
//! the question is kept by the workflow actor with the channel its answer
//! goes back through, listed with the other decisions waiting for the user,
//! and answered or declined from that list. The answer wakes the very await
//! the step stopped on, so the run goes on inside the step it was in.
//!
//! A question lives in memory, like the run that asked it. A runtime that
//! stops leaves the run `running` in `hitl.db`; the next start fails it with
//! its reason, as it does every interrupted run, and the question is gone
//! with it.

use std::collections::HashMap;

use tokio::sync::oneshot;

use super::WorkflowError;

/// Longest question a workflow may ask, in characters.
pub const QUESTION_MAX_CHARS: usize = 2000;

/// Most choices a question may propose.
pub const OPTIONS_MAX: usize = 20;

/// What a workflow asks the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionAsked {
    /// The question, as shown.
    pub question: String,
    /// The proposed choices; empty for a free answer. A free answer is taken
    /// besides them in every case.
    pub options: Vec<String>,
    /// A hint for the answer field.
    pub hint: Option<String>,
}

impl QuestionAsked {
    /// Refuses a question nobody could answer: empty, too long, or with an
    /// empty, repeated or surplus choice.
    ///
    /// # Errors
    ///
    /// The reason, as the workflow receives it.
    pub fn check(&self) -> Result<(), String> {
        if self.question.trim().is_empty() {
            return Err("a question to the user cannot be empty".into());
        }
        if self.question.chars().count() > QUESTION_MAX_CHARS {
            return Err(format!(
                "a question to the user holds at most {QUESTION_MAX_CHARS} characters"
            ));
        }
        if self.options.len() > OPTIONS_MAX {
            return Err(format!(
                "a question to the user proposes at most {OPTIONS_MAX} choices"
            ));
        }
        for (index, option) in self.options.iter().enumerate() {
            if option.trim().is_empty() {
                return Err(format!("choice {index} of the question is empty"));
            }
            if self.options[..index].contains(option) {
                return Err(format!("choice '{option}' is proposed twice"));
            }
        }
        Ok(())
    }
}

/// What the user did with a question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserReply {
    /// The answer: a proposed choice, or free text.
    Answer(String),
    /// The user declined to answer, with the reason given.
    Declined(String),
}

/// A question a run waits on, as listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowQuestion {
    /// Identifier to answer it by.
    pub request_id: String,
    /// The run that waits.
    pub run_id: String,
    /// Its workflow, `<package>/<workflow>`.
    pub workflow_id: String,
    /// The step the run is in, when it asked from inside one.
    pub step: Option<String>,
    /// What is asked.
    pub asked: QuestionAsked,
    /// When, RFC 3339.
    pub asked_at: String,
}

/// The questions waiting, each with the channel of its answer. Owned by the
/// workflow actor.
#[derive(Default)]
pub(super) struct WaitingQuestions {
    waiting: HashMap<String, (WorkflowQuestion, oneshot::Sender<UserReply>)>,
}

impl WaitingQuestions {
    pub(super) fn register(
        &mut self,
        question: WorkflowQuestion,
        reply: oneshot::Sender<UserReply>,
    ) {
        self.waiting
            .insert(question.request_id.clone(), (question, reply));
    }

    /// Every question waiting, oldest first.
    pub(super) fn list(&self) -> Vec<WorkflowQuestion> {
        let mut listed: Vec<WorkflowQuestion> =
            self.waiting.values().map(|(q, _)| q.clone()).collect();
        listed.sort_by(|a, b| {
            a.asked_at
                .cmp(&b.asked_at)
                .then_with(|| a.request_id.cmp(&b.request_id))
        });
        listed
    }

    /// Hands `reply` to the run waiting on `request_id` and returns the
    /// question it answered.
    pub(super) fn answer(
        &mut self,
        request_id: &str,
        reply: UserReply,
    ) -> Result<WorkflowQuestion, WorkflowError> {
        let (question, tx) = self
            .waiting
            .remove(request_id)
            .ok_or_else(|| WorkflowError::UnknownQuestion(request_id.to_string()))?;
        // A run stopped between the listing and the answer no longer listens.
        tx.send(reply)
            .map_err(|_| WorkflowError::UnknownQuestion(request_id.to_string()))?;
        Ok(question)
    }

    /// Forgets the question `request_id`: its run stopped waiting.
    pub(super) fn withdraw(&mut self, request_id: &str) {
        self.waiting.remove(request_id);
    }

    /// Forgets every question of the run `run_id`, which ended.
    pub(super) fn forget_run(&mut self, run_id: &str) {
        self.waiting.retain(|_, (q, _)| q.run_id != run_id);
    }
}
