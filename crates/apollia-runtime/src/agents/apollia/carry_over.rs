//! The settings the chat kept before it was an agent, carried over to the
//! Apollia agent once, at startup.
//!
//! The chat kept one row in `governance.db`: a system prompt, the tools
//! allowed without confirmation, and a preferred backend. Each becomes what an
//! agent holds:
//!
//! - the system prompt, with the built-in prompt of Apollia's tier after it,
//!   becomes the mission override, so the instructions the chat followed are
//!   kept along with the user's; no prompt, no override;
//! - the tools, added to the accesses Apollia's definition grants, become the
//!   access override; no tool, no override. What they let run without a
//!   question was already turned into standing consents by the access
//!   upgrade, and stays so;
//! - the backend becomes the model assigned to Apollia in `system.db`, unless
//!   one is assigned already.
//!
//! Every override is written through [`OverrideWriter`], so each is journaled.
//! The legacy table is dropped last; a start interrupted before that carries
//! the same values over again, which changes nothing.

use std::path::Path;

use apollia_core::model_preset::{AgentModel, ModelSource};
use apollia_core::paths::DataFile;
use apollia_core::{LlmBackendRepository, LlmProvider};
use apollia_permissions::{LegacyChatConfig, LegacyChatConfigState, PermissionError};
use apollia_tools::{AgentRepository, AgentRepositoryError, OverrideField};

use super::APOLLIA_AGENT_ID;
use crate::agents::overrides::{AgentOverrideError, OverrideWriter};
use crate::governance::access_upgrade::ACCESS_UPGRADE_ACTOR;
use crate::governance::GovernanceContract;

/// Errors of the carry-over; nothing is dropped when one occurs.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CarryOverError {
    /// `governance.db` could not be read or the legacy table dropped.
    #[error("chat settings unreadable: {0}")]
    Governance(#[from] PermissionError),
    /// `agents.db` could not be opened.
    #[error("agents.db unavailable: {0}")]
    Agents(#[from] AgentRepositoryError),
    /// An override could not be written or journaled.
    #[error("override not carried over: {0}")]
    Override(#[from] AgentOverrideError),
}

/// What a start did with the legacy settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CarryOver {
    /// They were carried over at an earlier start.
    AlreadyDone,
    /// The access upgrade has not run; they wait for it.
    Waiting,
    /// Carried over now: the fields that became overrides, and whether the
    /// backend became Apollia's model.
    Done {
        /// Fields written as overrides.
        overrides: Vec<OverrideField>,
        /// Whether a model was assigned.
        model_assigned: bool,
    },
}

/// Carries the legacy settings of the chat over to the Apollia agent, the
/// overrides journaled through `governance`, then drops the legacy table.
///
/// # Errors
///
/// [`CarryOverError`] when a file cannot be read or written; the legacy table
/// is then kept, and the next start tries again.
pub fn carry_over_chat_config(
    data_dir: &Path,
    governance: &dyn GovernanceContract,
) -> Result<CarryOver, CarryOverError> {
    let governance_db = DataFile::Governance.path(data_dir);
    let legacy = match apollia_permissions::read_legacy_chat_config(&governance_db)? {
        LegacyChatConfigState::Gone => return Ok(CarryOver::AlreadyDone),
        LegacyChatConfigState::Waiting => return Ok(CarryOver::Waiting),
        LegacyChatConfigState::Pending(legacy) => legacy,
    };
    let repo = AgentRepository::open(&DataFile::Agents.path(data_dir))?;
    let writer = OverrideWriter {
        repo: &repo,
        governance,
        actor: ACCESS_UPGRADE_ACTOR,
    };
    let overrides = write_overrides(&writer, &legacy)?;
    let model_assigned = legacy
        .llm_backend
        .as_deref()
        .is_some_and(|backend| assign_model(&DataFile::System.path(data_dir), backend));
    apollia_permissions::drop_legacy_chat_config(&governance_db)?;
    Ok(CarryOver::Done {
        overrides,
        model_assigned,
    })
}

/// Writes the mission and access overrides the legacy row calls for.
fn write_overrides(
    writer: &OverrideWriter<'_>,
    legacy: &LegacyChatConfig,
) -> Result<Vec<OverrideField>, CarryOverError> {
    let apollia = writer
        .repo
        .resolved(
            APOLLIA_AGENT_ID,
            None,
            crate::agents::global_default_autonomy(),
        )?
        .ok_or_else(|| AgentRepositoryError::NotFound(APOLLIA_AGENT_ID.to_owned()))?;
    let mut written = Vec::new();
    let prompt = legacy.system_prompt.trim();
    if !prompt.is_empty() {
        let built_in = crate::chat::builtin_agent::built_in_prompt(apollia.autonomy);
        let mission = format!("{prompt}\n\n{built_in}");
        writer.set(
            APOLLIA_AGENT_ID,
            OverrideField::Mission,
            &serde_json::Value::String(mission),
        )?;
        written.push(OverrideField::Mission);
    }
    if legacy.allowed_tools.iter().any(|t| !t.trim().is_empty()) {
        let mut access = apollia.access.clone();
        for tool in &legacy.allowed_tools {
            let tool = tool.trim();
            if !tool.is_empty() && !access.iter().any(|a| a == tool) {
                access.push(tool.to_owned());
            }
        }
        writer.set(
            APOLLIA_AGENT_ID,
            OverrideField::Access,
            &serde_json::json!(access),
        )?;
        written.push(OverrideField::Access);
    }
    Ok(written)
}

/// Assigns `backend` to Apollia in the `system.db` at `path`, unless a model
/// is assigned already. A backend the store refuses (a server nothing
/// declares any more) is left out, and the chat runs on the default model.
fn assign_model(path: &Path, backend: &str) -> bool {
    let assigned = LlmBackendRepository::open(path).and_then(|repo| {
        if repo.agent_model(APOLLIA_AGENT_ID)?.is_some() {
            return Ok(false);
        }
        let model_source = match repo.find_by_name(backend)? {
            Some(row) if row.provider == LlmProvider::LlamaCpp => ModelSource::Local,
            Some(_) => ModelSource::Server,
            None if backend.contains('/') => ModelSource::Server,
            None => ModelSource::Local,
        };
        repo.set_agent_model(&AgentModel {
            agent: APOLLIA_AGENT_ID.to_owned(),
            model_source,
            model_id: backend.to_owned(),
            preset_id: None,
        })?;
        Ok(true)
    });
    match assigned {
        Ok(done) => done,
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                backend = %backend,
                error = %error,
                detail = "the chat runs on the default model",
                "agents.apollia.model_not_carried"
            );
            false
        }
    }
}

/// Startup step: carries the settings over, journaled, or defers it while the
/// journal is unavailable, so no override is written without its record.
pub(crate) fn run_at_startup(
    data_dir: &Path,
    journal: Option<&crate::audit_journal::AuditJournalHandle>,
) {
    let Some(journal) = journal else {
        tracing::event!(
            tracing::Level::WARN,
            detail = "audit journal unavailable, carry-over deferred to the next start",
            "agents.apollia.carry_over.deferred"
        );
        return;
    };
    let governance = crate::governance::NoPolicyGovernance::new(journal.clone());
    match carry_over_chat_config(data_dir, &governance) {
        Ok(outcome) => tracing::event!(
            tracing::Level::INFO,
            outcome = ?outcome,
            "agents.apollia.carry_over.done"
        ),
        Err(e) => tracing::event!(
            tracing::Level::WARN,
            error = %e,
            "agents.apollia.carry_over.failed"
        ),
    }
}

#[cfg(test)]
mod tests;
