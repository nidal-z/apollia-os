//! Governance that fails closed.
//!
//! When what governance decides from cannot be read, a failure never becomes
//! a permission. Two causes put governance in that state: `governance.db`
//! exists and cannot be read, or an organisation policy is deposited and
//! refused (its schema, its signature, its key or its owner). While it lasts,
//! every sensitive tool call and every new connection is refused; reading
//! continues.
//!
//! A database that stops being readable keeps the last state read from it in
//! force (its rules, its consents, its disabled tools) on top of the refusal;
//! a database that cannot be read at startup has no state to keep, and only
//! the operator's `[tools] disabled` list applies.
//!
//! [`ToolGate`] is what a run that builds its own tool dispatcher reads: the
//! tools switched off, and whether governance fails closed.

use std::path::Path;

use apollia_permissions::PolicyVerdict;
use apollia_tools::ActionFamily;
use serde::{Deserialize, Serialize};

use crate::governance::actor::GovernanceHandle;

/// Why governance fails closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "cause", rename_all = "snake_case")]
pub enum FailClosed {
    /// `governance.db` exists and cannot be read.
    StoreUnreadable {
        /// What the read failed on.
        detail: String,
    },
    /// An organisation policy is deposited and refused.
    PolicyInvalid {
        /// The reason class: `schema`, `signature`, `key` or `owner`.
        reason: String,
        /// What exactly was refused.
        detail: String,
    },
}

impl FailClosed {
    /// The cause, as a stable lowercase name.
    #[must_use]
    pub fn cause(&self) -> &'static str {
        match self {
            Self::StoreUnreadable { .. } => "store_unreadable",
            Self::PolicyInvalid { .. } => "policy_invalid",
        }
    }

    /// What could not be read, in one sentence.
    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::StoreUnreadable { detail } => {
                format!("the governance database cannot be read: {detail}")
            }
            Self::PolicyInvalid { reason, detail } => {
                format!("the organisation policy is refused ({reason}): {detail}")
            }
        }
    }

    /// The state a refused policy puts governance in, `None` for a policy
    /// absent or valid.
    #[must_use]
    pub fn from_policy(verdict: &PolicyVerdict) -> Option<Self> {
        match verdict {
            PolicyVerdict::Invalid(invalid) => Some(Self::PolicyInvalid {
                reason: invalid.reason.as_str().to_owned(),
                detail: invalid.detail.clone(),
            }),
            PolicyVerdict::Absent | PolicyVerdict::Valid(_) => None,
        }
    }
}

/// Whether a tool of `family` is refused while governance fails closed: the
/// four sensitive families and a tool nobody classified. Reading, and acting
/// on the conversation alone, continue.
#[must_use]
pub fn refused_when_fail_closed(family: ActionFamily) -> bool {
    !matches!(family, ActionFamily::Read | ActionFamily::Exempt)
}

/// What a run that builds its own tool dispatcher reads from governance.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolGate {
    /// The tools switched off: the operator's `[tools] disabled` list and the
    /// tools disabled in `governance.db`.
    pub disabled_tools: Vec<String>,
    /// Set while governance fails closed: the run refuses every sensitive
    /// tool call.
    pub fail_closed: Option<FailClosed>,
}

/// The gate of a run starting now. `governance` is the actor of the runtime
/// the run belongs to; without it, `governance.db` is read from `data_dir`, and
/// a database that cannot be read fails closed with only `static_disabled`
/// switched off.
#[must_use]
pub fn tool_gate(
    governance: Option<&GovernanceHandle>,
    data_dir: &Path,
    static_disabled: &[String],
) -> ToolGate {
    let (mut disabled_tools, fail_closed) = match governance {
        Some(handle) => {
            let state = handle.state();
            (state.disabled_tools.clone(), state.fail_closed.clone())
        }
        None => match apollia_tools::load_governance_snapshot(data_dir) {
            Ok(snapshot) => (snapshot.disabled_tools, None),
            Err(e) => (
                Vec::new(),
                Some(FailClosed::StoreUnreadable {
                    detail: e.to_string(),
                }),
            ),
        },
    };
    for name in static_disabled {
        if !disabled_tools.contains(name) {
            disabled_tools.push(name.clone());
        }
    }
    if let Some(state) = &fail_closed {
        tracing::event!(
            tracing::Level::WARN,
            cause = state.cause(),
            detail = %state.detail(),
            "governance.fail_closed.gate"
        );
    }
    ToolGate {
        disabled_tools,
        fail_closed,
    }
}

#[cfg(test)]
mod tests;
