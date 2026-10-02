//! Governance contract expected by the agent side.
//!
//! The agent side asks governance four questions and never decides them
//! itself: which accesses an agent effectively holds (and, for each, what it
//! permits and whether it is sensitive), what to do with one access at the
//! moment it is used, whether an installation requires a signed package, and
//! where to record a change of access, an installation, an update, or an
//! uninstallation.
//!
//! [`GovernanceContract`] carries those four calls. [`NoPolicyGovernance`] is
//! the implementation in force when no policy is deposited: everything is
//! allowed, an access is sensitive only when its tool declares it so, no
//! signature is required, and every record lands in the hash-chained audit
//! journal next to the run entries, chained and signed like them.
//!
//! Connections answer to the same governance. [`tool_attributes`] describes
//! every tool by what governance reads (family, connection, provenance,
//! remote, local program); [`admit`] is the admission call that precedes the
//! addition of a connection, failing closed; [`ConnectionGovernance`] carries
//! that call and the journal of connection events to the places that add,
//! change or remove a connection.

mod admission;
mod connection;
mod contract;
mod no_policy;
mod record;
mod tool_attributes;

pub use admission::{
    admit, AdmissionDecision, AdmissionRequest, ConnectionAdmission, NoPolicyAdmission,
    PolicyReadError,
};
pub use connection::{connection_run_id, ConnectionEvent, ConnectionGovernance, ConnectionRecord};

pub use contract::{
    AccessCall, AccessDecision, DeclaredAccess, EffectiveAccess, GovernanceContract,
    GovernanceError, InstallSubject,
};
pub use no_policy::NoPolicyGovernance;
pub use record::{
    governance_run_id, AccessChange, GovernanceEvent, GovernanceRecord, OverrideChange,
};
pub use tool_attributes::{tool_attributes, ToolAttributes};
