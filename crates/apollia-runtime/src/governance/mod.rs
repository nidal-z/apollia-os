//! Governance contract expected by the agent side.
//!
//! The agent side asks governance four questions and never decides them
//! itself: which accesses an agent effectively holds (and, for each, what it
//! permits and whether it is sensitive), what to do with one access at the
//! moment it is used, whether an installation requires a signed package, and
//! where to record a change of access, an installation, an update, or an
//! uninstallation.
//!
//! [`GovernanceContract`] carries those four calls. [`GovernanceHandle`] is
//! the implementation the runtime runs on: an actor owns what decisions read
//! (the organisation policy, the fine rules and the standing consents of
//! `governance.db`, the developer mode) and publishes it as one
//! [`GovernanceState`]; [`decide`] answers a tool call from that state in a
//! fixed order, with its reason and the answers offered. A record lands in the
//! hash-chained audit journal next to the run entries, chained and signed like
//! them. The `[packages]` section of the policy answers whether a signature is
//! required.
//!
//! Connections answer to the same governance. [`tool_attributes`] describes
//! every tool by what governance reads (family, connection, provenance,
//! remote, local program); [`admit`] is the admission call that precedes the
//! addition of a connection, failing closed; [`ConnectionGovernance`] carries
//! that call and the journal of connection events to the places that add,
//! change or remove a connection. [`PolicyAdmission`] answers that call from
//! the organisation policy read at boot, its `[connections]` lists naming a
//! connection by the identifiers of [`AdmissionRequest`]; at every boot,
//! [`ForbiddenConnections`] submits the existing connections to the same call
//! and keeps out the ones it now refuses.
//!
//! [`access_upgrade`] runs, once at startup, the conversion of the rules
//! persisted before standing consents, and journals every change it makes.

pub mod access_upgrade;
mod actor;
mod admission;
mod connection;
mod contract;
mod decision;
mod forbidden;
#[cfg(test)]
mod no_policy;
pub(crate) mod policy_admission;
mod record;
mod tool_attributes;

pub use admission::{
    admit, AdmissionDecision, AdmissionRequest, ConnectionAdmission, NoPolicyAdmission,
    PolicyReadError,
};
pub use connection::{connection_run_id, ConnectionEvent, ConnectionGovernance, ConnectionRecord};

pub use actor::{GovernanceHandle, GovernanceSources, GovernanceWriteError};
pub use contract::{
    AccessCall, AccessDecision, ConsentAnswer, DecisionReason, DeclaredAccess, EffectiveAccess,
    GovernanceContract, GovernanceError, InstallSubject, ALL_ANSWERS, NO_ALWAYS_ANSWERS,
};
pub use decision::{decide, GovernanceState, PolicyRefusals};
pub use forbidden::{mailbox_refusal, ForbiddenConnections, FORBIDDEN_CONNECTION_REASON};
#[cfg(test)]
pub use no_policy::NoPolicyGovernance;
pub use policy_admission::{
    PolicyAdmission, CONNECTION_DENIED_REASON, CONNECTION_NOT_ALLOWED_REASON,
    POLICY_REFUSAL_PREFIX, REMOTE_DENIED_REASON,
};
pub use record::{
    governance_run_id, package_run_id, AccessChange, GovernanceEvent, GovernanceRecord,
    OverrideChange,
};
pub use tool_attributes::{tool_attributes, ToolAttributes};
