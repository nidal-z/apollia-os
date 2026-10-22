//! Writing the overrides of an agent, each write recorded by governance.
//!
//! Every write or removal of an override records an
//! [`GovernanceEvent::OverrideChanged`]; a change of the access list also
//! records one [`GovernanceEvent::AccessChanged`] per tool granted or taken
//! away, so the journal says who gave what to which agent.
//!
//! The access compared is the one the store holds for the agent: its list,
//! or, for an agent on the default access, the tools given on top of it. The
//! default itself is read from the machine at each run and no override write
//! changes it, so it never shows as granted or taken away.
//!
//! The model of an agent is not an override (it lives in `agent_model` of
//! `system.db`), but its assignment and its removal are recorded the same way,
//! under the field [`MODEL_FIELD`], so the history of an agent reads every
//! change made on its sheet.

use apollia_tools::{AgentRepository, AgentRepositoryError, OverrideField, ResolvedAgent};
use serde_json::Value;

use crate::governance::{
    AccessChange, GovernanceContract, GovernanceError, GovernanceEvent, OverrideChange,
};

/// Errors of an override write.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AgentOverrideError {
    /// The agent store refused or failed the write.
    #[error(transparent)]
    Store(#[from] AgentRepositoryError),
    /// The write was stored but its journal entry could not be built.
    #[error(transparent)]
    Governance(#[from] GovernanceError),
}

/// The store and the governance an override write goes through, and who
/// makes it.
#[derive(Clone, Copy)]
pub struct OverrideWriter<'a> {
    /// The agent store.
    pub repo: &'a AgentRepository,
    /// The governance that records each write.
    pub governance: &'a dyn GovernanceContract,
    /// Who makes the change, as the journal names it.
    pub actor: &'a str,
}

impl OverrideWriter<'_> {
    /// Sets the user's `value` of `field` for `agent`, and records it.
    ///
    /// # Errors
    ///
    /// [`AgentOverrideError::Store`] when the agent is unknown, the value does
    /// not fit the field, or the store fails; [`AgentOverrideError::Governance`]
    /// when the record cannot be written.
    pub fn set(
        &self,
        agent: &str,
        field: OverrideField,
        value: &Value,
    ) -> Result<(), AgentOverrideError> {
        let before = access_of(self.repo, agent)?;
        self.repo.set_override(agent, field, value)?;
        self.record_change(agent, field, OverrideChange::Set)?;
        self.record_access_diff(agent, &before, &access_of(self.repo, agent)?)
    }

    /// Sets an access of several agents at once, each `(agent, field,
    /// tools)` of `writes`, the field being [`OverrideField::Access`] (the
    /// list in force) or [`OverrideField::AccessAdded`] (the tools on top of
    /// the default access), and records each agent's change: its override,
    /// then one [`GovernanceEvent::AccessChanged`] per tool granted or taken
    /// away.
    ///
    /// The lists are stored in one transaction, so a refusal stores none and
    /// records nothing.
    ///
    /// # Errors
    ///
    /// As [`OverrideWriter::set`].
    pub fn set_access_of_many(
        &self,
        writes: &[(String, OverrideField, Vec<String>)],
    ) -> Result<(), AgentOverrideError> {
        let before = writes
            .iter()
            .map(|(agent, _, _)| access_of(self.repo, agent))
            .collect::<Result<Vec<_>, _>>()?;
        let values: Vec<(String, OverrideField, Value)> = writes
            .iter()
            .map(|(agent, field, tools)| (agent.clone(), *field, Value::from(tools.clone())))
            .collect();
        self.repo.set_overrides(&values)?;
        for ((agent, field, _), before) in writes.iter().zip(&before) {
            self.record_change(agent, *field, OverrideChange::Set)?;
            self.record_access_diff(agent, before, &access_of(self.repo, agent)?)?;
        }
        Ok(())
    }

    /// Sets lists of tools of several agents at once, each `(agent, field,
    /// tools)` of `writes`, and records each agent's change: its override,
    /// then one [`GovernanceEvent::AccessChanged`] per tool granted or taken
    /// away.
    ///
    /// The lists of one field are stored in one transaction, so a refusal
    /// stores none of them and records nothing.
    ///
    /// # Errors
    ///
    /// As [`OverrideWriter::set`].
    pub fn set_tool_lists_of_many(
        &self,
        writes: &[(String, OverrideField, Vec<String>)],
    ) -> Result<(), AgentOverrideError> {
        let mut agents: Vec<&str> = Vec::new();
        for (agent, _, _) in writes {
            if !agents.contains(&agent.as_str()) {
                agents.push(agent);
            }
        }
        let before = agents
            .iter()
            .map(|agent| access_of(self.repo, agent))
            .collect::<Result<Vec<_>, _>>()?;
        let mut fields: Vec<OverrideField> = writes.iter().map(|(_, field, _)| *field).collect();
        fields.sort();
        fields.dedup();
        for field in &fields {
            let values: Vec<(String, Value)> = writes
                .iter()
                .filter(|(_, written, _)| written == field)
                .map(|(agent, _, tools)| (agent.clone(), Value::from(tools.clone())))
                .collect();
            self.repo.set_overrides(*field, &values)?;
        }
        for (agent, before) in agents.iter().zip(&before) {
            for (_, field, _) in writes.iter().filter(|(written, _, _)| written == agent) {
                self.record_change(agent, *field, OverrideChange::Set)?;
            }
            self.record_access_diff(agent, before, &access_of(self.repo, agent)?)?;
        }
        Ok(())
    }

    /// Removes the user's value of `field` for `agent`, and records it.
    /// Removing a field that was not overridden changes nothing and records
    /// nothing.
    ///
    /// Returns whether an override was removed.
    ///
    /// # Errors
    ///
    /// As [`OverrideWriter::set`].
    pub fn remove(&self, agent: &str, field: OverrideField) -> Result<bool, AgentOverrideError> {
        let before = access_of(self.repo, agent)?;
        if self.repo.remove_override(agent, field)?.is_none() {
            return Ok(false);
        }
        self.record_change(agent, field, OverrideChange::Removed)?;
        self.record_access_diff(agent, &before, &access_of(self.repo, agent)?)?;
        Ok(true)
    }

    /// Back to defaults: removes every override of `agent`, recording each.
    ///
    /// Returns the fields that were overridden.
    ///
    /// # Errors
    ///
    /// As [`OverrideWriter::set`].
    pub fn reset(&self, agent: &str) -> Result<Vec<OverrideField>, AgentOverrideError> {
        let before = access_of(self.repo, agent)?;
        let removed = self.repo.reset_overrides(agent)?;
        for entry in &removed {
            self.record_change(agent, entry.field, OverrideChange::Removed)?;
        }
        self.record_access_diff(agent, &before, &access_of(self.repo, agent)?)?;
        Ok(removed.into_iter().map(|entry| entry.field).collect())
    }

    fn record_change(
        &self,
        agent: &str,
        field: OverrideField,
        change: OverrideChange,
    ) -> Result<(), GovernanceError> {
        self.governance.record(&GovernanceEvent::OverrideChanged {
            agent_id: agent.to_owned(),
            field: field.as_str().to_owned(),
            change,
            actor: self.actor.to_owned(),
        })
    }

    fn record_access_diff(
        &self,
        agent: &str,
        before: &[String],
        after: &[String],
    ) -> Result<(), AgentOverrideError> {
        let granted = after
            .iter()
            .filter(|a| !before.contains(a))
            .map(|a| (a, AccessChange::Granted));
        let revoked = before
            .iter()
            .filter(|a| !after.contains(a))
            .map(|a| (a, AccessChange::Revoked));
        for (access_id, change) in granted.chain(revoked) {
            self.governance.record(&GovernanceEvent::AccessChanged {
                agent_id: agent.to_owned(),
                access_id: access_id.clone(),
                change,
                actor: self.actor.to_owned(),
                reason: None,
            })?;
        }
        Ok(())
    }
}

/// Field the journal names for the model assigned to an agent.
pub const MODEL_FIELD: &str = "model";

/// Records that the model assigned to `agent` was set or removed by `actor`.
///
/// # Errors
///
/// [`GovernanceError`] when the record cannot be written.
pub fn record_model_change(
    governance: &dyn GovernanceContract,
    agent: &str,
    change: OverrideChange,
    actor: &str,
) -> Result<(), GovernanceError> {
    governance.record(&GovernanceEvent::OverrideChanged {
        agent_id: agent.to_owned(),
        field: MODEL_FIELD.to_owned(),
        change,
        actor: actor.to_owned(),
    })
}

/// The access the store holds for `agent`: its list, or the tools given on
/// top of the default access for an agent that holds it.
fn access_of(repo: &AgentRepository, agent: &str) -> Result<Vec<String>, AgentRepositoryError> {
    repo.resolved(agent, None, super::global_default_autonomy())?
        .map(|resolved: ResolvedAgent| {
            if apollia_tools::reads_by_default(&resolved) {
                resolved.access_added
            } else {
                resolved.access
            }
        })
        .ok_or_else(|| AgentRepositoryError::NotFound(agent.to_owned()))
}

#[cfg(test)]
mod tests;
