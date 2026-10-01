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
//! [`access_upgrade`] runs, once at startup, the conversion of the rules
//! persisted before standing consents, and journals every change it makes.

pub mod access_upgrade;
mod contract;
mod no_policy;
mod record;

pub use contract::{
    AccessCall, AccessDecision, DeclaredAccess, EffectiveAccess, GovernanceContract,
    GovernanceError, InstallSubject,
};
pub use no_policy::NoPolicyGovernance;
pub use record::{governance_run_id, AccessChange, GovernanceEvent, GovernanceRecord};
